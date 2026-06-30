//! Semua enum dan struct sederhana yang dipakai di seluruh TUI layer.

use crate::identity::vault;
use crate::identity::pm::PmEntry;
use crate::identity::keypair::KeyBundle;

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
    /// Argon2id KDF berjalan di background thread — tampilkan spinner
    Unlocking,
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
    pub(crate) fn me(text: String) -> Self {
        Self { who: Who::Me, text }
    }
    pub(crate) fn peer(text: String) -> Self {
        Self { who: Who::Peer, text }
    }
    pub(crate) fn system(text: String) -> Self {
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
    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Error, text: text.into(), dismiss_at: None }
    }
    pub(crate) fn warn(text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Warn, text: text.into(), dismiss_at: None }
    }
    pub(crate) fn success(tick: u64, text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Success, text: text.into(), dismiss_at: Some(tick + 30) }
    }
    pub(crate) fn info(tick: u64, text: impl Into<String>) -> Self {
        Self { level: NotifLevel::Info, text: text.into(), dismiss_at: Some(tick + 40) }
    }
}

/// Hasil komputasi KDF+dekripsi dari background thread — semua tipe ini Send.
pub(crate) enum UnlockComputed {
    AlterMode(KeyBundle),
    PmMode { pm_entries: Vec<PmEntry>, pm_key: [u8; 32], vault: Box<[u8; vault::VAULT_V2_SIZE]> },
    EmptyPm(Box<[u8; vault::VAULT_V2_SIZE]>),
    NeedsMigration(KeyBundle),
    Err(String),
}
