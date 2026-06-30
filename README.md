<div align="center">

```
   █████╗ ██╗  ████████╗███████╗██████╗
  ██╔══██╗██║  ╚══██╔══╝██╔════╝██╔══██╗
  ███████║██║     ██║   █████╗  ██████╔╝
  ██╔══██║██║     ██║   ██╔══╝  ██╔══██╗
  ██║  ██║███████╗██║   ███████╗██║  ██║
  ╚═╝  ╚═╝╚══════╝╚═╝   ╚══════╝╚═╝  ╚═╝
```

**Serverless · Encrypted · Sovereign**

*Chat end-to-end terenkripsi tanpa server, tanpa akun, tanpa metadata.*

[![Rust](https://img.shields.io/badge/Rust-1.89+-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-GPL--3.0-blue?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/0xAre/alter?style=flat-square&color=cyan)](https://github.com/0xAre/alter/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/0xAre/alter/release.yml?style=flat-square)](https://github.com/0xAre/alter/actions)
[![PRD](https://img.shields.io/badge/Spec-PRD_v0.4-blueviolet?style=flat-square)](PRD-alter-v0.4.md)

</div>

---

## Apa itu ALTER?

ALTER adalah aplikasi chat terminal yang berjalan **sepenuhnya peer-to-peer** — tidak ada server perantara, tidak ada akun, tidak ada metadata percakapan yang tersimpan di luar perangkatmu.

Dua orang terhubung langsung via **LAN** atau **Tor**, diautentikasi dan dienkripsi menggunakan **Noise Protocol Framework** (IK pattern). Begitu salah satu pihak meninggalkan room, kunci sesi dibuang permanen — pesan lama tidak bisa dibaca ulang oleh siapapun, termasuk pengirimnya sendiri.

> **Room-Bound Sync** — *Ephemeral by design, bukan by policy.*

```
┌─ ALTER ─── ◉ ONLINE ───────────────────────── id:29a40f ─┐
│                                                            │
│  CONTACTS         │  SESI  ·  ◎  Bob              ●       │
│                   │                                        │
│  ▸ ◎  Bob         │  ·  Sesi aman terbuka.                 │
│    ○  Alice       │                                        │
│                   │  →  halo, bro                          │
│                   │  ←  haloo, aman ini?                   │
│                   │  →  ya, E2E via Tor                    │
│                   ├────────────────────────────────────────│
│                   │  › ketik pesan...▏                     │
└───────────────────┴────────────────────────────────────────┘
 [↑↓] pilih   [Enter] sesi   [a] tambah   [i] identitas   [q] keluar
```

---

## Fitur Utama

| Fitur | Detail |
|-------|--------|
| 🔐 **Noise_IK Handshake** | `Noise_IK_25519_ChaChaPoly_BLAKE2s` — mutual auth + forward secrecy + identity hiding dalam satu protokol |
| 🧅 **Tor Built-in** | Onion service persisten dijalankan langsung dari binary — tidak perlu install/jalankan Tor daemon terpisah |
| 🌐 **LAN-first, Tor fallback** | Jika di satu jaringan → koneksi langsung (TCP). Jika lintas internet → otomatis fallback ke Tor |
| 🔑 **Vault Terenkripsi (v2)** | Dual-slot 4096 byte — slot B: ALTER keypair, slot A: Password Manager decoy. Argon2id + ChaCha20-Poly1305. Tanpa magic bytes |
| 👥 **Kontak Terenkripsi** | Daftar kontak tersimpan terenkripsi di disk (ChaCha20-Poly1305, key dari identity) |
| 💨 **Zero Trace** | Semua pesan hanya di RAM. Session key di-`ZeroizeOnDrop` saat room ditutup |
| 📟 **Terminal UI** | Antarmuka ratatui yang bersih, responsif, dengan spinner dan notifikasi real-time |
| 🚫 **Tanpa Server** | Tidak ada backend, tidak ada API, tidak ada akun — murni P2P |

---

## Instalasi

### Prasyarat

- **Rust stable ≥ 1.89** — pasang via [rustup.rs](https://rustup.rs)
- **Windows**: Visual Studio Build Tools (MSVC) — sudah terpasang jika Rust dipasang via rustup dengan MSVC host
- **Linux/macOS**: toolchain C standar (`build-essential` / Xcode CLT)

> Tidak butuh OpenSSL. Tidak butuh menjalankan daemon Tor terpisah. Semuanya bundled.

---

### Option A: Download Binary (Tanpa Install Rust)

Ambil binary siap pakai dari [**Releases**](https://github.com/0xAre/alter/releases):

| Platform | File |
|----------|------|
| Windows x64 | `alter-x86_64-pc-windows-msvc.exe` |
| Linux x64 | `alter-x86_64-unknown-linux-gnu` |
| macOS Apple Silicon | `alter-aarch64-apple-darwin` |

**Windows — installer satu baris:**
```powershell
irm https://raw.githubusercontent.com/0xAre/alter/main/install.ps1 | iex
```
Tutup & buka ulang terminal, lalu ketik `alter`.

---

### Option B: Cargo Install (Jika Rust Sudah Ada)

```bash
cargo install --git https://github.com/0xAre/alter
```

`alter` langsung tersedia di PATH via `~/.cargo/bin`. Tidak perlu setup tambahan.

---

### Option C: Build dari Source

```bash
git clone https://github.com/0xAre/alter
cd alter
cargo build --release
# Binary: ./target/release/alter
```

Atau langsung install ke `~/.cargo/bin`:
```bash
cargo install --path .
```

---

## Pemakaian Cepat

```bash
alter             # Mode ONLINE (LAN + Tor) — default
alter --offline   # Mode LAN murni (tanpa Tor, cocok untuk jaringan internal)
```

TUI muncul seketika — LAN langsung aktif, Tor di-bootstrap di latar belakang. Badge transport berubah dari `⌂ LOCAL` → `◉ ONLINE` saat Tor siap.

---

### Mulai Pertama Kali

```
1. Jalankan alter
2. Set passphrase → identitas dan kunci kriptografi dibuat otomatis
3. Tekan [i] → tampil invite code kamu
4. Bagikan invite code ke peer via channel aman lain (Signal, kertas, dll)
5. Tekan [a] → tempel invite code peer (+ spasi + nickname opsional)
6. Pilih kontak → [Enter] → masuk room terenkripsi
```

Kedua pihak harus menekan `Enter` ke kontak yang sama secara bersamaan. Role (Initiator/Responder) ditentukan otomatis dari perbandingan fingerprint — tidak perlu koordinasi manual.

---

### Keybinding

| Tombol | Konteks | Aksi |
|--------|---------|------|
| `↑` / `↓` | Kontak list | Pilih kontak |
| `Enter` | Kontak list | Buka sesi |
| `a` | Kontak list | Tambah kontak baru |
| `r` | Kontak list | Ganti nama kontak (UX-01) |
| `d` | Kontak list | Hapus kontak (minta konfirmasi) |
| `i` | Mana saja | Tampilkan / tutup invite code |
| `c` | Mana saja | Salin invite code ke clipboard |
| `Enter` | Dalam room | Kirim pesan |
| `Esc` | Dalam room | Keluar room (riwayat dibuang) |
| `n` | PM list | Tambah entri baru (Password Manager) |
| `d` | PM list | Hapus entri (minta konfirmasi) |
| `q` / `Esc` | Kontak list | Keluar aplikasi |
| `Ctrl+X` × 2 | Mana saja | Panic wipe — zeroize semua secret, exit |
| `Ctrl+C` | Mana saja | Force quit |

---

### Opsi CLI

```
alter [opsi]            Jalankan TUI
alter id [opsi]         Cetak invite code lalu keluar (untuk skrip/automasi)

Opsi:
  --vault <path>        Lokasi vault (default: ~/.alter/id.key)
  --offline             Matikan Tor — LAN murni, cocok untuk jaringan internal
  --add <invite>        Pra-muat satu kontak saat startup
  --name <nickname>     Nickname untuk kontak --add
  --listen <port>       Paksa mode responder, listen di port ini (testing)
  --dial <ip:port>      Paksa mode initiator, dial langsung (testing)
  -h, --help            Tampilkan bantuan
```

**Passphrase via environment** (untuk automasi):
```bash
ALTER_PASSPHRASE="passphraseku" alter id
```

---

## Arsitektur Keamanan

### Cryptographic Stack

```
┌─────────────────────────────────────────────────────┐
│                    APPLICATION                       │
├─────────────────────────────────────────────────────┤
│  NOISE TRANSPORT                                     │
│  Noise_IK_25519_ChaChaPoly_BLAKE2s                   │
│  ├─ Mutual authentication (kedua identitas diverif.) │
│  ├─ Forward secrecy (ephemeral DH tiap sesi)         │
│  └─ Identity hiding (static key initiator dienkr.)  │
├─────────────────────────────────────────────────────┤
│  TRANSPORT                                           │
│  ├─ LAN: TCP direct (mDNS discovery)                 │
│  └─ Tor: Onion service via arti-client               │
├─────────────────────────────────────────────────────┤
│  IDENTITY VAULT (v2)                                 │
│  Dual-slot 4096 B — Argon2id + ChaCha20-Poly1305     │
│  Slot A: PM decoy · Slot B: ALTER keys               │
│  Indistinguishable from random data (SEC-05)         │
└─────────────────────────────────────────────────────┘
```

### Properti Keamanan (PRD v0.3 Tier 0)

| Property | Implementasi |
|----------|-------------|
| **Mutual Auth** | Noise_IK — kedua pihak memverifikasi static key lawan |
| **Forward Secrecy** | Ephemeral X25519 DH per sesi, dibuang setelah handshake |
| **Identity Hiding** | Static key initiator dienkripsi (`es`) di message pertama |
| **Fail Closed** | Jika identity mismatch → koneksi langsung diputus, tidak dilanjutkan |
| **Zero Memory Leak** | `ZeroizeOnDrop` pada semua struct yang menyimpan secret material |
| **Plausible Deniability** | Vault 4096 B tanpa header/magic — tidak bisa diidentifikasi tanpa passphrase. Passphrase decoy membuka Password Manager |
| **Encrypted Contact List** | Social graph dienkripsi di disk — tidak plaintext |

### Threat Model

ALTER dirancang untuk:
- ✅ Mengamankan konten percakapan dari network observer
- ✅ Menyembunyikan identitas dari operator infrastruktur
- ✅ Ephemeral sessions — tidak ada history yang bisa disita
- ✅ Mutual authentication — tidak bisa di-MITM tanpa private key

ALTER **tidak** dirancang untuk:
- ❌ Perlindungan jika endpoint dikompromisikan
- ❌ Anonimitas mutlak (traffic correlation attack via Tor relay tetap mungkin)
- ❌ Perlindungan saat laptop hibernate (mlock tidak melindungi RAM dump ke disk)

> ⚠️ **Status: v0.5.0 — belum diaudit pihak ketiga.** Gunakan dengan pertimbangan risiko yang sesuai.

---

## Status Pengembangan

```
M0 ████████████ Fondasi: identity, vault, Noise_IK                    ✅ Done
M1 ████████████ LAN MVP: mDNS, TCP, TUI, chat 1-on-1                  ✅ Done
M2 ████████████ Jalur internet: Tor onion + LAN fallback               ✅ Done
M3 ████████████ Hardening: padding, panic-wipe, process-name, mlock    ✅ Done
M4 ████████████ Polish & audit (hidden passphrase, onboarding)         ✅ Done
M5 ████████████ Presence privacy: Restricted Discovery, lyrebird       ✅ Done
M6 ████████████ Password Manager decoy front (dual-slot vault v2)      ✅ Done
```

### Changelog Terbaru

**v0.5.0** — Password Manager Decoy Front (M6) + async unlock
- Vault v2 (4096 B): dual-slot independent — slot A (PM) + slot B (ALTER)
- Password Manager TUI fungsional: tambah/lihat/hapus/cari entries
- Backup codes per entry (maks 10, mark-as-used)
- Async unlock dengan spinner (Argon2id ~500ms di background thread)
- 9 test checklist vault v2 wajib (PRD v0.4 Bagian 5.4) — semua pass

---

## Kontribusi

Lihat [CONTRIBUTING.md](CONTRIBUTING.md) untuk panduan lengkap.

Secara singkat:
1. Fork → buat branch dari `main`
2. Buat perubahan, pastikan `cargo test` hijau dan `cargo clippy` bersih
3. Commit dengan format [Conventional Commits](https://www.conventionalcommits.org/)
4. Buka Pull Request

---

## Lisensi

ALTER dirilis di bawah **GNU General Public License v3.0** — lihat [LICENSE](LICENSE) untuk teks lengkapnya.

Singkatnya: bebas digunakan, dipelajari, dan dimodifikasi. Fork dan distribusi wajib tetap GPL-3.0 dan open source.

---

<div align="center">

*"Privacy is not about having something to hide — it's about having something to protect."*

**[Releases](https://github.com/0xAre/alter/releases) · [Issues](https://github.com/0xAre/alter/issues) · [PRD](PRD-alter-v0.3.md)**

</div>
