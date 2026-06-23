# PRD: ALTER — Serverless Encrypted P2P Terminal Chat

> **Status dokumen:** Draft v0.3 — revisi model konseptual berbasis feedback stakeholder
> **Tanggal v0.1:** 22 Juni 2026
> **Tanggal v0.2:** 22 Juni 2026
> **Tanggal v0.3:** 22 Juni 2026
> **Nama proyek:** **ALTER** — final, locked. Nama proses runtime (binary yang berjalan di device) TIDAK memakai nama ini secara default — lihat SEC-09.
> **Audiens dokumen:** Ditulis supaya bisa dipahami 3 sudut pandang berbeda — visi produk (apa & kenapa), value proposition (untuk siapa & berapa cost), dan spesifikasi teknis (bagaimana & seberapa kuat).
> **Changelog v0.3:** Lihat Bagian 15 di akhir dokumen.

---

## 1. Ringkasan Eksekutif

ALTER adalah aplikasi chat berbasis terminal (CLI/TUI) yang memungkinkan dua orang berkomunikasi end-to-end terenkripsi **tanpa server perantara** yang menyimpan atau merelay pesan. Identitas pengguna berbasis kriptografi (bukan nomor telepon/email), koneksi terjadi langsung antar dua perangkat — baik dalam jaringan lokal (LAN) maupun lintas internet via jaringan Tor sebagai infrastruktur rendezvous publik.

**Yang membedakan dari WhatsApp/Telegram/Signal:** tidak ada perusahaan, tidak ada server, tidak ada akun yang bisa dibekukan atau disubpoena, tidak ada metadata percakapan yang tersimpan di pihak ketiga manapun — karena pihak ketiga itu tidak ada.

**Model interaksi: Room-Bound Sync.** Kedua pihak harus hadir di "room" yang sama secara bersamaan. Room adalah sesi kriptografis ephemeral bersama — aktif selama keduanya terhubung, hancur begitu salah satu keluar. Ini bukan model asinkron seperti SMS. Ini adalah ruang komunikasi yang hidup hanya saat keduanya hadir, seperti dua orang berbicara di ruangan kosong: begitu salah satu keluar, suara itu tidak ada lagi — bahkan tidak bisa diputar ulang.

**Trade-off yang disengaja:** tidak ada riwayat pesan lintas sesi, tidak ada notifikasi offline, tidak ada "pending message" — semua ini konsekuensi langsung dari menghilangkan server dan memilih ephemeral-by-design. Model ini bukan limitation — ini adalah **identitas** ALTER.

---

## 2. Problem Statement

Aplikasi chat mainstream (WhatsApp, Telegram, bahkan Signal) tetap punya **satu titik kegagalan struktural**: server pihak ketiga. Walau enkripsi end-to-end melindungi *isi* pesan, server tersebut tetap:

- Tahu **siapa berkomunikasi dengan siapa** (metadata sosial graph)
- Tahu **kapan dan seberapa sering** (traffic pattern)
- Bisa **dipaksa oleh otoritas hukum/negara** untuk membuka data yang mereka punya
- Jadi **target tunggal** untuk peretasan/kebocoran skala besar

Bahkan aplikasi "zero-knowledge" sekalipun masih menyimpan ciphertext — yang artinya ada *sesuatu* yang bisa disita dan disimpan sebagai bukti keberadaan percakapan. ALTER menghilangkan artifact ini sepenuhnya: tidak ada yang tersimpan di server karena server tidak ada; tidak ada yang bisa dibaca ulang karena kunci sesi dibuang saat percakapan selesai.

**Hipotesis produk:** ada kebutuhan nyata untuk alat komunikasi yang menghilangkan server sepenuhnya dari ekuasi *dan* tidak meninggalkan artifact apapun — menerima trade-off kenyamanan demi menghilangkan titik kegagalan secara struktural, bukan ditambal dengan kebijakan privasi.

---

## 3. Tujuan Produk

### Goals
1. Komunikasi dua pihak yang **terenkripsi end-to-end** dengan primitif kriptografi modern teraudit (bukan buatan sendiri)
2. **Tidak ada server** yang menyimpan, merelay, atau punya visibilitas ke pesan maupun metadata percakapan
3. **Room-Bound Sync** — sesi kriptografis ephemeral: kunci sesi di-discard saat salah satu pihak keluar, pesan lama tidak bisa didekripsi bahkan oleh pengirimnya sendiri
4. Berjalan **dual-mode**: LAN (cepat, lokal) dan internet (via Tor, lintas jaringan)
5. Antarmuka **CLI/TUI** yang efisien dan nyaman dipakai (referensi UX: Claude Code) — bukan REPL mentah
6. **Secure by design** di setiap layer yang cost-nya proporsional dengan reduksi risiko (lihat Bagian 7)
7. Single binary, **cross-platform** (Linux, macOS, Windows minimal via WSL)

### Non-Goals (sengaja TIDAK dikerjakan di v1)
- ❌ Pesan asinkron / offline messaging (konsekuensi dari "Goal #2 & #3" — tidak bisa keduanya)
- ❌ Riwayat pesan lintas sesi (konsekuensi disengaja dari Goal #3 — ephemeral-by-design)
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
- **[v0.2]** Online/offline presence detection by unauthenticated parties — lihat risiko R-05 di Bagian 13

---

## 6. Prinsip Desain ("Secure by Design, Tricky, Efisien")

1. **Setiap fitur keamanan harus punya rasio reduksi-risiko : biaya-implementasi yang jelas** — bukan ditambahkan karena "kedengaran aman". Lihat tabel Tier di Bagian 7.
2. **Minim metadata by default.** Tidak ada version string di handshake, tidak ada nama proses yang identifiable, tidak ada filename yang menjelaskan diri sendiri.
3. **RAM-only untuk data sensitif.** Pesan tidak pernah ditulis ke disk dalam bentuk apapun, termasuk swap (dimitigasi via `mlock`).
4. **Fail closed, bukan fail open.** Kalau handshake gagal verifikasi, koneksi putus total — tidak ada fallback ke mode "kurang aman" secara diam-diam.
5. **Auditable di atas clever.** Pilih primitif kriptografi yang sudah diaudit luas (Noise Protocol, dipakai WireGuard) daripada desain custom yang "lebih pintar" tapi belum pernah diuji publik.
6. **[v0.3] Ephemeral-by-design, bukan ephemeral-by-policy.** Kunci sesi tidak disimpan *karena tidak bisa* disimpan — session key di-ZeroizeOnDrop saat transport state di-drop, tanpa perlu user action, tanpa perlu kebijakan retention. Zero-trace bukan janji — itu konsekuensi teknis yang tidak bisa dihindari.

---

## 7. Spesifikasi Keamanan per Tier

Fitur dikelompokkan berdasarkan rasio value/cost, supaya effort development terarah ke hal paling berdampak dulu.

### Tier 0 — Wajib, fondasi (v1 tidak bisa ship tanpa ini)
| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-01 | Noise_IK handshake (mutual auth + session key) | Fondasi seluruh keamanan transport |
| SEC-02 | Passphrase + Argon2id untuk enkripsi private key at rest | Mencegah private key terbaca langsung dari disk |
| SEC-03 | Chat history hanya di RAM **dalam sesi aktif saja**, tidak pernah ditulis ke disk | Menutup vektor forensik disk; lihat SEC-03a untuk detail discard policy |
| SEC-03a | **Session key di-ZeroizeOnDrop saat room ditutup** — pesan dalam sesi yang sudah berakhir tidak bisa didekripsi bahkan oleh user sendiri | Memperkuat ephemeral model: zero-trace bukan policy, tapi technical constraint |
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
| SEC-09 | Nama proses binary generik & **configurable per-install** (BUKAN hardcoded "alter") | Mengurangi sinyal di `ps aux`/Task Manager. Default config pakai nama plausible seperti `update-agent`, `sync-helper`. User bisa override (`--process-name=xxx`) supaya satu nama gak jadi fingerprint universal untuk semua instalasi ALTER |
| SEC-10 | Onion service on-demand, mati saat app ditutup | Menghindari onion address jadi sinyal presence 24/7 |
| SEC-11 | Hotkey "panic wipe" — wipe RAM + matikan onion service instan | Mitigasi skenario perangkat akan disita saat aplikasi aktif |
| SEC-12 | **[v0.3] Presence indicator dalam TUI** — indikator visual eksplisit saat peer hadir vs. keluar dari room | Security UX: user harus selalu tahu apakah room masih "hidup". Mencegah user mengira pesan dikirim ke peer padahal room sudah kosong. |

### Tier 3 — Eksplisit DI-SKIP untuk v1 (dengan alasan)
| Fitur | Kenapa skip |
|---|---|
| Secure-delete tingkat disk di app-level | Tidak efektif di SSD modern (wear-leveling); domain yang benar adalah full-disk encryption OS-level |
| Decoy/duress vault (password kedua) | Risiko bug tinggi, bisa menyebabkan kehilangan data tidak sengaja; dipertimbangkan ulang setelah v1 stabil |
| Cold-boot RAM protection | Membutuhkan mitigasi hardware (TPM), di luar kendali software aplikasi |
| Per-peer endpoint onion service (Gosling pattern) | Mencegah presence detection oleh unauthenticated third party — pattern Ricochet. Cost implementasi tinggi untuk v1; dipertimbangkan di v2. |

---

## 8. Arsitektur Teknis

### 8.1 Diagram Alur

```
┌─────────────────────────────────────────────────────────┐
│                    TUI Layer (ratatui)                   │
│  [Contact List] [Chat Pane] [Input] [Room Status Bar]   │
│                                  ↑                      │
│                      "● Peer hadir" / "○ Room kosong"  │
└───────────────────┬─────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────┐
│              Identity & Contact Store                    │
│  - Ed25519 keypair (lokal, encrypted at rest)            │
│  - Contact = {nickname, pubkey, onion_addr}              │
└───────────────────┬─────────────────────────────────────┘
                    │
        ┌───────────┴────────────┐
        ▼                        ▼
┌───────────────┐         ┌──────────────────┐
│  LAN PATH      │         │  INTERNET PATH    │
│  mDNS discover │         │  Tor onion service│
│  Direct TCP    │         │  + obfs4 transport│
└───────┬────────┘         └─────────┬─────────┘
        │   (coba dulu, timeout)     │ (fallback)
        └────────────┬───────────────┘
                     ▼
        ┌─────────────────────────────┐
        │      Noise_IK Handshake      │
        └────────────┬────────────────┘
                     ▼
        ┌─────────────────────────────────────┐
        │         ROOM (Encrypted Session)     │
        │  - Session keys: hanya dalam RAM     │
        │  - Pesan: hanya RAM, tidak pernah    │
        │    ditulis ke disk                   │
        │  - Lifetime: selama kedua pihak      │
        │    terhubung                         │
        │  - On exit: ZeroizeOnDrop →          │
        │    kunci dibuang, tak bisa dipulihkan│
        └─────────────────────────────────────┘
```

### 8.2 Modul (struktur kode)

```
src/
├── identity/      → Keypair generation, vault encryption (Argon2id + ChaCha20Poly1305)
├── contacts/       → Contact store, invite code encode/decode
├── transport/       → LAN (mDNS+TCP), Tor (onion + obfs4), dialer (LAN-first-fallback-Tor)
├── crypto/          → Noise_IK handshake, AEAD session, padding (stateless, paling mudah diaudit)
├── session/         → State machine: Connecting → Handshaking → Active → Closed
│                       On Closed: ZeroizeOnDrop session keys, clear message buffer
└── tui/              → Contact list, chat pane, input, unlock screen, panic hotkey,
                        room presence indicator (SEC-12)
```

### 8.3 Session Lifecycle (Room Model)

```
User pilih kontak
      │
      ▼
[Connecting] ── TCP connect + Noise_IK handshake ──▶ [Handshaking]
                                                            │
                              keduanya selesai handshake ──▶ [Active/Room Open]
                                                            │
                     peer disconnect (TCP FIN/RST) ─────▶ [Closed]
                     atau user keluar                           │
                                                         ZeroizeOnDrop
                                                         session keys
                                                         clear msg buffer
                                                               │
                                                         Room kosong.
                                                         Tidak ada yang
                                                         bisa dipulihkan.
```

**Keputusan desain yang dikunci (v0.3):**
- **Scroll dalam sesi aktif**: diperbolehkan — pesan dalam sesi aktif ada di RAM, masih bisa di-scroll. Kunci sesi belum di-discard. ✅
- **Scroll lintas sesi**: tidak mungkin secara teknis — kunci sesi sesi lama sudah dibuang, ciphertext (seandainya ada) tidak bisa didekripsi. Tidak ada "load history" dari sesi sebelumnya. ✅
- **Reconnect setelah disconnect**: selalu mulai sesi baru (Noise_IK handshake baru, ephemeral key baru). Sesi sebelumnya dianggap sudah berakhir permanent. ✅
- **Disconnect timeout**: jika TCP connection terputus, room langsung dinyatakan Closed — tidak ada grace period atau auto-reconnect ke sesi yang sama. ✅

### 8.4 Primitif Kriptografi

> **Catatan v0.2:** Tabel ini diperbarui dengan versi konkret hasil riset (Juni 2026). Setiap baris mencantumkan sumber verifikasi.

| Fungsi | Primitif | Library (Rust) | Versi | Catatan |
|---|---|---|---|---|
| Identitas | Ed25519 | `ed25519-dalek` | **2.2.0** | RUSTSEC-2024-0344 mempengaruhi dependensi `curve25519-dalek`; pastikan menggunakan `curve25519-dalek >= 4.1.3` (sudah included di dalek 2.x terbaru). |
| Handshake | Noise_IK | `snow` | **0.10.0** | Mendukung Noise_IK penuh, tracking spec revision 34. **Peringatan: belum ada formal security audit.** Dipilih karena protokolnya identik dengan yang dipakai WireGuard. |
| Enkripsi pesan | AEAD (ChaCha20-Poly1305) | `chacha20poly1305` | **0.10.1** | ✅ Diaudit NCC Group (2019). Tidak ada CVE aktif. RustCrypto project. |
| Key derivation dari passphrase | Argon2id | `argon2` | **0.5.3** | **OWASP 2024: m=19 MiB, t=2 iterasi, p=1 parallelism.** Default crate sesuai rekomendasi. |
| Wipe memory | Explicit zeroing | `zeroize` | **1.x** | Stable. Semua secret struct wajib implement `ZeroizeOnDrop`. |
| LAN discovery | mDNS | `mdns-sd` | **0.20.0** | Aktif dimainte. Supports macOS/Linux/Windows, IPv4+IPv6. |
| Anonymity network | Tor (onion service) | `arti-client` | **0.43.x** (arti 1.4.0) | ✅ Onion service hosting via `TorClient::launch_onion_service()`. API pre-1.x stable — pin versi eksplisit. Feature flag: `onion-service-service`. |
| Traffic obfuscation | obfs4 | **⚠️ TIDAK ADA Rust matang** | alpha | `jmwample/ptrs` masih `0.1.0-alpha.1`. **Untuk M3: gunakan `obfs4proxy` (Go binary) via subprocess.** |

---

## 9. Alur Pengguna (UX)

1. **Buka app** → prompt passphrase (unlock vault lokal)
2. **Tampilan utama**: panel kontak kiri, panel chat kanan — estetika terminal mirip Claude Code (clean, monospace, minim chrome visual)
3. **Tambah kontak baru**: tukar "kode kontak" (1 string berisi pubkey + onion address) lewat channel lain (manual, di luar app)
4. **Pilih kontak** → app coba LAN dulu (mDNS, ~3 detik timeout) → fallback ke Tor kalau tidak ketemu di LAN
5. **Status koneksi eksplisit ditampilkan**:
   - `"Mencari di LAN..."` → `"Membangun sirkuit Tor..."` → `"Handshake..."` → `"● Room terbuka"`
   - Status bar selalu visible; user tidak perlu menebak apakah peer masih hadir
6. **Room aktif**: pesan terenkripsi real-time; scroll up untuk baca pesan dalam sesi ini (RAM); room status bar menunjukkan `"● [nama-peer] hadir"` selama koneksi aktif
7. **Peer keluar / disconnect**: status bar berubah ke `"○ Room kosong — [nama-peer] telah keluar"`, input diblokir, prompt untuk menutup atau reconnect (sesi baru)
8. **User keluar / tutup app**: room ditutup, session keys di-ZeroizeOnDrop, seluruh state di-wipe dari memory. Tidak ada jejak sesi tersisa.

---

## 10. Batasan & Yang Tidak Dijamin (Honesty Section)

Bagian ini wajib dibaca semua stakeholder sebelum approve — supaya ekspektasi realistis:

- **Tidak ada pesan offline.** Kedua pihak harus hadir di room yang sama secara bersamaan. Ini bukan bug — ini identitas ALTER.
- **Tidak ada riwayat lintas sesi.** Begitu salah satu pihak keluar dari room, kunci sesi dibuang. Pesan dari sesi sebelumnya tidak bisa dibaca kembali oleh siapapun, termasuk pengirimnya sendiri. Ini konsekuensi teknis yang disengaja, bukan kebijakan.
- **Reconnect = sesi baru.** Jika koneksi terputus di tengah percakapan, koneksi ulang akan membuat room baru dengan kunci baru. Pesan dari sesi sebelumnya tidak bisa dipulihkan.
- **"Anti-forensik" bukan jaminan absolut.** Kita menutup vektor forensik disk & memory standar, tapi TIDAK bisa menahan akses fisik saat perangkat menyala (live forensics) atau serangan hardware-level.
- **Penggunaan Tor bisa terlihat** di jaringan yang dimonitor ketat, walau isi pesan tidak bisa dibaca. obfs4 mengurangi ini, tidak menghilangkan sepenuhnya.
- **Operational security ada di luar kendali aplikasi** — shell history, kebiasaan pengguna, keamanan fisik perangkat, semua itu domain pengguna, bukan sesuatu yang software bisa paksa.
- **[v0.2] Online/offline status.** Siapapun yang mengetahui onion address kamu secara teoritis bisa mendeteksi kapan kamu online dengan mencoba koneksi ke onion tersebut. SEC-10 (on-demand onion) mengurangi exposure window, tapi tidak menghilangkan risiko selama sesi aktif. Pattern mitigasi (Gosling) dipertimbangkan di v2.

---

## 11. Metrik Keberhasilan (v1)

| Metrik | Target | Catatan |
|---|---|---|
| Waktu koneksi LAN | < 3 detik | — |
| Waktu koneksi Tor (cold start) | < 60 detik | Perlu divalidasi dengan arti-client di POC; onion service bootstrap bisa bervariasi |
| Ukuran binary | < 20 MB (single binary) | **⚠️ PERLU APPROVAL:** Jika obfs4 diimplementasikan via bundling `obfs4proxy` Go binary, target ini kemungkinan tidak tercapai. Opsi: (A) naikkan target ke < 50 MB, (B) jadikan obfs4proxy sebagai opsional external dependency yang dideteksi runtime. |
| Crypto code coverage (unit test) | 100% pada modul `crypto/` | — |
| Tidak ada plaintext di disk | Diverifikasi via audit manual + automated test (cek filesystem setelah sesi chat) | — |
| Session key wipe setelah close | Diverifikasi via test — session keys tidak accessible setelah `EncryptedSession` di-drop | SEC-03a |

---

## 12. Roadmap

| Fase | Scope | Catatan |
|---|---|---|
| **M0 — Fondasi** | `identity/` + `crypto/handshake.rs`, unit test Noise_IK roundtrip lokal | ✅ **Selesai** — 12/12 tests pass |
| **M1 — LAN MVP** | Transport LAN selesai, TUI skeleton dengan presence indicator (SEC-12), chat 1-on-1 jalan di satu jaringan | TUI harus include room status bar sejak awal (bukan afterthought) |
| **M2 — Internet path** | Integrasi Tor onion service via `arti-client`, fallback logic | Pin `arti-client` ke versi konkret. Feature flags `onion-service-service` wajib aktif. |
| **M3 — Hardening** | Tier 1 & 2 (obfs4, padding, panic wipe, generic naming) | obfs4 via `obfs4proxy` binary. Evaluasi `jmwample/ptrs` saat M3 dimulai untuk cek kematangan. |
| **M4 — Polish & Audit** | Review keamanan internal, dokumentasi, cross-platform build | Review RUSTSEC advisory untuk semua dependency sebelum release. |

---

## 13. Risiko & Open Questions

### Resolved (ditemukan jawaban dari riset atau keputusan di v0.3)

| Item | Status | Jawaban |
|---|---|---|
| Maturity `arti` untuk hosting onion service | ✅ **Resolved** | Supported via `TorClient::launch_onion_service()` di `arti-client >= 0.43.x`. Feature flag `onion-service-service` wajib. API pre-1.x — pin versi, alokasikan maintenance overhead reguler. |
| Library obfs4 di Rust | ✅ **Resolved** | Tidak ada crate matang. `jmwample/ptrs` (obfs4 0.1.0-alpha.1) eksplisit masih "under construction". Untuk M3: gunakan `obfs4proxy` (Go resmi Tor Project) via subprocess. |
| Nama final aplikasi | ✅ **Resolved** | "ALTER" dipilih. Tidak ada collision serius. Nama proses runtime configurable, lihat SEC-09. |
| Scroll dalam sesi aktif | ✅ **Resolved v0.3** | Diperbolehkan — pesan ada di RAM selama session keys masih aktif. Scroll up/down dalam sesi yang sedang berlangsung adalah fitur normal. |
| Scroll lintas sesi / load history | ✅ **Resolved v0.3** | Tidak mungkin secara teknis — bukan kebijakan. Session keys di-ZeroizeOnDrop saat room tutup; tidak ada data yang bisa di-recover. |
| Behavior reconnect setelah disconnect | ✅ **Resolved v0.3** | Selalu sesi baru (handshake baru, ephemeral key baru). Tidak ada grace period atau auto-reconnect ke sesi lama. Room lama dinyatakan Closed permanent. |

### Open — masih perlu keputusan atau riset lanjutan

| ID | Risiko/Pertanyaan | Catatan | Prioritas |
|---|---|---|---|
| R-01 | **arti API breaking changes** — arti-client masih pre-1.x stable, breaking changes terjadi reguler. | Mitigasi: pin versi konkret, tetapkan jadwal upgrade sebelum M2. | Tinggi |
| R-02 | **obfs4proxy binary bundling** — cara distribute binary dan implikasi keamanan supply chain. | Opsi: (A) bundle (besar, simple), (B) runtime detection system-installed (kecil, external dep), (C) skip obfs4 di v1. **Keputusan arsitektur, perlu approval.** | Tinggi |
| R-03 | **Target binary < 20 MB** — kemungkinan tidak tercapai jika obfs4proxy di-bundle. | Terkait R-02. Perlu approval revisi target atau strategi. | Sedang |
| R-04 | **Implikasi institusional** | Pertimbangkan apakah ada kebijakan organisasi terkait pengembangan tooling anti-forensik/anti-surveillance secara independen. Pertanyaan administratif, bukan teknis. | Sedang |
| R-05 | **Online presence leak** — onion address yang diketahui pihak lain bisa di-probe untuk deteksi online/offline. SEC-10 ada tapi tidak melindungi selama sesi aktif. | Pattern mitigasi: Gosling architecture (dipakai Ricochet-Refresh). Cost tinggi untuk v1. Cukup disclosure di Bagian 10, atau jadikan v2 priority? | Rendah (v1) |
| R-06 | **snow — belum ada formal audit** | `snow` 0.10.0 mengimplementasikan Noise Protocol tapi implementasinya sendiri belum diaudit. Acknowledged risk untuk deployment di threat model state-level. | Sedang |
| R-07 | **[v0.3] UX expectation mismatch** — "tidak bisa baca pesan lama" adalah konsep yang tidak familiar bagi kebanyakan pengguna aplikasi chat modern. | Mitigasi: onboarding screen singkat saat pertama kali buka app yang menjelaskan room model secara eksplisit. Apakah ini perlu masuk M1 atau bisa ditunda ke M4? | Sedang |

---

## 14. Glosarium (untuk pembaca non-teknis)

- **End-to-end encryption**: hanya pengirim dan penerima yang bisa baca isi pesan, tidak ada pihak di tengah (termasuk pembuat app) yang bisa membacanya.
- **Serverless**: tidak ada komputer pusat yang menyimpan/merelay pesan; dua perangkat bicara langsung.
- **Tor / onion service**: jaringan anonimitas terdistribusi yang menyembunyikan alamat IP asli pengguna.
- **Handshake**: proses awal dua aplikasi "berkenalan" dan menyepakati kunci enkripsi sebelum chat mulai.
- **Forensik digital**: proses memeriksa perangkat (disk, memory) untuk menemukan jejak aktivitas/data.
- **Metadata**: data tentang data — misalnya bukan ISI pesan, tapi FAKTA bahwa pesan pernah dikirim, kapan, ke siapa.
- **Pluggable transport / obfs4**: lapisan tambahan yang menyamarkan traffic Tor agar tidak terdeteksi sebagai Tor oleh Deep Packet Inspection (DPI).
- **Noise Protocol**: framework kriptografi untuk membangun channel aman, fondasi teknis di balik VPN WireGuard.
- **[v0.3] Room**: sesi kriptografis ephemeral yang dibuat saat dua pihak terhubung via Noise_IK handshake. Room "hidup" selama koneksi aktif; "mati" (dan semua kunci dibuang) begitu salah satu pihak keluar.
- **[v0.3] Ephemeral-by-design**: properti sistem di mana ketidakhadiran riwayat adalah konsekuensi teknis yang tidak bisa dihindari, bukan kebijakan retention yang bisa diubah.
- **[v0.3] ZeroizeOnDrop**: mekanisme Rust yang memastikan data sensitif dihapus dari memori secara otomatis saat objek keluar dari scope — tidak butuh action eksplisit dari programmer.

---

## 15. Changelog

### v0.3 (22 Juni 2026) — Room-Bound Sync Model

**Konteks:** Feedback stakeholder mengusulkan reframing model interaksi dari "walkie-talkie" (dua perangkat online bersamaan) ke "room-bound sync" (dua perangkat hadir di room yang sama). Kesimpulan tim: ini bukan perubahan arsitektur — ini artikulasi yang lebih akurat dan lebih kuat dari properti yang sudah ada di desain. Adopsi penuh.

**Perubahan:**

1. **Section 1 (Executive Summary)** — "walkie-talkie model" diganti dengan "Room-Bound Sync" beserta deskripsi konseptual ruang komunikasi ephemeral.

2. **Section 2 (Problem Statement)** — diperluas untuk menjelaskan bahwa ALTER bukan hanya zero-knowledge dari server tapi juga tidak menyisakan artifact ciphertext sama sekali.

3. **Section 3 (Goals)** — Goal #3 baru: "Room-Bound Sync — sesi kriptografis ephemeral". "Tidak ada riwayat pesan lintas sesi" ditambahkan ke Non-Goals sebagai non-goal yang disengaja (bukan keterbatasan).

4. **Section 6 (Prinsip Desain)** — Prinsip #6 baru: "Ephemeral-by-design, bukan ephemeral-by-policy."

5. **Section 7 (Security Tiers)** — SEC-03a baru: session key di-ZeroizeOnDrop saat room ditutup. SEC-12 baru: presence indicator dalam TUI (Tier 2).

6. **Section 8 (Arsitektur)** — Subsection 8.1 diagram diperbarui dengan room status bar dan ZeroizeOnDrop annotation. Subsection 8.3 baru: Session Lifecycle diagram + keputusan desain yang dikunci (scroll dalam sesi, scroll lintas sesi, reconnect behavior, disconnect timeout) — semua Resolved di v0.3.

7. **Section 9 (UX Flow)** — Step 5 diperluas (status indicator sequence). Step 6 ditambah room status bar dan scroll behavior. Step 7 baru (peer disconnect UX). Step 8 (exit) diperjelas dengan ZeroizeOnDrop consequence.

8. **Section 10 (Honesty)** — Tambah dua item: "Tidak ada riwayat lintas sesi" (bukan kebijakan, tapi technical constraint) dan "Reconnect = sesi baru".

9. **Section 11 (Metrik)** — Tambah metrik: session key wipe setelah close (verifikasi SEC-03a).

10. **Section 12 (Roadmap)** — M0 ditandai ✅ Selesai (12/12 tests pass). M1 diperjelas: TUI harus include room status bar sejak awal.

11. **Section 13 (Risiko)** — Tiga item Resolved baru (scroll, cross-session, reconnect). R-07 baru: UX expectation mismatch — pengguna tidak familiar dengan model ephemeral.

12. **Section 14 (Glosarium)** — Tiga term baru: "Room", "Ephemeral-by-design", "ZeroizeOnDrop".

**Hal yang TIDAK berubah (locked):**
- Arsitektur teknis: Noise_IK, dual transport LAN+Tor, arti-client, vault format
- Threat model (state-level adversary)
- Nama proyek "ALTER"
- Semua keputusan v0.2 yang sudah locked

### v0.2 (22 Juni 2026) — Validasi Teknis
Lihat versi lengkap di `PRD-alter-v0.2.md`. Ringkasan: versi library konkret, resolusi obfs4, RUSTSEC-2024-0344, Argon2id params, risiko R-01 s/d R-06, restructuring Bagian 13.

### v0.1 (22 Juni 2026) — Draft awal
Lihat versi lengkap di `PRD-alter.md`.

---

*Akhir dokumen v0.3. Silakan beri komentar per section — bagian mana yang perlu didetailkan lagi atau dipotong scope-nya.*
