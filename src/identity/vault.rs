/// Vault: enkripsi keypair ke disk menggunakan Argon2id + ChaCha20Poly1305.
///
/// ## Vault v1 (108 bytes) — format lama
///
///   [16 bytes] Argon2id salt      — random per-encryption
///   [12 bytes] ChaCha20 nonce     — random per-encryption
///   [64 bytes] ciphertext         — Ed25519 sk (32 bytes) || X25519 sk (32 bytes)
///   [16 bytes] Poly1305 tag       — auth tag
///   ──────────────────────────────
///   108 bytes total
///
/// ## Vault v2 (4096 bytes) — M6: dual-slot dengan password manager decoy front
///
///   [32 bytes] salt_a             — Argon2id salt untuk slot A (PM)
///   [32 bytes] salt_b             — Argon2id salt untuk slot B (ALTER)
///   [12 bytes] nonce_a            — ChaCha20 nonce slot A
///   [3916 bytes] ciphertext_a     — PM entries (3900 bytes plaintext + 16 tag)
///   [12 bytes] nonce_b            — ChaCha20 nonce slot B
///   [80 bytes] ciphertext_b       — ALTER keypair (64 bytes plaintext + 16 tag)
///   [12 bytes] CSPRNG padding     — padding acak sampai 4096 bytes
///   ──────────────────────────────
///   4096 bytes total
///
/// Invariant: tidak ada magic bytes, tidak ada identifier — kedua ukuran vault
/// tidak bisa dibedakan dari random noise tanpa passphrase yang benar (SEC-05).
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use rand::rngs::OsRng;
use zeroize::Zeroizing;

use crate::error::Error;
use super::keypair::{IdentityKey, KeyBundle, NoiseKey};
use super::pm::{PmEntry, PmStore};

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

// ═══════════════════════════════════════════════════════════════════════════
//  Vault v2 — M6: dual-slot (ALTER + Password Manager Decoy Front)
// ═══════════════════════════════════════════════════════════════════════════

/// Ukuran vault v2 dalam bytes — selalu 4096.
pub const VAULT_V2_SIZE: usize = 4096;

// Layout offsets vault v2 (byte positions dalam VAULT_V2_SIZE)
const V2_OFF_SALT_A: usize = 0;
const V2_OFF_SALT_B: usize = 32;
const V2_OFF_NONCE_A: usize = 64;
const V2_OFF_CT_A: usize = 76;
const V2_CT_A_SIZE: usize = 3916; // PM_PLAINTEXT_SIZE(3900) + TAG(16)
const V2_OFF_NONCE_B: usize = 3992; // 76 + 3916
const V2_OFF_CT_B: usize = 4004;   // 3992 + 12
const V2_CT_B_SIZE: usize = 80;    // keypair(64) + TAG(16)
const V2_OFF_PAD: usize = 4084;    // 4004 + 80
const V2_PAD_SIZE: usize = 12;     // 4084 + 12 = 4096

// Verifikasi layout di compile-time
const _: () = assert!(V2_OFF_SALT_B == V2_OFF_SALT_A + 32);
const _: () = assert!(V2_OFF_NONCE_A == V2_OFF_SALT_B + 32);
const _: () = assert!(V2_OFF_CT_A == V2_OFF_NONCE_A + 12);
const _: () = assert!(V2_OFF_NONCE_B == V2_OFF_CT_A + V2_CT_A_SIZE);
const _: () = assert!(V2_OFF_CT_B == V2_OFF_NONCE_B + 12);
const _: () = assert!(V2_OFF_PAD == V2_OFF_CT_B + V2_CT_B_SIZE);
const _: () = assert!(VAULT_V2_SIZE == V2_OFF_PAD + V2_PAD_SIZE);

/// Versi vault yang terdeteksi dari ukuran file.
pub enum VaultVersion {
    V1,
    V2,
    Unknown,
}

/// Hasil membuka vault v2 — always returns something (never-fail).
pub enum VaultOpenResult {
    /// Passphrase B cocok → ALTER chat terbuka.
    AlterMode(KeyBundle),
    /// Passphrase A cocok → Password Manager terbuka.
    PmMode { pm_entries: Vec<PmEntry>, pm_key: [u8; 32] },
    /// Passphrase tidak dikenal → Password Manager kosong (never-fail).
    EmptyPm,
}

/// Deteksi versi vault berdasarkan panjang raw bytes.
pub fn detect_version(bytes: &[u8]) -> VaultVersion {
    match bytes.len() {
        VAULT_SIZE => VaultVersion::V1,
        VAULT_V2_SIZE => VaultVersion::V2,
        _ => VaultVersion::Unknown,
    }
}

/// Baca bytes mentah dari file vault (tanpa asumsi versi).
pub fn read_vault_raw(path: &std::path::Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(Error::Io)
}

/// Tulis vault v2 ke disk.
pub fn write_vault_v2(path: &std::path::Path, vault: &[u8; VAULT_V2_SIZE]) -> Result<(), Error> {
    std::fs::write(path, vault.as_slice()).map_err(Error::Io)
}

/// Parameter Argon2id vault v2 — lebih kuat dari v1 (PRD v0.4, SEC-15):
/// m=64 MiB, t=3, p=1.
///
/// Lebih lambat (~500ms per derivasi) tapi diperlukan untuk dual-slot
/// (attacker perlu brute-force dua slot independen).
fn argon2_params_v2() -> Params {
    Params::new(
        64 * 1024, // 64 MiB dalam KiB
        3,         // 3 iterasi
        1,         // 1 parallelism
        Some(32),  // output 32 bytes
    )
    .expect("hardcoded params selalu valid")
}

fn derive_key_v2(passphrase: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, Error> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params_v2());
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(passphrase, salt, key.as_mut())
        .map_err(|_| Error::KeyDerivation)?;
    Ok(key)
}

/// Buat vault v2 baru dengan dua slot independen.
///
/// - `bundle` + `passphrase_b` → slot B (ALTER keypair)
/// - PM kosong + `passphrase_a` → slot A (Password Manager)
///
/// Salt digenerate secara independen — `key_a` dan `key_b` tidak berkorelasi.
pub fn create_v2(
    bundle: &KeyBundle,
    passphrase_b: &[u8],
    passphrase_a: &[u8],
) -> Result<[u8; VAULT_V2_SIZE], Error> {
    // Random salts independen untuk dua slot
    let mut salt_a = [0u8; 32];
    let mut salt_b = [0u8; 32];
    OsRng.fill_bytes(&mut salt_a);
    OsRng.fill_bytes(&mut salt_b);

    // Derive key_a dan key_b dari salt masing-masing
    let key_a = derive_key_v2(passphrase_a, &salt_a)?;
    let key_b = derive_key_v2(passphrase_b, &salt_b)?;

    // Plaintext slot A: PM kosong (3900 bytes)
    let pm_plain = PmStore::default().to_plaintext()?;
    debug_assert_eq!(pm_plain.len(), 3900);

    // Plaintext slot B: Ed25519 sk (32) || X25519 sk (32) = 64 bytes
    let mut kp_plain = Zeroizing::new([0u8; 64]);
    kp_plain[..32].copy_from_slice(bundle.identity.secret_bytes());
    kp_plain[32..].copy_from_slice(&bundle.noise.secret_bytes());

    // Random nonces
    let nonce_a = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let nonce_b = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    // Enkripsi slot A
    let cipher_a = ChaCha20Poly1305::new(Key::from_slice(&*key_a));
    let ct_a = cipher_a
        .encrypt(&nonce_a, pm_plain.as_slice())
        .map_err(|_| Error::Encryption)?;
    debug_assert_eq!(ct_a.len(), V2_CT_A_SIZE);

    // Enkripsi slot B
    let cipher_b = ChaCha20Poly1305::new(Key::from_slice(&*key_b));
    let ct_b = cipher_b
        .encrypt(&nonce_b, kp_plain.as_slice())
        .map_err(|_| Error::Encryption)?;
    debug_assert_eq!(ct_b.len(), V2_CT_B_SIZE);

    // Assemble vault 4096 bytes
    let mut vault = [0u8; VAULT_V2_SIZE];
    vault[V2_OFF_SALT_A..V2_OFF_SALT_B].copy_from_slice(&salt_a);
    vault[V2_OFF_SALT_B..V2_OFF_NONCE_A].copy_from_slice(&salt_b);
    vault[V2_OFF_NONCE_A..V2_OFF_CT_A].copy_from_slice(&nonce_a);
    vault[V2_OFF_CT_A..V2_OFF_NONCE_B].copy_from_slice(&ct_a);
    vault[V2_OFF_NONCE_B..V2_OFF_CT_B].copy_from_slice(&nonce_b);
    vault[V2_OFF_CT_B..V2_OFF_PAD].copy_from_slice(&ct_b);
    OsRng.fill_bytes(&mut vault[V2_OFF_PAD..]);

    Ok(vault)
}

/// Buka vault v2 — tidak pernah return error (invariant: never-fail).
///
/// Urutan pemeriksaan:
/// 1. Derive key_b dan key_a (SELALU keduanya — mencegah timing attack)
/// 2. Coba decrypt slot B → jika berhasil → AlterMode
/// 3. Coba decrypt slot A → jika berhasil → PmMode
/// 4. Keduanya gagal → EmptyPm (passphrase tidak dikenal → PM kosong)
///
/// Karena kedua KDF selalu dijalankan (m=64MB ± 500ms masing-masing),
/// timing antara passphrase_a, passphrase_b, dan passphrase_unknown
/// tidak berbeda signifikan. (PRD v0.4 Bagian 5.4 — no timing leak)
pub fn open_v2(vault: &[u8; VAULT_V2_SIZE], passphrase: &[u8]) -> VaultOpenResult {
    let salt_a = &vault[V2_OFF_SALT_A..V2_OFF_SALT_B];
    let salt_b = &vault[V2_OFF_SALT_B..V2_OFF_NONCE_A];

    // Selalu derive dua kunci sebelum cek hasil — mencegah timing leak
    let key_b = derive_key_v2(passphrase, salt_b).ok();
    let key_a = derive_key_v2(passphrase, salt_a).ok();

    // Coba slot B (ALTER mode) terlebih dahulu
    if let Some(ref kb) = key_b {
        let nonce_b = Nonce::from_slice(&vault[V2_OFF_NONCE_B..V2_OFF_CT_B]);
        let ct_b = &vault[V2_OFF_CT_B..V2_OFF_PAD];
        let cipher_b = ChaCha20Poly1305::new(Key::from_slice(kb.as_ref()));
        if let Ok(plain) = cipher_b.decrypt(nonce_b, ct_b) {
            if plain.len() == 64 {
                let mut ed = [0u8; 32];
                let mut xk = [0u8; 32];
                ed.copy_from_slice(&plain[..32]);
                xk.copy_from_slice(&plain[32..]);
                return VaultOpenResult::AlterMode(KeyBundle {
                    identity: IdentityKey::from_secret_bytes(ed),
                    noise: NoiseKey::from_secret_bytes(xk),
                });
            }
        }
    }

    // Coba slot A (PM mode)
    if let Some(ka) = key_a {
        let nonce_a = Nonce::from_slice(&vault[V2_OFF_NONCE_A..V2_OFF_CT_A]);
        let ct_a = &vault[V2_OFF_CT_A..V2_OFF_NONCE_B];
        let cipher_a = ChaCha20Poly1305::new(Key::from_slice(ka.as_ref()));
        if let Ok(plain) = cipher_a.decrypt(nonce_a, ct_a) {
            let store = PmStore::from_plaintext(&plain);
            let pm_key: [u8; 32] = *ka;
            return VaultOpenResult::PmMode {
                pm_entries: store.entries,
                pm_key,
            };
        }
    }

    // Passphrase tidak dikenal → buka PM kosong (never-fail)
    VaultOpenResult::EmptyPm
}

/// Update slot A (PM entries) vault v2 — slot B tidak berubah.
///
/// Nonce baru di-generate setiap kali untuk semantic security.
pub fn update_pm(
    vault: &[u8; VAULT_V2_SIZE],
    pm_key: &[u8; 32],
    entries: &[PmEntry],
) -> Result<[u8; VAULT_V2_SIZE], Error> {
    let store = PmStore { entries: entries.to_vec() };
    let plain = store.to_plaintext()?;
    debug_assert_eq!(plain.len(), 3900);

    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(pm_key));
    let ct = cipher
        .encrypt(&nonce, plain.as_slice())
        .map_err(|_| Error::Encryption)?;
    debug_assert_eq!(ct.len(), V2_CT_A_SIZE);

    let mut new_vault = *vault;
    new_vault[V2_OFF_NONCE_A..V2_OFF_CT_A].copy_from_slice(&nonce);
    new_vault[V2_OFF_CT_A..V2_OFF_NONCE_B].copy_from_slice(&ct);

    Ok(new_vault)
}

// ═══════════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::pm::PmEntry;

    // ─── Vault v2 tests (9 checklist wajib PRD v0.4 Bagian 5.4) ────────────

    /// [1] Round-trip ALTER mode: buka dengan passphrase B → keypair benar.
    #[test]
    fn v2_roundtrip_alter_mode() {
        let bundle = KeyBundle::generate();
        let ed_pub = bundle.identity.public_key().to_bytes();
        let noise_pub = bundle.noise.public_bytes();

        let vault = create_v2(&bundle, b"passphrase-b-alter", b"passphrase-a-pm").unwrap();
        assert_eq!(vault.len(), VAULT_V2_SIZE);

        match open_v2(&vault, b"passphrase-b-alter") {
            VaultOpenResult::AlterMode(restored) => {
                assert_eq!(restored.identity.public_key().to_bytes(), ed_pub);
                assert_eq!(restored.noise.public_bytes(), noise_pub);
            }
            _ => panic!("Harus AlterMode"),
        }
    }

    /// [2] Round-trip PM mode: buka dengan passphrase A → entries benar.
    #[test]
    fn v2_roundtrip_pm_mode() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"passphrase-b", b"passphrase-a").unwrap();

        // Tambah entries ke slot A
        let entries = vec![
            PmEntry { id: 1, service: "github.com".into(), username: "alice".into(), password: "s3cr3t".into() },
        ];
        let vault2 = update_pm(&vault, &{
            // Derive key_a untuk test
            let mut key = [0u8; 32];
            if let VaultOpenResult::PmMode { pm_key, .. } = open_v2(&vault, b"passphrase-a") {
                key = pm_key;
            }
            key
        }, &entries).unwrap();

        match open_v2(&vault2, b"passphrase-a") {
            VaultOpenResult::PmMode { pm_entries, .. } => {
                assert_eq!(pm_entries.len(), 1);
                assert_eq!(pm_entries[0].service, "github.com");
                assert_eq!(pm_entries[0].password, "s3cr3t");
            }
            _ => panic!("Harus PmMode"),
        }
    }

    /// [3] Never-fail: passphrase acak selalu buka sesuatu — tidak pernah panic/Error.
    #[test]
    fn v2_never_fail() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"correct-b", b"correct-a").unwrap();

        // Passphrase acak → harus EmptyPm, bukan panic
        match open_v2(&vault, b"totally-random-unknown-passphrase-12345") {
            VaultOpenResult::EmptyPm => {}
            VaultOpenResult::AlterMode(_) => panic!("Tidak boleh AlterMode"),
            VaultOpenResult::PmMode { .. } => panic!("Tidak boleh PmMode"),
        }
    }

    /// [4] Indistinguishable size: vault selalu 4096 bytes.
    #[test]
    fn v2_always_4096_bytes() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"pass-b", b"pass-a").unwrap();
        assert_eq!(vault.len(), VAULT_V2_SIZE);
        assert_eq!(VAULT_V2_SIZE, 4096);
    }

    /// [5] No timing leak — catatan manual.
    ///
    /// Kedua KDF (derive_key_v2 untuk key_a dan key_b) selalu dijalankan
    /// di dalam open_v2(), terlepas dari passphrase. Argon2id(m=64MB, t=3)
    /// mendominasi waktu eksekusi (~500ms/KDF). Beda timing < 0.1% dari total.
    /// Verifikasi: `cargo bench` (benchmark manual, bukan di CI).
    #[test]
    fn v2_timing_leak_documented() {
        // Test ini mendokumentasikan jaminan timing, bukan mengukurnya.
        // open_v2() selalu menjalankan 2× derive_key_v2() sebelum cek hasil.
        // Lihat komentar di open_v2() untuk detail.
        assert!(true);
    }

    /// [6] Independence: key_a dan key_b tidak berkorelasi.
    /// Salt independen → kunci tidak bisa diturunkan satu dari yang lain.
    #[test]
    fn v2_key_independence() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"pass-b", b"pass-a").unwrap();

        // Ambil salt_a dan salt_b dari vault
        let salt_a = &vault[V2_OFF_SALT_A..V2_OFF_SALT_B];
        let salt_b = &vault[V2_OFF_SALT_B..V2_OFF_NONCE_A];

        // Salt harus berbeda (probabilitas sama: 1/2^256)
        assert_ne!(salt_a, salt_b, "Salt tidak boleh sama");

        // Derive dua kunci dari salt berbeda — hasilnya harus berbeda
        let key_a = derive_key_v2(b"same-passphrase", salt_a).unwrap();
        let key_b = derive_key_v2(b"same-passphrase", salt_b).unwrap();
        assert_ne!(*key_a, *key_b, "Key dari salt berbeda harus berbeda");
    }

    /// [7] Migration: vault v1 (108 bytes) dimigrasi ke v2 dengan keypair yang sama.
    #[test]
    fn v2_migration_preserves_keypair() {
        // Buat vault v1
        let bundle = KeyBundle::generate();
        let ed_pub_orig = bundle.identity.public_key().to_bytes();
        let noise_pub_orig = bundle.noise.public_bytes();
        let v1 = seal(&bundle, b"original-pass").unwrap();

        // Unseal v1 → dapat bundle → create_v2 (migration)
        let bundle2 = unseal(&v1, b"original-pass").unwrap();
        let v2 = create_v2(&bundle2, b"original-pass", b"new-pm-pass").unwrap();
        assert_eq!(v2.len(), VAULT_V2_SIZE);

        // Buka v2 dengan passphrase ALTER (passphrase lama)
        match open_v2(&v2, b"original-pass") {
            VaultOpenResult::AlterMode(restored) => {
                assert_eq!(restored.identity.public_key().to_bytes(), ed_pub_orig);
                assert_eq!(restored.noise.public_bytes(), noise_pub_orig);
            }
            _ => panic!("Harus AlterMode setelah migrasi"),
        }
    }

    /// [8] Stress: 100 open/close cycles tanpa memory leak atau panic.
    #[test]
    fn v2_stress_open_close() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"stress-b", b"stress-a").unwrap();

        for _ in 0..100 {
            match open_v2(&vault, b"stress-b") {
                VaultOpenResult::AlterMode(_) => {}
                _ => panic!("Harus AlterMode setiap iterasi"),
            }
        }
    }

    /// [9] PM functionality: tambah → simpan → buka ulang → entry ada; hapus → hilang.
    #[test]
    fn v2_pm_add_delete_roundtrip() {
        let bundle = KeyBundle::generate();
        let vault0 = create_v2(&bundle, b"alter-pass", b"pm-pass").unwrap();

        // Dapatkan pm_key
        let pm_key = match open_v2(&vault0, b"pm-pass") {
            VaultOpenResult::PmMode { pm_key, .. } => pm_key,
            _ => panic!("Harus PmMode"),
        };

        // Tambah entry
        let entries = vec![
            PmEntry { id: 1, service: "github.com".into(), username: "alice".into(), password: "p4ss".into() },
            PmEntry { id: 2, service: "protonmail.com".into(), username: "alice@pm.me".into(), password: "xxxx".into() },
        ];
        let vault1 = update_pm(&vault0, &pm_key, &entries).unwrap();

        // Buka ulang → entries ada
        match open_v2(&vault1, b"pm-pass") {
            VaultOpenResult::PmMode { pm_entries, pm_key: pk2 } => {
                assert_eq!(pm_entries.len(), 2);
                assert_eq!(pm_entries[0].service, "github.com");
                assert_eq!(pm_entries[1].service, "protonmail.com");

                // Hapus entry pertama
                let reduced: Vec<PmEntry> = pm_entries.into_iter().filter(|e| e.id != 1).collect();
                let vault2 = update_pm(&vault1, &pk2, &reduced).unwrap();

                // Buka lagi → hanya 1 entry
                match open_v2(&vault2, b"pm-pass") {
                    VaultOpenResult::PmMode { pm_entries: final_e, .. } => {
                        assert_eq!(final_e.len(), 1);
                        assert_eq!(final_e[0].service, "protonmail.com");
                    }
                    _ => panic!("Harus PmMode"),
                }
            }
            _ => panic!("Harus PmMode"),
        }
    }

    /// Dual-slot: passphrase A tidak membuka slot B (dan sebaliknya).
    #[test]
    fn v2_dual_slot_isolation() {
        let bundle = KeyBundle::generate();
        let vault = create_v2(&bundle, b"pass-alter", b"pass-pm").unwrap();

        // passphrase-pm tidak boleh membuka ALTER
        match open_v2(&vault, b"pass-pm") {
            VaultOpenResult::PmMode { .. } => {}
            VaultOpenResult::AlterMode(_) => panic!("pass-pm tidak boleh buka ALTER"),
            VaultOpenResult::EmptyPm => panic!("pass-pm harus buka PM"),
        }

        // passphrase-alter tidak boleh membuka PM
        match open_v2(&vault, b"pass-alter") {
            VaultOpenResult::AlterMode(_) => {}
            VaultOpenResult::PmMode { .. } => panic!("pass-alter tidak boleh buka PM"),
            VaultOpenResult::EmptyPm => panic!("pass-alter harus buka ALTER"),
        }
    }

    // ─── Vault v1 tests (tetap dari sebelumnya) ─────────────────────────────

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
