/// Semua error yang dilempar oleh ALTER. Pesan error sengaja tidak terlalu detail
/// untuk menghindari oracle attack (contoh: "wrong passphrase" vs "corrupted vault"
/// dibuat identik dari luar).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("key derivation failed")]
    KeyDerivation,

    #[error("encryption failed")]
    Encryption,

    /// Ambiguous on purpose: caller tidak tahu apakah passphrase salah atau vault corrupt.
    #[error("vault could not be opened")]
    Decryption,

    #[error("noise handshake error")]
    Noise(#[from] snow::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid key material")]
    InvalidKey,

    #[error("frame exceeds maximum size")]
    FrameTooLarge,

    #[error("connection closed")]
    ConnectionClosed,

    #[error("mDNS error: {0}")]
    Mdns(String),

    #[error("Tor error: {0}")]
    Tor(String),

    #[error("invalid invite code")]
    InvalidInvite,

    /// Remote static key tidak cocok dengan kontak yang diharapkan — fail closed.
    #[error("peer identity mismatch")]
    IdentityMismatch,

}
