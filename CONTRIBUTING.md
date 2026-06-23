# Contributing to ALTER

Terima kasih atas minatmu berkontribusi! Dokumen ini menjelaskan cara kerja proyek, standar kode, dan proses kontribusi.

---

## Daftar Isi

- [Code of Conduct](#code-of-conduct)
- [Cara Berkontribusi](#cara-berkontribusi)
- [Setup Lingkungan](#setup-lingkungan)
- [Struktur Proyek](#struktur-proyek)
- [Standar Kode](#standar-kode)
- [Commit Convention](#commit-convention)
- [Proses Pull Request](#proses-pull-request)
- [Security Policy](#security-policy)
- [Arsitektur & Desain](#arsitektur--desain)

---

## Code of Conduct

Proyek ini mengutamakan diskusi teknis yang jelas dan konstruktif. Yang diharapkan:

- Kritik pada **kode**, bukan pada orang
- Penjelasan yang jelas atas setiap keputusan desain
- Kesabaran dalam review — maintainer mungkin tidak merespons langsung

---

## Cara Berkontribusi

### Bug Reports

Buka [Issue](https://github.com/0xAre/alter/issues) dengan:

```
**Versi ALTER:** (output `alter --version`)
**OS & versi Rust:** (output `rustc --version`)
**Langkah reproduksi:**
1. ...
2. ...
**Yang diharapkan terjadi:**
**Yang sebenarnya terjadi:**
**Log/output:**
```

> ⚠️ Untuk **kerentanan keamanan**, jangan buka issue publik. Lihat [Security Policy](#security-policy).

### Feature Requests

Buka Issue dengan label `enhancement`. Sertakan:
- Use case yang jelas — *siapa* yang butuh, dan *mengapa*
- Apakah ini masuk roadmap (lihat `PRD-alter-v0.3.md`)
- Apakah ada trade-off keamanan yang perlu dipertimbangkan

### Pull Requests

Untuk perubahan kecil (typo, dokumentasi, bug fix sederhana): langsung buka PR.

Untuk perubahan besar (arsitektur, protokol, crypto): **buka Issue dulu** untuk diskusi sebelum menulis kode. Ini menghindari PR besar yang harus di-reject karena arah yang berbeda.

---

## Setup Lingkungan

### Prasyarat

```bash
# Rust toolchain (stable ≥ 1.89)
rustup update stable

# Komponen tambahan
rustup component add clippy rustfmt
```

**Windows tambahan:**
- Visual Studio Build Tools dengan MSVC C++ toolset
- Tidak perlu OpenSSL — SQLite bundled, TLS via rustls

### Clone & Build

```bash
git clone https://github.com/0xAre/alter
cd alter

# Debug build (cepat)
cargo build

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --check
```

### Run Lokal (Testing 2 Instance)

**Terminal 1 (Responder):**
```bash
cargo run -- --listen 9876 --offline
```

**Terminal 2 (Initiator):**
```bash
cargo run -- --dial 127.0.0.1:9876 --offline
```

Kedua instance pakai vault berbeda (`--vault /tmp/a.key` dan `--vault /tmp/b.key`) jika identitas perlu dipisah.

---

## Struktur Proyek

```
alter/
├── src/
│   ├── main.rs              # Entry point: CLI args, bootstrap, spawn TUI
│   ├── error.rs             # Error enum terpusat (ambigu on purpose untuk oracle-safety)
│   │
│   ├── identity/
│   │   ├── keypair.rs       # IdentityKey (Ed25519), NoiseKey (X25519), KeyBundle
│   │   └── vault.rs         # Enkripsi/dekripsi vault: Argon2id + ChaCha20-Poly1305
│   │
│   ├── crypto/
│   │   └── handshake.rs     # HandshakeSession + EncryptedSession (wrapper snow)
│   │
│   ├── contacts/
│   │   └── mod.rs           # Contact struct, invite encode/decode, enkripsi file kontak
│   │
│   ├── session/
│   │   └── mod.rs           # run_session(): state machine Connecting→Handshaking→Active→Closed
│   │
│   ├── transport/
│   │   ├── mod.rs           # establish(): orkestrasi LAN-first → Tor fallback
│   │   ├── frame.rs         # Framing: [2-byte length][payload]
│   │   ├── lan.rs           # mDNS discovery + TCP helper
│   │   └── tor.rs           # TorContext: bootstrap, onion service, connect/accept
│   │
│   └── tui/
│       ├── mod.rs           # App state, event loop, key handlers
│       └── ui.rs            # Rendering (ratatui widgets, stateless)
│
├── PRD-alter-v0.3.md        # Product Requirements Document (spec resmi)
├── Cargo.toml               # Dependencies
└── README.md
```

### Prinsip Desain Antar Modul

- **`transport`** tidak tahu tentang crypto — hanya establish connection
- **`session`** tidak tahu tentang transport internals — hanya dapat stream
- **`crypto`** tidak tahu tentang network — murni wrapping snow
- **`tui`** adalah orchestrator — panggil semua modul lain, tidak ada logic crypto/network langsung

---

## Standar Kode

### Security-First

Semua perubahan yang menyentuh crypto, key material, atau networking **wajib** mempertimbangkan threat model di `PRD-alter-v0.3.md`.

**Aturan wajib:**
```rust
// ✅ Secret key wajib ZeroizeOnDrop
#[derive(ZeroizeOnDrop)]
struct MySecret { key: [u8; 32] }

// ✅ Bersihkan passphrase dengan zeroize, BUKAN clear()
use zeroize::Zeroize;
passphrase.zeroize(); // ✅
passphrase.clear();   // ❌ tidak menimpa memori

// ✅ Error ambigu untuk path kriptografi (cegah oracle attack)
Err(Error::Decryption) // ✅ — sama untuk "passphrase salah" dan "vault rusak"
Err("wrong passphrase") // ❌ — membocorkan informasi

// ✅ Fail closed: jika ada keraguan, tolak koneksi
return Err(Error::IdentityMismatch); // ✅
// ❌ Jangan lanjutkan jika verifikasi gagal
```

### Rust Style

- **Clippy bersih:** `cargo clippy -- -D warnings` harus lulus tanpa peringatan
- **Format:** ikuti `cargo fmt` default (tidak ada kustomisasi `.rustfmt.toml`)
- **Error handling:** gunakan `?` dengan tipe `Error` terpusat, jangan `unwrap()` di production code
- **Async:** tokio runtime, gunakan `tokio::select!` untuk multiplex concurrent operations
- **Komentar:** Indonesia OK untuk komentar internal, English untuk doc comments publik

### Testing

Setiap modul baru wajib punya unit test. Pattern yang diharapkan:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nama_yang_deskriptif() {
        // Arrange
        // Act
        // Assert
    }

    #[tokio::test]
    async fn async_test_dengan_timeout() {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            async { /* test body */ }
        ).await.expect("test timeout");
    }
}
```

**Coverage yang diharapkan:**
- Happy path
- Error path (invalid input, wrong key, closed connection)
- Security properties (fail closed, identity verification)

---

## Commit Convention

Gunakan [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <deskripsi singkat>

[body opsional — jelaskan WHY, bukan WHAT]
```

**Types:**

| Type | Kapan dipakai |
|------|---------------|
| `feat` | Fitur baru |
| `fix` | Bug fix |
| `fix(security)` | Bug fix yang berdampak keamanan |
| `refactor` | Perubahan kode tanpa mengubah behavior |
| `test` | Menambah/memperbaiki test |
| `docs` | Dokumentasi saja |
| `chore` | Dependencies, CI, build scripts |
| `perf` | Optimasi performa |

**Contoh commit yang baik:**
```
fix(security): zeroize passphrase after vault unlock (SEC-04)

String::clear() hanya set len=0 tanpa menimpa memory.
Passphrase bisa recovered dari heap dump. Ganti dengan
zeroize::Zeroize::zeroize() yang secara eksplisit menimpa bytes.
```

**Contoh commit yang kurang baik:**
```
fix stuff         ❌ — tidak deskriptif
update code       ❌ — tidak informatif
WIP               ❌ — jangan push WIP ke main
```

---

## Proses Pull Request

### Checklist Sebelum Submit

```bash
# 1. Tests hijau
cargo test

# 2. Zero warnings
cargo clippy -- -D warnings

# 3. Terformat
cargo fmt --check

# 4. Build release (pastikan tidak ada compile error)
cargo build --release
```

### Template PR

```markdown
## Deskripsi
Jelaskan apa yang berubah dan mengapa.

## Jenis Perubahan
- [ ] Bug fix
- [ ] Fitur baru
- [ ] Perubahan yang breaking backward compatibility
- [ ] Perubahan keamanan

## Checklist
- [ ] `cargo test` — semua pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo fmt --check` — terformat
- [ ] Dokumentasi diperbarui (jika perlu)
- [ ] CHANGELOG diperbarui (jika perlu)

## Security Consideration
[Jika relevan: apakah perubahan ini menyentuh crypto, key material,
networking, atau threat model? Jelaskan implikasinya.]
```

### Review Process

1. Maintainer review dalam beberapa hari (tidak ada SLA formal)
2. Feedback diberikan via komentar PR
3. Request changes harus di-address sebelum merge
4. Squash merge ke `main` — history commit PR tidak dipertahankan

---

## Security Policy

### Melaporkan Kerentanan

**Jangan buka public issue untuk kerentanan keamanan.**

Kirim laporan ke maintainer secara private (lihat profil GitHub untuk kontak). Sertakan:

1. Deskripsi kerentanan
2. Langkah reproduksi
3. Versi ALTER yang terpengaruh
4. Dampak yang potensial
5. Saran mitigasi (opsional)

### Response Time

- Acknowledgment: dalam 72 jam
- Status update: setiap 7 hari
- Fix: bergantung severity

### Scope

**In scope (mohon laporkan):**
- Kelemahan pada Noise handshake atau verifikasi identitas
- Memory leak dari secret material (passphrase, private key)
- Bypass autentikasi
- Kerentanan pada vault encryption

**Out of scope (tidak perlu dilaporkan):**
- Traffic analysis (sudah diketahui, roadmap M3)
- Process name identifiable (sudah diketahui, roadmap M3)
- Kerentanan pada dependencies upstream (laporkan ke upstream)

---

## Arsitektur & Desain

### Prinsip yang Tidak Boleh Dilanggar

1. **No Server** — Tidak boleh ada komunikasi ke server pihak ketiga selain Tor network
2. **Fail Closed** — Jika ada keraguan dalam autentikasi/verifikasi → putus koneksi, jangan lanjutkan
3. **Ephemeral by Design** — Kunci sesi tidak boleh di-persist; `ZeroizeOnDrop` wajib
4. **Oracle Safety** — Error untuk operasi kriptografi harus ambigu (tidak membocorkan info)

### Menambah Fitur yang Menyentuh Crypto

Sebelum membuat perubahan pada `crypto/`, `identity/`, atau `session/`:

1. Baca `PRD-alter-v0.3.md` — terutama bagian Security Tiers
2. Buka Issue dulu untuk diskusi arsitektur
3. Tandai PR dengan label `security-review`
4. Sertakan analisis dampak terhadap threat model

### Menambah Dependencies

Setiap dependency baru wajib dipertimbangkan:

| Pertanyaan | Harapan |
|-----------|---------|
| Apakah di-maintain aktif? | Ya |
| Apakah punya audit independen? | Lebih baik ya |
| Apakah bisa dikurangi dengan stdlib? | Coba dulu |
| Apakah menambah build time signifikan? | Minimal |
| Apakah butuh native library (OpenSSL, dll)? | Hindari — pakai pure-Rust |

---

## Pertanyaan?

Buka [Discussion](https://github.com/0xAre/alter/discussions) atau Issue dengan label `question`.
