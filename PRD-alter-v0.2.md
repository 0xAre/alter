# PRD: ALTER — Serverless Encrypted P2P Terminal Chat

> **Status dokumen:** Draft v0.2 — revisi berbasis riset teknis
> **Tanggal v0.1:** 22 Juni 2026
> **Tanggal v0.2:** 22 Juni 2026
> **Nama proyek:** **ALTER** — final, locked. Catatan penting: nama ini dipakai untuk repo, dokumentasi, dan percakapan tim. Nama proses runtime (binary yang berjalan di device) TIDAK memakai nama ini secara default — lihat SEC-09.
> **Audiens dokumen:** Ditulis supaya bisa dipahami 3 sudut pandang berbeda — visi produk (apa & kenapa), value proposition (untuk siapa & berapa cost), dan spesifikasi teknis (bagaimana & seberapa kuat).
> **Changelog v0.2:** Lihat Bagian 15 di akhir dokumen.

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
- **[baru v0.2]** Online/offline presence detection by unauthenticated parties — lihat risiko R-05 di Bagian 13

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
| **[baru v0.2]** Per-peer endpoint onion service (Gosling pattern) | Mencegah presence detection oleh unauthenticated third party — pattern ini dipakai Ricochet. Cost implementasi tinggi untuk v1; dipertimbangkan di v2. |

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

> **Catatan v0.2:** Tabel ini diperbarui dengan versi konkret hasil riset (Juni 2026). Setiap baris mencantumkan sumber verifikasi. Kolom "Catatan" merangkum temuan kritis yang perlu diperhatikan saat implementasi.

| Fungsi | Primitif | Library (Rust) | Versi | Catatan |
|---|---|---|---|---|
| Identitas | Ed25519 | `ed25519-dalek` | **2.2.0** | RUSTSEC-2024-0344 mempengaruhi dependensi `curve25519-dalek`; pastikan menggunakan `curve25519-dalek >= 4.1.3` (sudah included di dalek 2.x terbaru). Sumber: [rustsec.org/advisories/RUSTSEC-2024-0344](https://rustsec.org/advisories/RUSTSEC-2024-0344.html) |
| Handshake | Noise_IK | `snow` | **0.10.0** | Mendukung Noise_IK penuh, tracking spec revision 34. **Peringatan: belum ada formal security audit.** Dipilih karena sama-sama dipakai WireGuard (bukan crate `snow` yang sama, tapi protokolnya identik). Sumber: [github.com/mcginty/snow](https://github.com/mcginty/snow) |
| Enkripsi pesan | AEAD (ChaCha20-Poly1305) | `chacha20poly1305` | **0.10.1** | ✅ Diaudit oleh NCC Group (2019, atas nama MobileCoin). Tidak ada CVE aktif untuk crate ini. RustCrypto project. Sumber: [crates.io/crates/chacha20poly1305](https://crates.io/crates/chacha20poly1305) |
| Key derivation dari passphrase | Argon2id | `argon2` | **0.5.3** | Stable. v0.6.0-rc (RC, belum stable). **Parameter yang direkomendasikan OWASP 2024: m=19 MiB, t=2 iterasi, p=1 parallelism.** Default crate sudah sesuai rekomendasi ini. Sumber: [rustcrypto.org/key-derivation/hashing-password.html](https://rustcrypto.org/key-derivation/hashing-password.html) |
| Wipe memory | Explicit zeroing | `zeroize` | **1.x** | Stable, banyak dipakai ekosistem RustCrypto. |
| LAN discovery | mDNS | `mdns-sd` | **0.20.0** | Aktif dimainte (75 versi, 2.9M+ downloads). Supports macOS/Linux/Windows, IPv4+IPv6. Sumber: [crates.io/crates/mdns-sd](https://crates.io/crates/mdns-sd) |
| Anonymity network | Tor (onion service) | `arti-client` | **0.43.x** (arti 1.4.0) | ✅ Onion service hosting sudah didukung via `TorClient::launch_onion_service()`. **Caveat penting:** API masih pre-1.x stable (breaking changes masih terjadi), memerlukan feature flags `onion-service-service`. Rekomendasi: pin versi eksplisit di Cargo.toml dan alokasikan waktu upgrade reguler. Sumber: [blog.torproject.org/arti_1_4_0_released](https://blog.torproject.org/arti_1_4_0_released) |
| Traffic obfuscation | obfs4 | **⚠️ TIDAK ADA implementasi Rust yang matang** | alpha | Satu-satunya pure Rust implementation (`jmwample/ptrs`) masih `0.1.0-alpha.1` dengan warning eksplisit "under construction". **Untuk M3: gunakan `obfs4proxy` (Go binary) via subprocess.** Lihat implikasi di Bagian 11 dan Bagian 13. Sumber: [github.com/jmwample/ptrs](https://github.com/jmwample/ptrs), [docs.rs/obfs4](https://docs.rs/obfs4) |

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
- **[baru v0.2] Online/offline status.** Siapapun yang mengetahui onion address kamu (bukan hanya kontak terdaftar) secara teoritis bisa mendeteksi kapan kamu online dengan mencoba koneksi ke onion tersebut. SEC-10 (on-demand onion) mengurangi exposure window, tapi tidak menghilangkan risiko selama sesi aktif. Ini trade-off yang disengaja di v1; solusi Gosling-pattern (per-peer endpoint) dipertimbangkan di v2.

---

## 11. Metrik Keberhasilan (v1)

| Metrik | Target | Catatan v0.2 |
|---|---|---|
| Waktu koneksi LAN | < 3 detik | — |
| Waktu koneksi Tor (cold start) | < 60 detik | Perlu divalidasi dengan arti-client di POC; onion service bootstrap bisa bervariasi |
| Ukuran binary | < 20 MB (single binary) | **⚠️ PERLU APPROVAL:** Jika obfs4 diimplementasikan via bundling `obfs4proxy` Go binary, target ini kemungkinan tidak tercapai tanpa perubahan strategi. Dua opsi: (A) naikkan target ke < 50 MB, (B) jadikan obfs4proxy sebagai opsional external dependency yang dideteksi runtime. Lihat risiko R-03. |
| Crypto code coverage (unit test) | 100% pada modul `crypto/` | — |
| Tidak ada plaintext di disk | Diverifikasi via audit manual + automated test (cek filesystem setelah sesi chat) | — |

---

## 12. Roadmap

| Fase | Scope | Catatan v0.2 |
|---|---|---|
| **M0 — Fondasi** | `identity/` + `crypto/handshake.rs`, unit test Noise_IK roundtrip lokal | — |
| **M1 — LAN MVP** | Transport LAN selesai, TUI skeleton, chat 1-on-1 jalan di satu jaringan | — |
| **M2 — Internet path** | Integrasi Tor onion service via `arti-client`, fallback logic | Pin `arti-client` ke versi konkret. Pastikan feature flags `onion-service-service` aktif. |
| **M3 — Hardening** | Tier 1 & 2 (obfs4, padding, panic wipe, generic naming) | **Implikasi baru:** obfs4 via `obfs4proxy` binary (bukan Rust native). Perlu keputusan arsitektur: subprocess spawn vs IPC. Tambah ke scope M3: evaluasi `jmwample/ptrs` untuk menentukan apakah sudah cukup matang di waktu M3 dimulai. |
| **M4 — Polish & Audit** | Review keamanan internal, dokumentasi, cross-platform build | Tambahkan: review RUSTSEC advisory untuk semua dependency sebelum release. |

---

## 13. Risiko & Open Questions

### Resolved (ditemukan jawaban dari riset)

| Item | Status | Jawaban |
|---|---|---|
| Maturity `arti` untuk hosting onion service | ✅ **Resolved** | Supported via `TorClient::launch_onion_service()` di `arti-client >= 0.43.x`. Feature flag `onion-service-service` wajib diaktifkan. API belum declared stable (pre-1.x) — pin versi dan alokasikan maintenance overhead reguler. |
| Library obfs4 di Rust | ✅ **Resolved** | Tidak ada crate matang. `jmwample/ptrs` (obfs4 0.1.0-alpha.1) eksplisit masih "under construction". Untuk M3: gunakan `obfs4proxy` (implementasi Go resmi Tor Project) via subprocess. Evaluasi ulang Rust crate saat M3 dimulai. |
| Nama final aplikasi | ✅ **Resolved** | "ALTER" dipilih (stealth-by-name: tidak menyebut fungsi komunikasi secara eksplisit). Riset nama (Juni 2026) tidak menemukan collision serius di GitHub, crates.io, atau domain security tools. Nama proses runtime tetap terpisah & configurable, lihat SEC-09. |

### Open — masih perlu keputusan atau riset lanjutan

| ID | Risiko/Pertanyaan | Catatan | Prioritas |
|---|---|---|---|
| R-01 | **arti API breaking changes** — arti-client masih pre-1.x stable, breaking changes terjadi reguler (contoh: upgrade dalek crates di v1.1.11 menyebabkan breaking change di public API). | Mitigasi: pin versi konkret, tetapkan jadwal upgrade sebelum M2. Perlu keputusan: seberapa sering kita siap upgrade arti dependency? | Tinggi |
| R-02 | **obfs4proxy binary bundling** — jika M3 pakai Go binary, cara distribute binary dan implikasi keamanan supply chain perlu diputuskan. | Opsi: (A) bundle di binary (besar, simple), (B) runtime detection of system-installed obfs4proxy (kecil, tapi dependency external), (C) skip obfs4 di v1 (kurangi kompleksitas, tapi melemahkan SEC-06). Ini **keputusan arsitektur**, perlu approval. | Tinggi |
| R-03 | **Target binary < 20 MB** — kemungkinan tidak tercapai jika obfs4proxy di-bundle. | Terkait R-02. Perlu approval untuk revisi target atau strategi obfs4. | Sedang |
| R-04 | **Implikasi institusional** | Pertimbangkan apakah ada kebijakan organisasi terkait pengembangan tooling anti-forensik/anti-surveillance secara independen — pertanyaan administratif, bukan teknis. | Sedang |
| R-05 | **Online presence leak** — onion address yang diketahui pihak lain bisa di-probe untuk deteksi online/offline status. SEC-10 (on-demand onion) ada tapi tidak melindungi selama sesi aktif. | Pattern mitigasi: Gosling architecture (per-peer endpoint services, dipakai Ricochet-Refresh). Cost tinggi untuk v1. Keputusan: cukup dengan disclosure di Honesty Section (Bagian 10), atau jadikan v2 priority? | Rendah (v1), tapi perlu acknowledgment eksplisit |
| R-06 | **snow — tidak ada formal audit** | `snow` 0.10.0 belum pernah diaudit secara formal. Dipilih karena mengimplementasikan Noise Protocol (dipakai WireGuard), tapi implementasinya sendiri belum diaudit. Untuk penggunaan produksi di threat model state-level, ini perlu dicatat eksplisit sebagai acknowledged risk. | Sedang |

---

## 14. Glosarium (untuk pembaca non-teknis)

- **End-to-end encryption**: hanya pengirim dan penerima yang bisa baca isi pesan, tidak ada pihak di tengah (termasuk pembuat app) yang bisa membacanya.
- **Serverless**: tidak ada komputer pusat yang menyimpan/merelay pesan; dua perangkat bicara langsung.
- **Tor / onion service**: jaringan anonimitas terdistribusi yang menyembunyikan alamat IP asli pengguna.
- **Handshake**: proses awal dua aplikasi "berkenalan" dan menyepakati kunci enkripsi sebelum chat mulai.
- **Forensik digital**: proses memeriksa perangkat (disk, memory) untuk menemukan jejak aktivitas/data.
- **Metadata**: data tentang data — misalnya bukan ISI pesan, tapi FAKTA bahwa pesan pernah dikirim, kapan, ke siapa.
- **[baru] Pluggable transport / obfs4**: lapisan tambahan yang menyamarkan traffic Tor agar tidak terdeteksi sebagai Tor oleh Deep Packet Inspection (DPI).
- **[baru] Noise Protocol**: framework kriptografi untuk membangun channel aman, fondasi teknis di balik VPN WireGuard.

---

## 15. Changelog v0.2

### Ringkasan: Apa yang Berubah dan Kenapa

1. **Tabel 8.3 (Primitif Kriptografi) diperbarui lengkap** — setiap baris kini memiliki versi konkret dan link sumber. Ini menghilangkan ambiguitas "versi berapa yang kita pakai" saat development dimulai.

2. **obfs4 dikonfirmasi sebagai risiko tinggi** — satu-satunya Rust crate (`0.1.0-alpha.1`) eksplisit "under construction". Untuk M3 harus pakai `obfs4proxy` Go binary. Ini berdampak ke target binary size dan arsitektur distribution. Ditandai sebagai open question R-02 dan R-03 yang perlu approval.

3. **RUSTSEC-2024-0344 ditambahkan** — timing variability di `curve25519-dalek` yang dipakai `ed25519-dalek`. Sudah ada fix di versi terbaru, tapi perlu pin versi eksplisit agar tidak regresi.

4. **Parameter Argon2id dikuantifikasi** — OWASP 2024 merekomendasikan m=19 MiB, t=2, p=1. Default crate sudah sesuai, tapi PRD kini menyebut angka konkret agar tidak ada interpretasi berbeda saat implementasi.

5. **Catatan audit `snow` ditambahkan** — snow belum pernah formal audit. Dipilih karena mengimplementasikan Noise Protocol (bukan custom crypto), tapi acknowledged sebagai R-06.

6. **Risiko online presence leak ditambahkan (R-05)** — dari riset precedent Ricochet-Refresh (Gosling architecture). Jika onion address diketahui pihak ketiga, mereka bisa deteksi online/offline status dengan probe. SEC-10 ada tapi tidak cukup selama sesi aktif. Pattern mitigasi (per-peer endpoint) ditempatkan ke Tier 3 skip dengan alasan "v2 consideration."

7. **Honesty Section (Bagian 10) diperbarui** — tambah item eksplisit tentang presence detection, supaya user dan stakeholder tahu limitasi ini sejak awal.

8. **Bagian 13 distrukturisasi ulang** — pisahkan antara "Resolved" (jawaban ketemu) vs "Open" (masih perlu keputusan). Dua open question dari v0.1 (arti maturity, obfs4 library) dipindah ke Resolved dengan jawaban konkret. Empat risiko baru ditambahkan.

9. **Metrik binary size diberi flag** — target < 20 MB ditandai "PERLU APPROVAL" dengan dua opsi konkret karena terdampak obfs4proxy decision.

10. **Roadmap M2/M3 diperbarui** — tambah catatan teknis per milestone berdasarkan temuan riset (pin arti version, keputusan obfs4 architecture).

### Hal yang TIDAK berubah (locked)
- Scope produk (walkie-talkie model, 1-on-1, teks saja)
- Threat model (state-level adversary)
- Keputusan arsitektur inti (dual transport LAN+Tor, Noise_IK, RAM-only history)
- Nama proyek "ALTER"
- Struktur 3-layer audiens dokumen

---

*Akhir dokumen v0.2. Silakan beri komentar per section — bagian mana yang perlu didetailkan lagi atau dipotong scope-nya.*
