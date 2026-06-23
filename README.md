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
[![License](https://img.shields.io/badge/License-TBD-gray?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/0xAre/alter?style=flat-square&color=cyan)](https://github.com/0xAre/alter/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/0xAre/alter/release.yml?style=flat-square)](https://github.com/0xAre/alter/actions)
[![PRD](https://img.shields.io/badge/Spec-PRD_v0.3-blueviolet?style=flat-square)](PRD-alter-v0.3.md)

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
| 🔑 **Vault Terenkripsi** | Identity key dienkripsi Argon2id + ChaCha20-Poly1305. File 108 byte, tanpa magic bytes |
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
| `d` | Kontak list | Hapus kontak (minta konfirmasi) |
| `i` | Mana saja | Tampilkan / tutup invite code |
| `c` | Mana saja | Salin invite code ke clipboard |
| `Enter` | Dalam room | Kirim pesan |
| `Esc` | Dalam room | Keluar room (riwayat dibuang) |
| `q` / `Esc` | Kontak list | Keluar aplikasi |
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
│  IDENTITY VAULT                                      │
│  Argon2id (m=19MiB, t=2, p=1) + ChaCha20-Poly1305   │
│  108 bytes — indistinguishable from random data      │
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
| **Plausible Deniability** | Vault file 108 byte tanpa header/magic — tidak bisa diidentifikasi tanpa passphrase |
| **Encrypted Contact List** | Social graph dienkripsi di disk — tidak plaintext |

### Threat Model

ALTER dirancang untuk:
- ✅ Mengamankan konten percakapan dari network observer
- ✅ Menyembunyikan identitas dari operator infrastruktur
- ✅ Ephemeral sessions — tidak ada history yang bisa disita
- ✅ Mutual authentication — tidak bisa di-MITM tanpa private key

ALTER **tidak** dirancang untuk (belum, roadmap M3):
- ❌ Traffic analysis resistance (obfs4/padding belum diimplementasikan)
- ❌ Perlindungan jika endpoint dikompromisikan
- ❌ Plausible deniability terhadap process name `alter`

> ⚠️ **Status: Pre-rilis (v0.1.x).** Belum diaudit oleh pihak ketiga. Gunakan untuk eksperimen, bukan situasi kritis.

---

## Status Pengembangan

```
M0 ████████████ Fondasi: identity, vault, Noise_IK          ✅ Done
M1 ████████████ LAN MVP: mDNS, TCP, TUI, chat 1-on-1        ✅ Done
M2 ████████████ Jalur internet: Tor onion + LAN fallback     ✅ Done
M3 ░░░░░░░░░░░░ Hardening: obfs4, padding, panic-wipe        ⏳ Planned
M4 ░░░░░░░░░░░░ Polish & audit internal                      ⏳ Planned
```

### Changelog Terbaru

**v0.1.8** — Audit & Security Fixes
- Fix: Deadlock pada role negotiation → model deterministik fingerprint-based
- Fix: ZeroizeOnDrop pada `SelfKeys` (noise_sk wajib di-wipe, SEC-04)
- Fix: `String::clear()` → `zeroize()` untuk passphrase (SEC-04)
- Fix: Cegah self-add sebagai kontak (deadlock role)
- Fix: `set_var` dipindah sebelum tokio runtime (UB Rust ≥ 1.83)
- Fix: Hapus dead code dari v0.1.7 (symmetric connect removal)
- Improvement: Tor dial dengan retry + exponential backoff
- Improvement: Tor accept dengan timeout (120 detik)
- Improvement: mDNS hanya iklankan IP LAN asli (skip link-local 169.254.x)

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

Lisensi belum ditetapkan secara formal. Hubungi maintainer sebelum menggunakan dalam produk lain.

---

<div align="center">

*"Privacy is not about having something to hide — it's about having something to protect."*

**[Releases](https://github.com/0xAre/alter/releases) · [Issues](https://github.com/0xAre/alter/issues) · [PRD](PRD-alter-v0.3.md)**

</div>
