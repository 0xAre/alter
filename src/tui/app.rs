//! App struct + impl — state utama TUI.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::contacts::{self, Contact};
use crate::identity::keypair::KeyBundle;
use crate::identity::pm::PmEntry;
use crate::identity::vault;
use crate::platform;
use crate::transport::LanMode;
use crate::transport::obfs4::Obfs4Status;
use crate::transport::tor::TorContext;

use super::types::{
    ChatLine, CreatePhase, Mode, Notification, RoomState, Screen, UnlockComputed,
};

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

pub struct App {
    pub keys: Option<SelfKeys>,
    pub vault_path: PathBuf,
    pub tor: Option<Arc<TorContext>>,
    pub tor_connecting: bool,
    /// SEC-13: true saat onion service di-restart untuk update restricted discovery.
    pub tor_restarting: bool,
    /// Sender untuk menerima hasil restart service dari background task.
    pub(super) tor_restart_result_tx: mpsc::UnboundedSender<Result<Arc<TorContext>, String>>,
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
    /// Field aktif di form PmAdd: 0=service, 1=username, 2=password, 3=backup codes.
    pub pm_add_field: u8,
    /// Backup codes yang sedang diketik di form tambah (step 4).
    pub pm_add_codes: Vec<String>,
    pub pm_add_code_input: String,

    // ─── PM Codes modal ──────────────────────────────────────────────────
    /// True saat modal backup codes terbuka.
    pub pm_codes_open: bool,
    /// Indeks code yang sedang dipilih dalam modal.
    pub pm_codes_selected: usize,
    /// True saat user sedang mengetik code baru di modal.
    pub pm_codes_add_mode: bool,
    /// Buffer input code baru di modal.
    pub pm_codes_input: String,

    // ─── M6: Migrasi vault v1 → v2 ───────────────────────────────────────
    /// Bundle sementara selama migrasi (dibuang setelah migrasi selesai).
    pub migration_bundle: Option<KeyBundle>,
    /// Fase migrasi: 0=masukkan PM pass, 1=konfirmasi PM pass.
    pub migration_phase: u8,

    // ─── Async unlock ─────────────────────────────────────────────────────
    /// Receiver hasil KDF dari background thread (aktif hanya saat Screen::Unlocking).
    pub unlock_rx: Option<mpsc::UnboundedReceiver<UnlockComputed>>,
}

impl App {
    pub(super) fn new(
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
            pm_add_codes: Vec::new(),
            pm_add_code_input: String::new(),
            pm_codes_open: false,
            pm_codes_selected: 0,
            pm_codes_add_mode: false,
            pm_codes_input: String::new(),
            migration_bundle: None,
            migration_phase: 0,
            unlock_rx: None,
        }
    }

    pub fn tor_active(&self) -> bool {
        self.tor.is_some()
    }

    pub(crate) fn set_notif_error(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::error(text));
    }
    pub(crate) fn set_notif_success(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::success(self.tick_count, text));
    }
    pub(crate) fn set_notif_info(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::info(self.tick_count, text));
    }
    pub(crate) fn set_notif_warn(&mut self, text: impl Into<String>) {
        self.notification = Some(Notification::warn(text));
    }
}

pub fn build_self_keys(bundle: &KeyBundle, onion: Option<&str>) -> SelfKeys {
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
pub(super) fn refresh_invite(app: &mut App) {
    let onion = app.tor.as_ref().map(|t| t.onion_address.clone());
    if let Some(k) = app.keys.as_mut() {
        k.invite =
            contacts::encode_invite(&k.ed25519_pub, &k.noise_pub, &k.tor_client_auth_pub, onion.as_deref());
    }
}
