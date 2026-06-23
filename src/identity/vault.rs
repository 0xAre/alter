/// Vault: enkripsi keypair ke disk menggunakan Argon2id + ChaCha20Poly1305.
///
/// Format binary vault (tidak ada magic bytes, tidak ada versi, tidak ada identifier):
///
///   [16 bytes] Argon2id salt      — random per-encryption
///   [12 bytes] ChaCha20 nonce     — random per-encryption
///   [64 bytes] ciphertext         — Ed25519 sk (32 bytes) || X25519 sk (32 bytes)
///   [16 bytes] Poly1305 tag       — auth tag
///   ──────────────────────────────
///   108 bytes total
///
/// Tidak ada header, tidak ada magic bytes, tidak ada string yang mengidentifikasi
/// app atau format. File ini tidak bisa dibedakan dari 108 bytes noise secara
/// statistik tanpa passphrase yang benar (SEC-05 compliance).
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::rngs::OsRng;
use zeroize::Zeroizing;

use crate::error::Error;
use super::keypair::{IdentityKey, KeyBundle, NoiseKey};

pub const VAULT_SIZE: usize = SALT_LEN + NONCE_LEN + PLAINTEXT_LEN + TAG_LEN;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const PLAINTEXT_LEN: usize = 64; // Ed25519 sk (32) + X25519 sk (32)
const TAG_LEN: usize = 16;

/// Parameter Argon2id per rekomendasi OWASP 2024:
/// m=19 MiB, t=2 iterasi, p=1 thread.
///
/// Dipilih untuk balance antara security dan UX — pada hardware modern ini
/// membutuhkan ~100ms untuk unlock, cukup lambat untuk brute-force tapi
/// tidak menyebabkan frustrasi user.
fn argon2_params() -> Params {
    Params::new(
        19 * 1024, // 19 MiB dalam KiB
        2,         // 2 iterasi
        1,         // 1 parallelism
        Some(32),  // output 32 bytes (ukuran ChaCha20 key)
    )
    .expect("hardcoded params selalu valid")
}

fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, Error> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params());
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(passphrase, salt, key.as_mut())
        .map_err(|_| Error::KeyDerivation)?;
    Ok(key)
}

/// Enkripsi `bundle` ke dalam `[u8; VAULT_SIZE]` menggunakan `passphrase`.
/// Hasil bisa langsung ditulis ke file — tidak perlu format wrapper apapun.
pub fn seal(bundle: &KeyBundle, passphrase: &[u8]) -> Result<[u8; VAULT_SIZE], Error> {
    // 1. Susun plaintext: [Ed25519 sk (32)] [X25519 sk (32)]
    let mut plaintext = Zeroizing::new([0u8; PLAINTEXT_LEN]);
    plaintext[..32].copy_from_slice(bundle.identity.secret_bytes());
    plaintext[32..].copy_from_slice(&bundle.noise.secret_bytes());

    // 2. Random salt untuk Argon2id
    let mut salt = [0u8; SALT_LEN];
    use rand::RngCore;
    OsRng.fill_bytes(&mut salt);

    // 3. Derive encryption key dari passphrase
    let enc_key = derive_key(passphrase, &salt)?;

    // 4. Random nonce untuk ChaCha20Poly1305
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    // 5. Encrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&*enc_key));
    let ciphertext: Vec<u8> = cipher
        .encrypt(&nonce, plaintext.as_slice())
        .map_err(|_| Error::Encryption)?;

    // ciphertext = encrypted plaintext (64 bytes) + tag (16 bytes) = 80 bytes
    debug_assert_eq!(ciphertext.len(), PLAINTEXT_LEN + TAG_LEN);

    // 6. Assemble vault: salt || nonce || ciphertext+tag
    let mut out = [0u8; VAULT_SIZE];
    out[..SALT_LEN].copy_from_slice(&salt);
    out[SALT_LEN..SALT_LEN + NONCE_LEN].copy_from_slice(&nonce);
    out[SALT_LEN + NONCE_LEN..].copy_from_slice(&ciphertext);

    Ok(out)
}

/// Dekripsi vault bytes menggunakan `passphrase`, kembalikan `KeyBundle`.
/// Error sengaja ambigu antara "passphrase salah" dan "vault corrupt"
/// untuk mencegah oracle attack (lihat error.rs).
pub fn unseal(vault: &[u8; VAULT_SIZE], passphrase: &[u8]) -> Result<KeyBundle, Error> {
    let salt = &vault[..SALT_LEN];
    let nonce = Nonce::from_slice(&vault[SALT_LEN..SALT_LEN + NONCE_LEN]);
    let ciphertext = &vault[SALT_LEN + NONCE_LEN..];

    // Derive key (Argon2id — sama parameter dengan seal())
    let enc_key = derive_key(passphrase, salt)?;

    // Decrypt + verify auth tag
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&*enc_key));
    let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| Error::Decryption)?,
    );

    if plaintext.len() != PLAINTEXT_LEN {
        return Err(Error::Decryption);
    }

    // Parse plaintext: [Ed25519 sk] [X25519 sk]
    let mut ed_bytes = [0u8; 32];
    let mut x25519_bytes = [0u8; 32];
    ed_bytes.copy_from_slice(&plaintext[..32]);
    x25519_bytes.copy_from_slice(&plaintext[32..]);

    Ok(KeyBundle {
        identity: IdentityKey::from_secret_bytes(ed_bytes),
        noise: NoiseKey::from_secret_bytes(x25519_bytes),
    })
}

/// Tulis vault ke path yang diberikan. Filename dipilih oleh caller —
/// ini sengaja agar aplikasi bisa pakai nama generik (SEC-05).
pub fn write_vault(path: &std::path::Path, vault: &[u8; VAULT_SIZE]) -> Result<(), Error> {
    std::fs::write(path, vault.as_slice()).map_err(Error::Io)
}

/// Baca vault dari disk.
pub fn read_vault(path: &std::path::Path) -> Result<[u8; VAULT_SIZE], Error> {
    let bytes = std::fs::read(path).map_err(Error::Io)?;
    bytes
        .try_into()
        .map_err(|_| Error::Decryption) // ukuran salah → ambigu error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unseal_roundtrip() {
        let bundle = KeyBundle::generate();
        let original_id_pub = bundle.identity.public_key().to_bytes();
        let original_noise_pub = bundle.noise.public_bytes();

        let vault = seal(&bundle, b"test-passphrase-secure-enough").unwrap();
        assert_eq!(vault.len(), VAULT_SIZE);

        let restored = unseal(&vault, b"test-passphrase-secure-enough").unwrap();
        assert_eq!(restored.identity.public_key().to_bytes(), original_id_pub);
        assert_eq!(restored.noise.public_bytes(), original_noise_pub);
    }

    #[test]
    fn wrong_passphrase_returns_error() {
        let bundle = KeyBundle::generate();
        let vault = seal(&bundle, b"correct-passphrase").unwrap();
        assert!(unseal(&vault, b"wrong-passphrase").is_err());
    }

    #[test]
    fn vault_looks_random() {
        // Dua enkripsi dari keybundle yang sama menghasilkan vault yang berbeda
        // (karena salt dan nonce random)
        let bundle = KeyBundle::generate();
        let vault1 = seal(&bundle, b"same-passphrase").unwrap();
        let vault2 = seal(&bundle, b"same-passphrase").unwrap();
        assert_ne!(vault1, vault2, "vault harus non-deterministic");
    }

    #[test]
    fn tampered_vault_returns_error() {
        let bundle = KeyBundle::generate();
        let mut vault = seal(&bundle, b"test-passphrase").unwrap();
        // Flip satu byte di ciphertext
        vault[SALT_LEN + NONCE_LEN + 10] ^= 0xFF;
        assert!(unseal(&vault, b"test-passphrase").is_err());
    }

    #[test]
    fn vault_has_no_magic_bytes() {
        // Vault tidak boleh diawali dengan pattern yang identifiable
        let bundle = KeyBundle::generate();
        let vault = seal(&bundle, b"test").unwrap();
        // Tidak ada magic bytes — 16 byte pertama adalah random salt
        // (Tidak ada assertion spesifik yang bisa dibuat selain "bukan constant")
        // Verifikasi: enkripsi ulang dengan key yang sama menghasilkan salt berbeda
        let vault2 = seal(&bundle, b"test").unwrap();
        assert_ne!(&vault[..SALT_LEN], &vault2[..SALT_LEN], "salt harus random per-call");
    }
}
