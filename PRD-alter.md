# PRD: ALTER — Serverless Encrypted P2P Terminal Chat

> **Status dokumen:** Draft v0.1 — untuk review
> **Tanggal:** 22 Juni 2026
> **Nama proyek:** **ALTER** — final, locked. Catatan penting: nama ini dipakai untuk repo, dokumentasi, dan percakapan tim. Nama proses runtime (binary yang berjalan di device) TIDAK memakai nama ini secara default — lihat SEC-09.
> **Audiens dokumen:** Ditulis supaya bisa dipahami 3 sudut pandang berbeda — visi produk (apa & kenapa), value proposition (untuk siapa & berapa cost), dan spesifikasi teknis (bagaimana & seberapa kuat).

---

## 1. Ringkasan Eksekutif

ALTER adalah aplikasi chat berbasis terminal (CLI/TUI) yang memungkinkan dua orang berkomunikasi end-to-end terenkripsi **tanpa server perantara** yang menyimpan atau merelay pesan. Identitas pengguna berbasis kriptografi (bukan nomor telepon/email), koneksi terjadi langsung antar dua perangkat — baik dalam jaringan lokal (LAN) maupun lintas internet via jaringan Tor sebagai infrastruktur rendezvous publik.

**Yang membedakan dari WhatsApp/Telegram/Signal:** tidak ada perusahaan, tidak ada server, tidak ada akun yang bisa dibekukan atau disubpoena, tidak ada metadata percakapan yang tersimpan di pihak ketiga manapun — karena pihak ketiga itu tidak ada.

**Trade-off yang disengaja:** kedua pihak harus online bersamaan untuk bisa chat (model "walkie-talkie"), bukan model asinkron seperti SMS. Ini konsekuensi langsung dari prinsip serverless — tidak ada tempat menitip pesan saat lawan offline.

---

## 2. Problem Statement

Aplikasi chat mainstream (WhatsApp, Telegram, bahkan Signal) tetap punya **satu titik kegagalan struktural**: server pihak ketiga. Walau enkripsi end-to-end melindungi *isi* pesan, server tersebut tetap:

- Tahu **siapa berkomunikasi dengan siapa** (metadata sosial graph)
- Tahu **kapan dan seberapa sering** (traffic pattern)
- Bisa **dipaksa oleh otoritas hukum/negara** untuk membuka data yang mereka punya
- Jadi **target tunggal** untuk peretasan/kebocoran skala besar

Untuk pengguna dengan kebutuhan privasi tinggi (jurnalis, aktivis, profesional keamanan siber, atau siapapun dengan model ancaman serius), titik kegagalan ini tidak bisa diterima — berapa pun kuat enkripsinya, *siapa yang mengontrol infrastruktur* tetap jadi celah.

**Hipotesis produk:** ada kebutuhan nyata untuk alat komunikasi yang menghilangkan server sepenuhnya dari ekuasi, menerima trade-off kenyamanan demi menghilangkan titik kegagalan tersebut secara struktural — bukan ditambal dengan kebijakan privasi.

---

## 3. Tujuan Produk

### Goals
1. Komunikasi dua pihak yang **terenkripsi end-to-end** dengan primitif kriptografi modern teraudit (bukan buatan sendiri)
2. **Tidak ada server** yang menyimpan, merelay, atau punya visibilitas ke pesan maupun metadata percakapan
3. Berjalan **dual-mode**: LAN (cepat, lokal) dan internet (via Tor, lintas jaringan)
4. Antarmuka **CLI/TUI** yang efisien dan nyaman dipakai (referensi UX: Claude Code) — bukan REPL mentah
5. **Secure by design** di setiap layer yang cost-nya proporsional dengan reduksi risiko (lihat Bagian 7)
6. Single binary, **cross-platform** (Linux, macOS, Windows minimal via WSL)

### Non-Goals (sengaja TIDAK dikerjakan di v1)
- ❌ Pesan asinkron / offline messaging (konsekuensi dari "Goal #2" — tidak bisa keduanya)
- ❌ Group chat / multi-party (kompleksitas kriptografi group key management jauh lebih besar; v1 fokus 1-on-1 dulu)
- ❌ File transfer besar / voice call (scope creep; fokus dulu ke teks)
- ❌ Multi-device sync untuk satu identitas (sudah diputuskan: 1 identitas = 1 perangkat)
- ❌ Mobile app (CLI-first; mobile adalah pertimbangan terpisah di masa depan)

---

## 4. Target Pengguna

| Persona | Kebutuhan | Contoh use case |
|---|---|---|
| **Profesional keamanan siber** | Komunikasi sensitif tanpa jejak korporat | Diskusi temuan vulnerability sebelum disclosure publik |
| **Jurnalis / peneliti** | Lindungi sumber, anti-surveillance | Koordinasi dengan sumber di lingkungan represif |
| **Tim teknis privasi-sadar** | Alternatif Slack/WhatsApp untuk topik sensitif | Diskusi internal yang tidak ingin masuk log perusahaan |
| **Individu dengan threat model tinggi** | Privasi maksimal dari pengawasan negara/ISP | Komunikasi pribadi yang menolak titik kegagalan tunggal |

---

## 5. Model Ancaman (Threat Model)

Ini bagian paling kritis dari PRD ini — **semua keputusan desain di bawah diturunkan dari sini**, bukan sebaliknya.

**Asumsi adversary: state-level.** Artinya kita asumsikan penyerang punya kapabilitas:
- Memantau traffic di level ISP/backbone (passive network monitoring)
- Melakukan Deep Packet Inspection (DPI) untuk fingerprinting protokol
- Berpotensi melakukan penyitaan perangkat fisik (forensik disk & memory)
- **TIDAK** diasumsikan punya kemampuan membobol primitif kriptografi modern (AEAD, Curve25519/Ed25519, Noise Protocol) — itu di luar scope yang realistis untuk produk software apapun

**Eksplisit DI LUAR scope perlindungan** (harus jujur ke semua stakeholder, ini bukan kelemahan tersembunyi):
- Akses fisik ke perangkat **saat aplikasi sedang berjalan** (live memory dump) — tidak ada software yang bisa menahan ini sepenuhnya
- Cold boot attack tingkat hardware — butuh mitigasi hardware (TPM/secure enclave), bukan domain aplikasi
- Kompromi di endpoint (keylogger, malware di OS) — di luar kendali aplikasi chat manapun
- Deniability hukum mutlak — "menggunakan Tor" itu sendiri bisa jadi sinyal yang dicurigai di sebagian jaringan, walau isi tidak terbaca

---

## 6. Prinsip Desain ("Secure by Design, Tricky, Efisien")

1. **Setiap fitur keamanan harus punya rasio reduksi-risiko : biaya-implementasi yang jelas** — bukan ditambahkan karena "kedengaran aman". Lihat tabel Tier di Bagian 7.
2. **Minim metadata by default.** Tidak ada version string di handshake, tidak ada nama proses yang identifiable, tidak ada filename yang menjelaskan diri sendiri.
3. **RAM-only untuk data sensitif.** Pesan tidak pernah ditulis ke disk dalam bentuk apapun, termasuk swap (dimitigasi via `mlock`).
4. **Fail closed, bukan fail open.** Kalau handshake gagal verifikasi, koneksi putus total — tidak ada fallback ke mode "kurang aman" secara diam-diam.
5. **Auditable di atas clever.** Pilih primitif kriptografi yang sudah diaudit luas (Noise Protocol, dipakai WireGuard) daripada desain custom yang "lebih pintar" tapi belum pernah diuji publik.

---

## 7. Spesifikasi Keamanan per Tier

Fitur dikelompokkan berdasarkan rasio value/cost, supaya effort development terarah ke hal paling berdampak dulu.

### Tier 0 — Wajib, fondasi (v1 tidak bisa ship tanpa ini)
| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-01 | Noise_IK handshake (mutual auth + session key) | Fondasi seluruh keamanan transport |
| SEC-02 | Passphrase + Argon2id untuk enkripsi private key at rest | Mencegah private key terbaca langsung dari disk |
| SEC-03 | Chat history hanya di RAM, tidak pernah ditulis ke disk | Menutup vektor forensik disk untuk isi percakapan |
| SEC-04 | `zeroize` + `mlock()` pada secret di memory | Mengurangi jejak di RAM dan mencegah swap-to-disk |
| SEC-05 | Nama file vault generik (bukan `alter_config.dat`) | Mengurangi metadata leak level OS |

### Tier 1 — Dampak besar, effort sedang
| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-06 | Pluggable transport obfs4 untuk traffic Tor | Menyamarkan traffic agar tidak mudah di-fingerprint via DPI |
| SEC-07 | Padding ciphertext ke ukuran blok fixed | Mencegah panjang pesan asli bocor dari panjang ciphertext |
| SEC-08 | Tidak ada version/identifier string di handshake payload | Mencegah fingerprinting aplikasi via pola protokol |

### Tier 2 — "Tricky", cost rendah, value tinggi untuk threat model ini
| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-09 | Nama proses binary generik & **configurable per-install** (BUKAN hardcoded "alter" atau nama proyek apapun) | Mengurangi sinyal di `ps aux`/Task Manager. Default config harus pakai nama plausible seperti `update-agent`, `sync-helper`, dst — dan user bisa override sendiri (`--process-name=xxx`) supaya satu nama gak jadi fingerprint universal untuk semua instalasi ALTER di seluruh dunia |
| SEC-10 | Onion service on-demand, mati saat app ditutup | Menghindari onion address jadi sinyal presence 24/7 |
| SEC-11 | Hotkey "panic wipe" — wipe RAM + matikan onion service instan | Mitigasi skenario perangkat akan disita saat aplikasi aktif |

### Tier 3 — Eksplisit DI-SKIP untuk v1 (dengan alasan)
| Fitur | Kenapa skip |
|---|---|
| Secure-delete tingkat disk di app-level | Tidak efektif di SSD modern (wear-leveling); domain yang benar adalah full-disk encryption OS-level |
| Decoy/duress vault (password kedua) | Risiko bug tinggi, bisa menyebabkan kehilangan data tidak sengaja; dipertimbangkan ulang setelah v1 stabil |
| Cold-boot RAM protection | Membutuhkan mitigasi hardware (TPM), di luar kendali software aplikasi |

---

## 8. Arsitektur Teknis

### 8.1 Diagram Alur

```
┌─────────────────────────────────────────────┐
│              TUI Layer (ratatui)             │
│  [Contact List] [Chat Pane] [Input] [Status] │
└───────────────────┬───────────────────────────┘
                     │
┌────────────────────▼───────────────────────────┐
│           Identity & Contact Store              │
│  - Ed25519 keypair (lokal, encrypted at rest)   │
│  - Contact = {nickname, pubkey, onion_addr}     │
└────────────────────┬───────────────────────────┘
                     │
        ┌────────────┴────────────┐
        ▼                         ▼
┌───────────────┐         ┌───────────────────┐
│  LAN PATH      │         │  INTERNET PATH     │
│  mDNS discover │         │  Tor onion service │
│  Direct TCP    │         │  + obfs4 transport │
└───────┬────────┘         └─────────┬──────────┘
        │   (coba dulu, timeout)     │ (fallback)
        └────────────┬───────────────┘
                     ▼
        ┌─────────────────────────┐
        │   Noise_IK Handshake     │
        └────────────┬────────────┘
                     ▼
        ┌─────────────────────────┐
        │  Encrypted Chat Session  │
        │  (RAM-only, no disk I/O) │
        └─────────────────────────┘
```

### 8.2 Modul (struktur kode)

```
src/
├── identity/      → Keypair generation, vault encryption (Argon2id + ChaCha20Poly1305)
├── contacts/       → Contact store, invite code encode/decode
├── transport/       → LAN (mDNS+TCP), Tor (onion + obfs4), dialer (LAN-first-fallback-Tor)
├── crypto/          → Noise_IK handshake, AEAD session, padding (stateless, paling mudah diaudit)
├── session/         → State machine: Connecting → Handshaking → Active → Closed
└── tui/              → Contact list, chat pane, input, unlock screen, panic hotkey
```

### 8.3 Primitif Kriptografi

| Fungsi | Primitif | Library (Rust) |
|---|---|---|
| Identitas | Ed25519 | `ed25519-dalek` |
| Handshake | Noise_IK | `snow` |
| Enkripsi pesan | AEAD (ChaCha20-Poly1305) | `chacha20poly1305` |
| Key derivation dari passphrase | Argon2id | `argon2` |
| Wipe memory | Explicit zeroing | `zeroize` |
| LAN discovery | mDNS | `mdns-sd` |
| Anonymity network | Tor (onion service) | `arti` *(perlu validasi maturity)* |
| Traffic obfuscation | obfs4 | TBD — riset pustaka Rust vs binding ke `obfs4proxy` |

### 8.4 Alasan Pilihan Bahasa: Rust
Memory safety (mencegah buffer overflow/use-after-free di kode yang menangani secret), performa setara C/C++, dan ekosistem kripto yang matang (`snow`, `ed25519-dalek` dipakai produksi di proyek lain).

---

## 9. Alur Pengguna (UX)

1. **Buka app** → prompt passphrase (unlock vault lokal)
2. **Tampilan utama**: panel kontak kiri, panel chat kanan — estetika terminal mirip Claude Code (clean, monospace, minim chrome visual)
3. **Tambah kontak baru**: tukar "kode kontak" (1 string berisi pubkey + onion address) lewat channel lain (manual, di luar app)
4. **Pilih kontak** → app coba LAN dulu (mDNS, ~3 detik timeout) → fallback ke Tor kalau tidak ketemu di LAN
5. **Status koneksi eksplisit ditampilkan**: "Mencari di LAN...", "Membangun sirkuit Tor...", "Terhubung" — supaya user paham kenapa ada delay
6. **Chat aktif** → pesan terenkripsi real-time, history hanya hidup selama sesi
7. **Tutup sesi/app** → seluruh state di-wipe dari memory, tidak ada jejak tersisa

---

## 10. Batasan & Yang Tidak Dijamin (Honesty Section)

Bagian ini wajib dibaca semua stakeholder sebelum approve — supaya ekspektasi realistis:

- **Tidak ada pesan offline.** Kedua pihak harus online bersamaan. Ini bukan bug, ini konsekuensi arsitektur serverless yang disengaja.
- **"Anti-forensik" bukan jaminan absolut.** Kita menutup vektor forensik disk & memory standar, tapi TIDAK bisa menahan akses fisik saat perangkat menyala (live forensics) atau serangan hardware-level.
- **Penggunaan Tor bisa terlihat** di jaringan yang dimonitor ketat, walau isi pesan tidak bisa dibaca. obfs4 mengurangi ini, tidak menghilangkan sepenuhnya.
- **Operational security ada di luar kendali aplikasi** — shell history, kebiasaan pengguna, keamanan fisik perangkat, semua itu domain pengguna, bukan sesuatu yang software bisa paksa.

---

## 11. Metrik Keberhasilan (v1)

| Metrik | Target |
|---|---|
| Waktu koneksi LAN | < 3 detik |
| Waktu koneksi Tor (cold start) | < 60 detik |
| Ukuran binary | < 20 MB (single binary, tanpa dependency eksternal yang harus diinstal user) |
| Crypto code coverage (unit test) | 100% pada modul `crypto/` |
| Tidak ada plaintext di disk | Diverifikasi via audit manual + automated test (cek filesystem setelah sesi chat) |

---

## 12. Roadmap

| Fase | Scope |
|---|---|
| **M0 — Fondasi** | `identity/` + `crypto/handshake.rs`, unit test Noise_IK roundtrip lokal |
| **M1 — LAN MVP** | Transport LAN selesai, TUI skeleton, chat 1-on-1 jalan di satu jaringan |
| **M2 — Internet path** | Integrasi Tor onion service, fallback logic, validasi `arti` |
| **M3 — Hardening** | Tier 1 & 2 (obfs4, padding, panic wipe, generic naming) |
| **M4 — Polish & Audit** | Review keamanan internal, dokumentasi, cross-platform build |

---

## 13. Risiko & Open Questions

| Risiko/Pertanyaan | Catatan |
|---|---|
| Maturity `arti` untuk hosting onion service (bukan cuma client) belum divalidasi | Perlu riset/POC sebelum M2 dimulai |
| Library obfs4 di Rust belum dipastikan ada/matang | Mungkin perlu binding ke implementasi C/Go yang sudah ada |
| Implikasi institusional | Pertimbangkan apakah ada kebijakan organisasi terkait pengembangan tooling anti-forensik/anti-surveillance secara independen — pertanyaan administratif, bukan teknis |
| Nama final aplikasi | ✅ **Resolved** — "ALTER" dipilih (stealth-by-name: tidak menyebut fungsi komunikasi secara eksplisit). Nama proses runtime tetap terpisah & configurable, lihat SEC-09. |

---

## 14. Glosarium (untuk pembaca non-teknis)

- **End-to-end encryption**: hanya pengirim dan penerima yang bisa baca isi pesan, tidak ada pihak di tengah (termasuk pembuat app) yang bisa membacanya.
- **Serverless**: tidak ada komputer pusat yang menyimpan/merelay pesan; dua perangkat bicara langsung.
- **Tor / onion service**: jaringan anonimitas terdistribusi yang menyembunyikan alamat IP asli pengguna.
- **Handshake**: proses awal dua aplikasi "berkenalan" dan menyepakati kunci enkripsi sebelum chat mulai.
- **Forensik digital**: proses memeriksa perangkat (disk, memory) untuk menemukan jejak aktivitas/data.
- **Metadata**: data tentang data — misalnya bukan ISI pesan, tapi FAKTA bahwa pesan pernah dikirim, kapan, ke siapa.

---

*Akhir dokumen. Silakan beri komentar per section — bagian mana yang perlu didetailkan lagi atau dipotong scope-nya.*
