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

#[derive(PartialEq, Eq)]
pub(crate) enum Screen {
    Splash,
    Unlock,
    Create,
    Init,
    Onboard,
    Main,
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
    pub create_confirming: bool,
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
            create_confirming: false,
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
        app.messages.clear();
        app.input.clear();
        app.add_buffer.clear();
        app.rename_buffer.clear();
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
    }
}

fn handle_unlock_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter => {
            if try_unlock(app) {
                app.pass_input.zeroize();
                app.init_step = 1;
                app.init_start_tick = app.tick_count;
                app.screen = Screen::Init;
            } else {
                app.pass_input.zeroize();
            }
        }
        KeyCode::Backspace => { app.pass_input.pop(); }
        KeyCode::Char(c) => app.pass_input.push(c),
        _ => {}
    }
    false
}

fn handle_create_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Backspace => {
            if app.create_confirming {
                app.pass_confirm.pop();
            } else {
                app.pass_input.pop();
            }
        }
        KeyCode::Char(c) => {
            if app.create_confirming {
                app.pass_confirm.push(c);
            } else {
                app.pass_input.push(c);
            }
        }
        KeyCode::Enter => {
            if !app.create_confirming {
                if app.pass_input.is_empty() {
                    app.auth_error = Some("Passphrase tidak boleh kosong.".into());
                } else {
                    app.auth_error = None;
                    app.create_confirming = true;
                }
            } else if app.pass_confirm != app.pass_input {
                app.auth_error = Some("Passphrase tidak cocok. Ulangi.".into());
                app.pass_input.zeroize();
                app.pass_confirm.zeroize();
                app.create_confirming = false;
            } else {
                match create_vault(app) {
                    Ok(()) => {
                        app.pass_input.zeroize();
                        app.pass_confirm.zeroize();
                        app.auth_error = None;
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

fn try_unlock(app: &mut App) -> bool {
    let vault_bytes = match vault::read_vault(&app.vault_path) {
        Ok(v) => v,
        Err(_) => {
            app.auth_error = Some("Vault tidak terbaca.".into());
            return false;
        }
    };
    match vault::unseal(&vault_bytes, app.pass_input.as_bytes()) {
        Ok(bundle) => {
            app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
            app.keys = Some(build_self_keys(&bundle, None));
            refresh_invite(app);
            app.auth_error = None;
            load_contacts_into(app);

            // SEC-13: setelah unlock, inject semua client auth keys dan restart service
            // bila ada kontak dengan restricted discovery.
            inject_all_client_auth_keys(app);
            let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
            if app.tor.is_some() && has_restricted {
                trigger_tor_restart(app);
            }

            true
        }
        Err(_) => {
            app.auth_error = Some("Passphrase salah atau vault rusak.".into());
            false
        }
    }
}

fn create_vault(app: &mut App) -> Result<(), Error> {
    let bundle = KeyBundle::generate();
    let vault_bytes = vault::seal(&bundle, app.pass_input.as_bytes())?;
    vault::write_vault(&app.vault_path, &vault_bytes)?;
    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
    app.keys = Some(build_self_keys(&bundle, None));
    refresh_invite(app);
    load_contacts_into(app);
    Ok(())
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
