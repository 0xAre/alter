//! TUI layer (ratatui + crossterm).
//!
//! Alur layar:
//!   Splash  →  Unlock / Create  →  Init  →  Main (kontak + room + chat)

mod ui;

use zeroize::Zeroize;

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::contacts::{self, Contact};
use crate::error::Error;
use crate::identity::keypair::KeyBundle;
use crate::identity::pm::PmEntry;
use crate::identity::vault;
use crate::platform;
use crate::session::{self, SessionEvent, SessionState};
use crate::transport::obfs4::Obfs4Status;
use crate::transport::tor::TorContext;
use crate::transport::{self, LanMode};

/// Material identitas milik sendiri (tersedia setelah unlock).
///
/// `noise_sk` dan `tor_client_auth_secret` adalah secret — ZeroizeOnDrop (SEC-04).
#[derive(zeroize::ZeroizeOnDrop)]
pub struct SelfKeys {
    #[zeroize(skip)]
    pub fingerprint: String,
    pub noise_sk: [u8; 32],
    #[zeroize(skip)]
    pub noise_pub: [u8; 32],
    #[zeroize(skip)]
    pub ed25519_pub: [u8; 32],
    #[zeroize(skip)]
    pub invite: String,
    /// x25519 pubkey client auth (SEC-13) — dibagikan lewat invite v2.
    #[zeroize(skip)]
    pub tor_client_auth_pub: [u8; 32],
    /// x25519 secret seed untuk decrypt restricted descriptor peer.
    pub tor_client_auth_secret: [u8; 32],
}

#[derive(Clone, Copy)]
pub enum ConnectKind {
    Auto,
    Listen(u16),
    Dial(SocketAddr),
}

impl From<ConnectKind> for LanMode {
    fn from(k: ConnectKind) -> Self {
        match k {
            ConnectKind::Auto => LanMode::Auto,
            ConnectKind::Listen(p) => LanMode::Listen(p),
            ConnectKind::Dial(a) => LanMode::Dial(a),
        }
    }
}

/// Fase pembuatan vault v2 baru (4-langkah karena butuh 2 passphrase).
#[derive(PartialEq, Eq)]
pub(crate) enum CreatePhase {
    AlterPass,    // Langkah 1: masukkan passphrase ALTER
    AlterConfirm, // Langkah 2: konfirmasi passphrase ALTER
    PmPass,       // Langkah 3: masukkan passphrase PM (decoy)
    PmConfirm,    // Langkah 4: konfirmasi passphrase PM
}

#[derive(PartialEq, Eq)]
pub(crate) enum Screen {
    Splash,
    Unlock,
    Create,
    Init,
    Onboard,
    Main,
    /// M6: mode Password Manager (passphrase A cocok atau EmptyPm)
    PmMain,
    /// M6: form tambah entri PM baru
    PmAdd,
    /// M6: vault v1 terdeteksi → tampilkan prompt migrasi ke v2
    Migrate,
}

#[derive(PartialEq, Eq)]
pub(crate) enum Mode {
    Browsing,
    AddContact,
    /// UX-01: ganti nama kontak yang dipilih.
    RenameContact,
    InRoom,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomState {
    None,
    Connecting,
    Handshaking,
    Open,
    PeerLeft,
    Closed,
}

pub(crate) enum Who {
    Me,
    Peer,
    System,
}

pub(crate) struct ChatLine {
    pub who: Who,
    pub text: String,
}

impl ChatLine {
    fn me(text: String) -> Self {
        Self { who: Who::Me, text }
    }
    fn peer(text: String) -> Self {
        Self { who: Who::Peer, text }
    }
    fn system(text: String) -> Self {
        Self { who: Who::System, text }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotifLevel {
    Error,
    Warn,
    Success,
    Info,
}

pub(crate) struct Notification {
    pub level: NotifLevel,
    pub text: String,
    pub dismiss_at: Option<u64>,
}

impl Notification {
    pub fn error(text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Error, text: text.into(), dismiss_at: None }
    }
    pub fn warn(text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Warn, text: text.into(), dismiss_at: None }
    }
    pub fn success(tick: u64, text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Success, text: text.into(), dismiss_at: Some(tick + 30) }
    }
    pub fn info(tick: u64, text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Info, text: text.into(), dismiss_at: Some(tick + 40) }
    }
}

pub(crate) struct App {
    pub keys: Option<SelfKeys>,
    pub vault_path: PathBuf,
    pub tor: Option<Arc<TorContext>>,
    pub tor_connecting: bool,
    /// SEC-13: true saat onion service di-restart untuk update restricted discovery.
    pub tor_restarting: bool,
    /// Sender untuk menerima hasil restart service dari background task.
    tor_restart_result_tx: mpsc::UnboundedSender<Result<Arc<TorContext>, String>>,
    pub connect_kind: ConnectKind,

    pub screen: Screen,
    pub splash_ticks: u64,
    pub tick_count: u64,
    pub init_step: u8,
    pub init_start_tick: u64,

    pub pass_input: String,
    pub pass_confirm: String,
    /// M6: passphrase PM (slot A) — untuk Create multi-phase dan Migrate.
    pub pm_pass_input: String,
    pub pm_pass_confirm: String,
    /// M6: fase saat ini dalam Create flow.
    pub create_phase: CreatePhase,
    pub auth_error: Option<String>,

    pub contacts: Vec<Contact>,
    pub selected: usize,
    pub mode: Mode,
    pub room: RoomState,
    pub peer_name: Option<String>,
    pub messages: Vec<ChatLine>,
    pub input: String,
    pub add_buffer: String,
    /// UX-01: buffer untuk rename kontak.
    pub rename_buffer: String,
    pub notification: Option<Notification>,
    pub show_invite: bool,
    pub conn_task: Option<tokio::task::JoinHandle<()>>,
    pub contacts_key: Option<[u8; 32]>,
    pub pending_delete: Option<usize>,
    pub show_onboard_after_init: bool,
    pub obfs4_status: Obfs4Status,
    pub panic_armed_tick: Option<u64>,
    pub should_panic_wipe: bool,

    // ─── M6: Password Manager state ──────────────────────────────────────
    /// Entri PM yang sedang aktif di memori (di-zeroize saat panic wipe).
    pub pm_entries: Vec<PmEntry>,
    pub pm_selected: usize,
    /// key_a (kunci slot A) — None jika EmptyPm (mode read-only).
    pub pm_key: Option<zeroize::Zeroizing<[u8; 32]>>,
    /// Bytes vault v2 saat ini (untuk update slot A tanpa re-seal slot B).
    pub pm_vault_bytes: Option<Box<[u8; vault::VAULT_V2_SIZE]>>,
    /// True jika passphrase tidak dikenal — PM kosong tidak bisa di-edit.
    pub pm_is_readonly: bool,
    /// Tick saat password di-reveal untuk auto-hide 5 detik (50 tick).
    pub pm_reveal_tick: Option<u64>,
    /// Entri yang menunggu konfirmasi hapus.
    pub pm_pending_delete: Option<usize>,
    /// Filter pencarian — kosong = tampilkan semua.
    pub pm_search: String,
    pub pm_search_active: bool,
    // Form tambah entri baru
    pub pm_add_service: String,
    pub pm_add_username: String,
    pub pm_add_password: String,
    /// Field aktif di form PmAdd: 0=service, 1=username, 2=password.
    pub pm_add_field: u8,

    // ─── M6: Migrasi vault v1 → v2 ───────────────────────────────────────
    /// Bundle sementara selama migrasi (dibuang setelah migrasi selesai).
    pub migration_bundle: Option<KeyBundle>,
    /// Fase migrasi: 0=masukkan PM pass, 1=konfirmasi PM pass.
    pub migration_phase: u8,
}

impl App {
    fn new(
        vault_path: PathBuf,
        vault_exists: bool,
        connect_kind: ConnectKind,
        contacts: Vec<Contact>,
        tor_restart_result_tx: mpsc::UnboundedSender<Result<Arc<TorContext>, String>>,
    ) -> Self {
        let screen = if vault_exists { Screen::Unlock } else { Screen::Create };
        Self {
            keys: None,
            vault_path,
            tor: None,
            tor_connecting: false,
            tor_restarting: false,
            tor_restart_result_tx,
            connect_kind,
            screen,
            splash_ticks: 0,
            tick_count: 0,
            init_step: 0,
            init_start_tick: 0,
            pass_input: String::new(),
            pass_confirm: String::new(),
            pm_pass_input: String::new(),
            pm_pass_confirm: String::new(),
            create_phase: CreatePhase::AlterPass,
            auth_error: None,
            contacts,
            selected: 0,
            mode: Mode::Browsing,
            room: RoomState::None,
            peer_name: None,
            messages: Vec::new(),
            input: String::new(),
            add_buffer: String::new(),
            rename_buffer: String::new(),
            notification: None,
            show_invite: false,
            conn_task: None,
            contacts_key: None,
            pending_delete: None,
            show_onboard_after_init: false,
            obfs4_status: crate::transport::obfs4::detect(),
            panic_armed_tick: None,
            should_panic_wipe: false,
            pm_entries: Vec::new(),
            pm_selected: 0,
            pm_key: None,
            pm_vault_bytes: None,
            pm_is_readonly: false,
            pm_reveal_tick: None,
            pm_pending_delete: None,
            pm_search: String::new(),
            pm_search_active: false,
            pm_add_service: String::new(),
            pm_add_username: String::new(),
            pm_add_password: String::new(),
            pm_add_field: 0,
            migration_bundle: None,
            migration_phase: 0,
        }
    }

    pub fn tor_active(&self) -> bool {
        self.tor.is_some()
    }

    fn set_notif_error(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::error(text));
    }
    fn set_notif_success(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::success(self.tick_count, text));
    }
    fn set_notif_info(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::info(self.tick_count, text));
    }
    fn set_notif_warn(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::warn(text));
    }
}

fn build_self_keys(bundle: &KeyBundle, onion: Option<&str>) -> SelfKeys {
    let ed_pub = bundle.identity.public_key().to_bytes();
    let noise_pub = bundle.noise.public_bytes();
    let noise_sk = bundle.noise.secret_bytes();
    let cap_pub = contacts::derive_tor_client_auth_pub(bundle);
    let cap_secret = contacts::derive_tor_client_auth_secret_seed(bundle);
    let invite = contacts::encode_invite(&ed_pub, &noise_pub, &cap_pub, onion);
    let mut sk = SelfKeys {
        fingerprint: contacts::fingerprint(&ed_pub),
        noise_sk,
        noise_pub,
        ed25519_pub: ed_pub,
        invite,
        tor_client_auth_pub: cap_pub,
        tor_client_auth_secret: cap_secret,
    };
    // SEC-04: kunci secret ke RAM agar tidak di-swap ke disk.
    platform::try_mlock(sk.noise_sk.as_mut_ptr(), 32);
    platform::try_mlock(sk.tor_client_auth_secret.as_mut_ptr(), 32);
    sk
}

/// Bangun ulang invite code menyertakan onion address dan client_auth_pub bila siap.
fn refresh_invite(app: &mut App) {
    let onion = app.tor.as_ref().map(|t| t.onion_address.clone());
    if let Some(k) = app.keys.as_mut() {
        k.invite =
            contacts::encode_invite(&k.ed25519_pub, &k.noise_pub, &k.tor_client_auth_pub, onion.as_deref());
    }
}

const SPLASH_TICKS: u64 = 12;

pub async fn run(
    vault_path: PathBuf,
    vault_exists: bool,
    connect_kind: ConnectKind,
    contacts: Vec<Contact>,
    mut tor_rx: Option<mpsc::UnboundedReceiver<Result<Arc<TorContext>, String>>>,
) -> Result<(), Error> {
    // Channel untuk menerima hasil restart service dari background task (SEC-13).
    let (restart_tx, mut restart_rx) =
        mpsc::unbounded_channel::<Result<Arc<TorContext>, String>>();

    let mut app = App::new(vault_path, vault_exists, connect_kind, contacts, restart_tx);
    app.screen = Screen::Splash;

    if tor_rx.is_some() {
        app.tor_connecting = true;
        app.set_notif_info("Menyambung ke Tor di latar belakang (~30-60 dtk)…");
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<KeyEvent>();
    spawn_input_thread(input_tx);

    let mut out_tx: Option<mpsc::UnboundedSender<String>> = None;
    let mut ev_rx: Option<mpsc::UnboundedReceiver<SessionEvent>> = None;

    let mut tick = tokio::time::interval(Duration::from_millis(100));

    let result = loop {
        if let Err(e) = terminal.draw(|f| ui::render(f, &app)) {
            break Err(Error::from(e));
        }

        tokio::select! {
            maybe_key = input_rx.recv() => {
                match maybe_key {
                    Some(key) => {
                        if handle_key(&mut app, &mut out_tx, &mut ev_rx, key) {
                            break Ok(());
                        }
                    }
                    None => break Ok(()),
                }
            }
            maybe_ev = recv_session(&mut ev_rx) => {
                match maybe_ev {
                    Some(se) => handle_session_event(&mut app, se),
                    None => {
                        ev_rx = None;
                        out_tx = None;
                    }
                }
            }
            maybe_tor = recv_tor(&mut tor_rx) => {
                match maybe_tor {
                    Some(Ok(ctx)) => {
                        app.tor = Some(ctx);
                        refresh_invite(&mut app);
                        app.set_notif_success("Tor siap — sekarang online (LAN + Tor).");
                        // SEC-13: inject auth keys + restart service bila kontak sudah ada.
                        inject_all_client_auth_keys(&app);
                        let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
                        if has_restricted {
                            trigger_tor_restart(&mut app);
                        }
                    }
                    Some(Err(e)) => {
                        app.set_notif_warn(format!("Tor gagal: {e}. Jalan mode LAN saja."));
                    }
                    None => {}
                }
                app.tor_connecting = false;
                tor_rx = None;
            }
            maybe_restart = restart_rx.recv() => {
                if let Some(res) = maybe_restart {
                    match res {
                        Ok(new_ctx) => {
                            app.tor = Some(new_ctx);
                            refresh_invite(&mut app);
                            app.tor_restarting = false;
                            app.set_notif_success("Restricted discovery diaktifkan.");
                        }
                        Err(e) => {
                            app.tor_restarting = false;
                            app.set_notif_warn(
                                format!("Tor service restart gagal: {e}. Restricted discovery tidak aktif.")
                            );
                        }
                    }
                }
            }
            _ = tick.tick() => {
                app.tick_count += 1;

                if app.screen == Screen::Splash {
                    app.splash_ticks += 1;
                    if app.splash_ticks >= SPLASH_TICKS {
                        let vault_exists = app.vault_path.exists();
                        app.screen = if vault_exists { Screen::Unlock } else { Screen::Create };
                    }
                }

                // M6: auto-hide password setelah 5 detik (50 tick × 100ms)
                if let Some(reveal_tick) = app.pm_reveal_tick {
                    if app.tick_count.saturating_sub(reveal_tick) >= 50 {
                        app.pm_reveal_tick = None;
                    }
                }

                if app.screen == Screen::Init {
                    let elapsed = app.tick_count.saturating_sub(app.init_start_tick);
                    app.init_step = match elapsed {
                        0..=2  => 1,
                        3..=5  => 2,
                        6..=8  => 3,
                        _      => 4,
                    };
                    if elapsed >= 14 {
                        if app.show_onboard_after_init {
                            app.screen = Screen::Onboard;
                        } else {
                            app.screen = Screen::Main;
                        }
                    }
                }

                if let Some(n) = &app.notification {
                    if let Some(dismiss_at) = n.dismiss_at {
                        if app.tick_count >= dismiss_at {
                            app.notification = None;
                        }
                    }
                }

                if let Some(armed_tick) = app.panic_armed_tick {
                    if app.tick_count.saturating_sub(armed_tick) > 30 {
                        app.panic_armed_tick = None;
                        if matches!(&app.notification, Some(n) if n.text.contains("PANIC")) {
                            app.notification = None;
                        }
                    }
                }
            }
        }
    };

    if app.should_panic_wipe {
        if let Some(h) = app.conn_task.take() {
            h.abort();
        }
        app.tor = None;
        app.keys = None;
        app.pass_input.zeroize();
        app.pass_confirm.zeroize();
        app.pm_pass_input.zeroize();
        app.pm_pass_confirm.zeroize();
        app.messages.clear();
        app.input.clear();
        app.add_buffer.clear();
        app.rename_buffer.clear();
        // M6: wipe PM data sensitif
        for entry in &mut app.pm_entries {
            entry.password.zeroize();
            entry.username.zeroize();
            entry.service.zeroize();
        }
        app.pm_entries.clear();
        app.pm_key = None;
        app.pm_vault_bytes = None;
        app.pm_add_password.zeroize();
        app.pm_add_service.clear();
        app.pm_add_username.clear();
        app.migration_bundle = None;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn recv_session(
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) -> Option<SessionEvent> {
    match ev_rx.as_mut() {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn recv_tor(
    rx: &mut Option<mpsc::UnboundedReceiver<Result<Arc<TorContext>, String>>>,
) -> Option<Result<Arc<TorContext>, String>> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

fn spawn_input_thread(tx: mpsc::UnboundedSender<KeyEvent>) {
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                if tx.send(k).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });
}

fn handle_panic_hotkey(app: &mut App) -> bool {
    if let Some(armed_tick) = app.panic_armed_tick {
        if app.tick_count.saturating_sub(armed_tick) <= 30 {
            app.should_panic_wipe = true;
            return true;
        }
    }
    app.panic_armed_tick = Some(app.tick_count);
    app.notification = Some(Notification {
        level: NotifLevel::Error,
        text: "⚠ PANIC — tekan Ctrl+Shift+X lagi dalam 3 detik untuk wipe & exit.".into(),
        dismiss_at: None,
    });
    false
}

fn handle_key(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
    key: KeyEvent,
) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('X') {
        return handle_panic_hotkey(app);
    }

    match app.screen {
        Screen::Splash => {
            let vault_exists = app.vault_path.exists();
            app.screen = if vault_exists { Screen::Unlock } else { Screen::Create };
            false
        }
        Screen::Unlock => handle_unlock_key(app, key),
        Screen::Create => handle_create_key(app, key),
        Screen::Init => handle_init_key(app, key),
        Screen::Onboard => {
            app.screen = Screen::Main;
            false
        }
        Screen::Main => handle_main_key(app, out_tx, ev_rx, key),
        Screen::PmMain => handle_pm_main_key(app, key),
        Screen::PmAdd => handle_pm_add_key(app, key),
        Screen::Migrate => handle_migrate_key(app, key),
    }
}

fn handle_unlock_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter => {
            match try_unlock(app) {
                UnlockResult::AlterOk => {
                    app.pass_input.zeroize();
                    app.init_step = 1;
                    app.init_start_tick = app.tick_count;
                    app.screen = Screen::Init;
                }
                UnlockResult::PmOk => {
                    app.pass_input.zeroize();
                    app.screen = Screen::PmMain;
                }
                UnlockResult::NeedsMigration => {
                    // JANGAN zeroize pass_input — akan dipakai sebagai passphrase B saat migrasi
                    app.auth_error = None;
                    app.migration_phase = 0;
                    app.screen = Screen::Migrate;
                }
                UnlockResult::Error(msg) => {
                    app.pass_input.zeroize();
                    app.auth_error = Some(msg);
                }
            }
        }
        KeyCode::Backspace => { app.pass_input.pop(); }
        KeyCode::Char(c) => app.pass_input.push(c),
        _ => {}
    }
    false
}

/// Hasil dari `try_unlock` — menggantikan `bool` agar bisa bedakan 4 outcome.
enum UnlockResult {
    AlterOk,
    PmOk,
    NeedsMigration,
    Error(String),
}

fn try_unlock(app: &mut App) -> UnlockResult {
    let bytes = match vault::read_vault_raw(&app.vault_path) {
        Ok(b) => b,
        Err(_) => return UnlockResult::Error("Vault tidak terbaca.".into()),
    };

    match vault::detect_version(&bytes) {
        vault::VaultVersion::V1 => {
            let v1: [u8; vault::VAULT_SIZE] = match bytes.try_into() {
                Ok(v) => v,
                Err(_) => return UnlockResult::Error("Vault tidak valid.".into()),
            };
            match vault::unseal(&v1, app.pass_input.as_bytes()) {
                Ok(bundle) => {
                    // Simpan bundle sementara untuk dipakai saat migrasi
                    app.migration_bundle = Some(bundle);
                    UnlockResult::NeedsMigration
                }
                Err(_) => UnlockResult::Error("Passphrase salah atau vault rusak.".into()),
            }
        }

        vault::VaultVersion::V2 => {
            let v2: [u8; vault::VAULT_V2_SIZE] = match bytes.try_into() {
                Ok(v) => v,
                Err(_) => return UnlockResult::Error("Vault tidak valid.".into()),
            };
            match vault::open_v2(&v2, app.pass_input.as_bytes()) {
                vault::VaultOpenResult::AlterMode(bundle) => {
                    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
                    app.keys = Some(build_self_keys(&bundle, None));
                    refresh_invite(app);
                    app.auth_error = None;
                    load_contacts_into(app);
                    inject_all_client_auth_keys(app);
                    let has_restricted =
                        app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
                    if app.tor.is_some() && has_restricted {
                        trigger_tor_restart(app);
                    }
                    UnlockResult::AlterOk
                }

                vault::VaultOpenResult::PmMode { pm_entries, pm_key } => {
                    app.pm_entries = pm_entries;
                    let mut zk = zeroize::Zeroizing::new(pm_key);
                    platform::try_mlock((&mut *zk).as_mut_ptr(), 32);
                    app.pm_key = Some(zk);
                    app.pm_vault_bytes = Some(Box::new(v2));
                    app.pm_is_readonly = false;
                    UnlockResult::PmOk
                }

                vault::VaultOpenResult::EmptyPm => {
                    // Passphrase tidak dikenal → PM kosong, read-only
                    app.pm_entries = Vec::new();
                    app.pm_key = None;
                    app.pm_vault_bytes = Some(Box::new(v2));
                    app.pm_is_readonly = true;
                    UnlockResult::PmOk
                }
            }
        }

        vault::VaultVersion::Unknown => {
            UnlockResult::Error("Format vault tidak dikenal.".into())
        }
    }
}

fn handle_create_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Backspace => match app.create_phase {
            CreatePhase::AlterPass    => { app.pass_input.pop(); }
            CreatePhase::AlterConfirm => { app.pass_confirm.pop(); }
            CreatePhase::PmPass       => { app.pm_pass_input.pop(); }
            CreatePhase::PmConfirm    => { app.pm_pass_confirm.pop(); }
        },
        KeyCode::Char(c) => match app.create_phase {
            CreatePhase::AlterPass    => app.pass_input.push(c),
            CreatePhase::AlterConfirm => app.pass_confirm.push(c),
            CreatePhase::PmPass       => app.pm_pass_input.push(c),
            CreatePhase::PmConfirm    => app.pm_pass_confirm.push(c),
        },
        KeyCode::Enter => match app.create_phase {
            CreatePhase::AlterPass => {
                if app.pass_input.is_empty() {
                    app.auth_error = Some("Passphrase ALTER tidak boleh kosong.".into());
                } else {
                    app.auth_error = None;
                    app.create_phase = CreatePhase::AlterConfirm;
                }
            }
            CreatePhase::AlterConfirm => {
                if app.pass_confirm != app.pass_input {
                    app.auth_error = Some("Passphrase ALTER tidak cocok. Ulangi.".into());
                    app.pass_input.zeroize();
                    app.pass_confirm.zeroize();
                    app.create_phase = CreatePhase::AlterPass;
                } else {
                    app.pass_confirm.zeroize();
                    app.auth_error = None;
                    app.create_phase = CreatePhase::PmPass;
                }
            }
            CreatePhase::PmPass => {
                if app.pm_pass_input.is_empty() {
                    app.auth_error = Some("Passphrase PM tidak boleh kosong.".into());
                } else if app.pm_pass_input == app.pass_input {
                    app.auth_error = Some(
                        "Passphrase PM tidak boleh sama dengan passphrase ALTER.".into(),
                    );
                } else {
                    app.auth_error = None;
                    app.create_phase = CreatePhase::PmConfirm;
                }
            }
            CreatePhase::PmConfirm => {
                if app.pm_pass_confirm != app.pm_pass_input {
                    app.auth_error = Some("Passphrase PM tidak cocok. Ulangi.".into());
                    app.pm_pass_input.zeroize();
                    app.pm_pass_confirm.zeroize();
                    app.create_phase = CreatePhase::PmPass;
                } else {
                    match create_vault_v2(app) {
                        Ok(()) => {
                            app.pass_input.zeroize();
                            app.pass_confirm.zeroize();
                            app.pm_pass_input.zeroize();
                            app.pm_pass_confirm.zeroize();
                            app.auth_error = None;
                            app.create_phase = CreatePhase::AlterPass;
                            app.init_step = 1;
                            app.init_start_tick = app.tick_count;
                            app.screen = Screen::Init;
                            app.show_onboard_after_init = true;
                        }
                        Err(_) => {
                            app.auth_error = Some("Gagal membuat vault.".into());
                        }
                    }
                }
            }
        },
        _ => {}
    }
    false
}

fn handle_init_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter if app.init_step >= 4 => {
            app.screen = Screen::Main;
        }
        _ => {}
    }
    false
}


fn create_vault_v2(app: &mut App) -> Result<(), Error> {
    let bundle = KeyBundle::generate();
    let v2 = vault::create_v2(
        &bundle,
        app.pass_input.as_bytes(),
        app.pm_pass_input.as_bytes(),
    )?;
    vault::write_vault_v2(&app.vault_path, &v2)?;
    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
    app.keys = Some(build_self_keys(&bundle, None));
    refresh_invite(app);
    load_contacts_into(app);
    Ok(())
}

// ─── Migrasi vault v1 → v2 ────────────────────────────────────────────────

fn handle_migrate_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Backspace => {
            if app.migration_phase == 0 {
                app.pm_pass_input.pop();
            } else {
                app.pm_pass_confirm.pop();
            }
        }
        KeyCode::Char(c) => {
            if app.migration_phase == 0 {
                app.pm_pass_input.push(c);
            } else {
                app.pm_pass_confirm.push(c);
            }
        }
        KeyCode::Enter => {
            if app.migration_phase == 0 {
                if app.pm_pass_input.is_empty() {
                    app.auth_error = Some("Passphrase PM tidak boleh kosong.".into());
                } else if app.pm_pass_input == app.pass_input {
                    app.auth_error = Some(
                        "Passphrase PM tidak boleh sama dengan passphrase ALTER.".into(),
                    );
                } else {
                    app.auth_error = None;
                    app.migration_phase = 1;
                }
            } else {
                if app.pm_pass_confirm != app.pm_pass_input {
                    app.auth_error = Some("Passphrase tidak cocok. Ulangi.".into());
                    app.pm_pass_input.zeroize();
                    app.pm_pass_confirm.zeroize();
                    app.migration_phase = 0;
                } else {
                    do_migrate(app);
                }
            }
        }
        _ => {}
    }
    false
}

fn do_migrate(app: &mut App) {
    let bundle = match app.migration_bundle.take() {
        Some(b) => b,
        None => {
            app.auth_error = Some("State error: bundle tidak ada.".into());
            return;
        }
    };

    let v2 = match vault::create_v2(
        &bundle,
        app.pass_input.as_bytes(),
        app.pm_pass_input.as_bytes(),
    ) {
        Ok(v) => v,
        Err(_) => {
            app.auth_error = Some("Gagal membuat vault v2.".into());
            // Kembalikan bundle supaya bisa retry
            // (sudah di-take — jika retry butuh re-unlock)
            return;
        }
    };

    if let Err(_) = vault::write_vault_v2(&app.vault_path, &v2) {
        app.auth_error = Some("Gagal menulis vault ke disk.".into());
        return;
    }

    // Setup ALTER mode
    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
    app.keys = Some(build_self_keys(&bundle, None));
    refresh_invite(app);
    load_contacts_into(app);
    inject_all_client_auth_keys(app);
    let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
    if app.tor.is_some() && has_restricted {
        trigger_tor_restart(app);
    }

    // Bersihkan state sensitif
    app.pass_input.zeroize();
    app.pm_pass_input.zeroize();
    app.pm_pass_confirm.zeroize();
    app.migration_bundle = None;
    app.migration_phase = 0;
    app.auth_error = None;

    app.init_step = 1;
    app.init_start_tick = app.tick_count;
    app.show_onboard_after_init = false;
    app.screen = Screen::Init;
}

// ─── Password Manager TUI handlers ───────────────────────────────────────

fn handle_pm_main_key(app: &mut App, key: KeyEvent) -> bool {
    // Mode pencarian aktif
    if app.pm_search_active {
        match key.code {
            KeyCode::Esc => {
                app.pm_search_active = false;
                app.pm_search.clear();
                app.pm_selected = 0;
            }
            KeyCode::Backspace => { app.pm_search.pop(); app.pm_selected = 0; }
            KeyCode::Char(c) => { app.pm_search.push(c); app.pm_selected = 0; }
            _ => {}
        }
        return false;
    }

    // Konfirmasi hapus
    if let Some(idx) = app.pm_pending_delete {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => pm_delete_entry(app, idx),
            _ => {
                app.pm_pending_delete = None;
                app.set_notif_info("Hapus dibatalkan.");
            }
        }
        return false;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('/') => {
            app.pm_search_active = true;
            app.pm_search.clear();
        }
        KeyCode::Char('a') if !app.pm_is_readonly => {
            app.pm_add_service.clear();
            app.pm_add_username.clear();
            app.pm_add_password.clear();
            app.pm_add_field = 0;
            app.screen = Screen::PmAdd;
        }
        KeyCode::Char('a') => {
            app.set_notif_warn("Mode baca — passphrase tidak dikenal, tidak bisa tambah entri.");
        }
        KeyCode::Char('d') if !app.pm_is_readonly => {
            let visible = pm_visible_entries(app);
            if visible.is_empty() {
                app.set_notif_info("Tidak ada entri untuk dihapus.");
            } else {
                app.pm_pending_delete = Some(visible[app.pm_selected]);
            }
        }
        KeyCode::Char('d') => {
            app.set_notif_warn("Mode baca — tidak bisa hapus entri.");
        }
        KeyCode::Enter => {
            // Reveal password 5 detik
            let visible = pm_visible_entries(app);
            if !visible.is_empty() {
                app.pm_reveal_tick = Some(app.tick_count);
            }
        }
        KeyCode::Up => {
            if app.pm_selected > 0 {
                app.pm_selected -= 1;
            }
        }
        KeyCode::Down => {
            let visible_count = pm_visible_entries(app).len();
            if visible_count > 0 && app.pm_selected + 1 < visible_count {
                app.pm_selected += 1;
            }
        }
        _ => {}
    }
    false
}

fn handle_pm_add_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.pm_add_service.clear();
            app.pm_add_username.clear();
            app.pm_add_password.clear();
            app.pm_add_field = 0;
            app.screen = Screen::PmMain;
        }
        KeyCode::Tab | KeyCode::Down => {
            if app.pm_add_field < 2 {
                app.pm_add_field += 1;
            }
        }
        KeyCode::Up => {
            if app.pm_add_field > 0 {
                app.pm_add_field -= 1;
            }
        }
        KeyCode::Backspace => match app.pm_add_field {
            0 => { app.pm_add_service.pop(); }
            1 => { app.pm_add_username.pop(); }
            _ => { app.pm_add_password.pop(); }
        },
        KeyCode::Char(c) => match app.pm_add_field {
            0 => app.pm_add_service.push(c),
            1 => app.pm_add_username.push(c),
            _ => app.pm_add_password.push(c),
        },
        KeyCode::Enter => {
            if app.pm_add_field < 2 {
                app.pm_add_field += 1;
            } else {
                // Simpan entri baru
                if app.pm_add_service.trim().is_empty() {
                    app.set_notif_error("[!] Service tidak boleh kosong.");
                } else {
                    let new_id = app.pm_entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                    let entry = PmEntry {
                        id: new_id,
                        service: app.pm_add_service.trim().to_string(),
                        username: app.pm_add_username.trim().to_string(),
                        password: app.pm_add_password.clone(),
                    };
                    app.pm_entries.push(entry);
                    // Zeroize field password dari form
                    app.pm_add_password.zeroize();
                    app.pm_add_service.clear();
                    app.pm_add_username.clear();
                    app.pm_add_field = 0;
                    pm_save(app);
                    app.screen = Screen::PmMain;
                }
            }
        }
        _ => {}
    }
    false
}

/// Indeks entries yang terlihat (setelah filter pencarian).
pub(super) fn pm_visible_entries(app: &App) -> Vec<usize> {
    let q = app.pm_search.to_lowercase();
    app.pm_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            q.is_empty() || e.service.to_lowercase().contains(&q)
                || e.username.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

fn pm_delete_entry(app: &mut App, idx: usize) {
    app.pm_pending_delete = None;
    if idx >= app.pm_entries.len() {
        return;
    }
    let svc = app.pm_entries[idx].service.clone();
    app.pm_entries.remove(idx);
    // Sesuaikan pm_selected
    let visible_count = pm_visible_entries(app).len();
    if app.pm_selected >= visible_count && app.pm_selected > 0 {
        app.pm_selected -= 1;
    }
    pm_save(app);
    app.set_notif_info(format!("Entri '{}' dihapus.", svc));
}

fn pm_save(app: &mut App) {
    let (vault_bytes, pm_key) = match (&app.pm_vault_bytes, &app.pm_key) {
        (Some(v), Some(k)) => (*v.clone(), *k.clone()),
        _ => {
            app.set_notif_error("[!] Tidak bisa simpan — state PM tidak valid.");
            return;
        }
    };

    match vault::update_pm(&vault_bytes, &pm_key, &app.pm_entries) {
        Ok(new_vault) => {
            if vault::write_vault_v2(&app.vault_path, &new_vault).is_ok() {
                app.pm_vault_bytes = Some(Box::new(new_vault));
                app.set_notif_success("[✓] Tersimpan.");
            } else {
                app.set_notif_error("[!] Gagal menulis vault ke disk.");
            }
        }
        Err(_) => app.set_notif_error("[!] Gagal enkripsi PM entries."),
    }
}

fn contacts_file_path(vault_path: &Path) -> PathBuf {
    let stem = vault_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "alter".to_string());
    vault_path.with_file_name(format!("{stem}-contacts"))
}

fn load_contacts_into(app: &mut App) {
    let Some(key) = app.contacts_key else { return };
    let path = contacts_file_path(&app.vault_path);
    if let Ok(disk) = contacts::load_contacts(&path, &key) {
        let mut merged = disk;
        for c in std::mem::take(&mut app.contacts) {
            if !merged.iter().any(|d| d.ed25519_pub == c.ed25519_pub) {
                merged.insert(0, c);
            }
        }
        app.contacts = merged;
    }
    persist_contacts(app);
}

fn persist_contacts(app: &mut App) {
    let Some(key) = app.contacts_key else { return };
    let path = contacts_file_path(&app.vault_path);
    if contacts::save_contacts(&path, &app.contacts, &key).is_err() {
        app.set_notif_warn("Peringatan: gagal menyimpan kontak ke disk.");
    }
}

fn copy_invite(app: &mut App) {
    let invite = match &app.keys {
        Some(k) => k.invite.clone(),
        None => return,
    };
    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(invite)) {
        Ok(()) => app.set_notif_success("[✓] Identity disalin ke clipboard"),
        Err(_) => app.set_notif_warn("Clipboard tak tersedia — tekan 'i' untuk salin manual"),
    }
}

/// SEC-13: inject our client auth secret ke keystore arti untuk semua kontak
/// yang punya onion address. Fire-and-forget — tidak memblok.
fn inject_all_client_auth_keys(app: &App) {
    let Some(tor) = app.tor.clone() else { return };
    let Some(keys) = &app.keys else { return };
    let our_secret = keys.tor_client_auth_secret;

    let targets: Vec<String> = app.contacts.iter().filter_map(|c| c.onion.clone()).collect();
    if targets.is_empty() {
        return;
    }

    tokio::spawn(async move {
        for onion in targets {
            let _ = tor.register_client_auth_key(&onion, our_secret).await;
        }
    });
}

/// SEC-13: kumpulkan semua tor_client_auth_pub dari kontak dan restart service.
///
/// Dipanggil saat: (1) kontak baru dengan cap ditambah, (2) vault baru dibuka
/// dengan kontak yang punya cap, (3) Tor baru siap dan sudah ada kontak cap.
fn trigger_tor_restart(app: &mut App) {
    let Some(tor) = app.tor.clone() else { return };

    let auth_keys: Vec<[u8; 32]> = app
        .contacts
        .iter()
        .filter_map(|c| c.tor_client_auth_pub)
        .collect();

    app.tor_restarting = true;
    app.set_notif_info("Tor service restart untuk restricted discovery (~5s)…");

    let tx = app.tor_restart_result_tx.clone();
    tokio::spawn(async move {
        let result = tor.restart_with_authorized_keys(&auth_keys).await.map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
}

fn handle_main_key(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
    key: KeyEvent,
) -> bool {
    match app.mode {
        Mode::Browsing => {
            if let Some(idx) = app.pending_delete {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => delete_contact(app, idx),
                    _ => {
                        app.pending_delete = None;
                        app.set_notif_info("Hapus dibatalkan.");
                    }
                }
                return false;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return true,
                KeyCode::Char('i') => app.show_invite = !app.show_invite,
                KeyCode::Char('c') => copy_invite(app),
                KeyCode::Char('a') => {
                    app.mode = Mode::AddContact;
                    app.add_buffer.clear();
                    app.notification = None;
                }
                KeyCode::Char('r') => {
                    if !app.contacts.is_empty() {
                        app.rename_buffer = app.contacts[app.selected].nickname.clone();
                        app.mode = Mode::RenameContact;
                    }
                }
                KeyCode::Char('d') => {
                    if app.contacts.is_empty() {
                        app.set_notif_info("Belum ada kontak untuk dihapus.");
                    } else {
                        app.pending_delete = Some(app.selected);
                    }
                }
                KeyCode::Up => {
                    if app.selected > 0 {
                        app.selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.selected + 1 < app.contacts.len() {
                        app.selected += 1;
                    }
                }
                KeyCode::Enter => start_connection(app, out_tx, ev_rx),
                _ => {}
            }
        }

        Mode::AddContact => match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Browsing;
                app.add_buffer.clear();
            }
            KeyCode::Backspace => { app.add_buffer.pop(); }
            KeyCode::Enter => add_contact_from_buffer(app),
            KeyCode::Char(c) => app.add_buffer.push(c),
            _ => {}
        },

        Mode::RenameContact => match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Browsing;
                app.rename_buffer.clear();
            }
            KeyCode::Backspace => { app.rename_buffer.pop(); }
            KeyCode::Enter => {
                let new_name = app.rename_buffer.trim().to_string();
                if !new_name.is_empty() && !app.contacts.is_empty() {
                    let old_name = app.contacts[app.selected].nickname.clone();
                    app.contacts[app.selected].nickname = new_name.clone();
                    persist_contacts(app);
                    app.set_notif_success(format!("[✓] '{old_name}' → '{new_name}'"));
                }
                app.mode = Mode::Browsing;
                app.rename_buffer.clear();
            }
            KeyCode::Char(c) => app.rename_buffer.push(c),
            _ => {}
        },

        Mode::InRoom => match key.code {
            KeyCode::Esc => leave_room(app, out_tx, ev_rx),
            KeyCode::Backspace => { app.input.pop(); }
            KeyCode::Enter => send_message(app, out_tx),
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
    }
    false
}

fn start_connection(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) {
    if app.contacts.is_empty() {
        app.set_notif_info("Belum ada kontak. Tekan 'a' untuk menambah.");
        return;
    }
    let keys = match &app.keys {
        Some(k) => k,
        None => return,
    };
    let contact = app.contacts[app.selected].clone();

    if let Some(h) = app.conn_task.take() {
        h.abort();
    }

    let (o_tx, o_rx) = mpsc::unbounded_channel::<String>();
    let (e_tx, e_rx) = mpsc::unbounded_channel::<SessionEvent>();
    *out_tx = Some(o_tx);
    *ev_rx = Some(e_rx);

    app.mode = Mode::InRoom;
    app.room = RoomState::Connecting;
    app.peer_name = Some(contact.nickname.clone());
    app.messages.clear();

    let my_fp = keys.fingerprint.clone();
    let target_fp = contacts::fingerprint(&contact.ed25519_pub);
    let local_sk = keys.noise_sk;
    let peer_pk = contact.noise_pub;
    let onion = contact.onion.clone();
    let lan: LanMode = app.connect_kind.into();
    let tor = app.tor.clone();
    // SEC-13: sertakan our_tor_auth_secret untuk inject ke arti keystore saat dial.
    let our_auth_secret = keys.tor_client_auth_secret;

    let handle = tokio::spawn(async move {
        let _ = e_tx.send(SessionEvent::StateChanged(SessionState::Connecting));
        match transport::establish(
            &my_fp,
            &target_fp,
            lan,
            onion.as_deref(),
            tor.as_ref(),
            Some(our_auth_secret),
        )
        .await
        {
            Ok((conn, role)) => {
                let _ =
                    session::run_session(conn, role, local_sk, Some(peer_pk), o_rx, e_tx).await;
            }
            Err(err) => {
                let _ = e_tx.send(SessionEvent::Error(err.to_string()));
            }
        }
    });
    app.conn_task = Some(handle);
}

fn add_contact_from_buffer(app: &mut App) {
    let line = app.add_buffer.trim().to_string();
    let mut parts = line.splitn(2, char::is_whitespace);
    let code = parts.next().unwrap_or("");
    let nick = parts.next().unwrap_or("").trim();

    match contacts::decode_invite(code) {
        Ok((ed, noise, cap, onion)) => {
            if let Some(keys) = &app.keys {
                if ed == keys.ed25519_pub {
                    app.set_notif_error("[!] Tidak bisa menambah diri sendiri sebagai kontak.");
                    return;
                }
            }
            let nickname = if nick.is_empty() {
                format!("peer-{}", &contacts::fingerprint(&ed)[..8])
            } else {
                nick.to_string()
            };
            let via = match (&onion, &cap) {
                (Some(_), Some(_)) => "LAN+Tor (restricted discovery)",
                (Some(_), None) => "LAN+Tor",
                _ => "LAN",
            };
            let has_cap = cap.is_some();
            app.contacts.insert(
                0,
                Contact {
                    nickname: nickname.clone(),
                    ed25519_pub: ed,
                    noise_pub: noise,
                    onion,
                    tor_client_auth_pub: cap,
                },
            );
            app.selected = 0;
            app.mode = Mode::Browsing;
            app.add_buffer.clear();
            persist_contacts(app);

            // SEC-13: bila kontak v2 (punya cap), restart service dengan semua kunci.
            if has_cap {
                trigger_tor_restart(app);
            }

            app.set_notif_success(format!("[✓] Kontak '{nickname}' ditambahkan ({via})."));
        }
        Err(_) => {
            app.set_notif_error(
                "[!] Invite code tidak valid. (Format v1 tidak lagi didukung — minta invite baru.)",
            );
        }
    }
}

fn delete_contact(app: &mut App, idx: usize) {
    app.pending_delete = None;
    if idx >= app.contacts.len() {
        return;
    }
    let removed = app.contacts.remove(idx);
    if app.selected >= app.contacts.len() {
        app.selected = app.contacts.len().saturating_sub(1);
    }
    persist_contacts(app);
    app.set_notif_info(format!("Kontak '{}' dihapus.", removed.nickname));
}

fn send_message(app: &mut App, out_tx: &Option<mpsc::UnboundedSender<String>>) {
    if app.room != RoomState::Open {
        return;
    }
    let text = std::mem::take(&mut app.input);
    if text.is_empty() {
        return;
    }
    if let Some(tx) = out_tx {
        if tx.send(text.clone()).is_ok() {
            app.messages.push(ChatLine::me(text));
        }
    }
}

fn leave_room(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) {
    if let Some(h) = app.conn_task.take() {
        h.abort();
    }
    *out_tx = None;
    *ev_rx = None;
    app.mode = Mode::Browsing;
    app.room = RoomState::None;
    app.peer_name = None;
    app.input.clear();
    app.messages.clear();
    app.set_notif_info("Keluar dari sesi. Riwayat dibuang.");
}

fn handle_session_event(app: &mut App, se: SessionEvent) {
    match se {
        SessionEvent::StateChanged(state) => match state {
            SessionState::Connecting => { app.room = RoomState::Connecting; }
            SessionState::Handshaking => { app.room = RoomState::Handshaking; }
            SessionState::Active => {
                app.room = RoomState::Open;
                app.messages.push(ChatLine::system("Sesi aman terbuka.".into()));
            }
            SessionState::Closed => {
                if app.room == RoomState::Open {
                    app.room = RoomState::Closed;
                }
            }
        },
        SessionEvent::Message(text) => app.messages.push(ChatLine::peer(text)),
        SessionEvent::PeerLeft => {
            app.room = RoomState::PeerLeft;
            app.messages.push(ChatLine::system("Peer keluar dari sesi.".into()));
        }
        SessionEvent::Error(e) => {
            app.room = RoomState::Closed;
            app.set_notif_error(format!("Koneksi gagal: {e}"));
            app.messages.push(ChatLine::system(format!("Error: {e}")));
        }
    }
}
