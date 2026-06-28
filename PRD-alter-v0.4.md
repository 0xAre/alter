# PRD: ALTER — Serverless Encrypted P2P Terminal Chat

> **Status dokumen:** Draft v0.4 — planning pasca-v1 (M5–M6)
> **Tanggal v0.4:** 26 Juni 2026
> **Base dokumen:** PRD-alter-v0.3.md (semua keputusan v0.3 tetap berlaku kecuali dinyatakan berubah)
> **Audiens dokumen:** Ditulis supaya bisa dipahami 3 sudut pandang berbeda — visi produk (apa & kenapa), value proposition (untuk siapa & berapa cost), dan spesifikasi teknis (bagaimana & seberapa kuat).
> **Changelog v0.4:** Lihat Bagian 15 di akhir dokumen.

---

## 1. Konteks: Status Pasca-v1

ALTER v0.2.0 (M0–M4 selesai, 26 Juni 2026) mengimplementasikan seluruh scope v1 yang didefinisikan PRD v0.3:

| Milestone | Status |
|---|---|
| M0 — Fondasi (identity, crypto) | ✅ Selesai |
| M1 — LAN MVP (mDNS, TUI) | ✅ Selesai |
| M2 — Internet path (Tor, onion service) | ✅ Selesai |
| M3 — Hardening (padding, panic wipe, obfs4 detection) | ✅ Selesai |
| M4 — Polish & Audit (passphrase tersembunyi, onboarding, cargo audit) | ✅ Selesai |

PRD v0.4 ini mendefinisikan **v2 roadmap** (M5–M6). Scope dipilih dari open questions PRD v0.3 yang ditunda ke v2, hasil riset teknis Juni 2026, dan analisis gap keamanan yang tersisa.

---

## 2. Open Questions dari v0.3 — Status per Juni 2026

Sebelum scope v0.4 ditetapkan, semua risiko terbuka dari v0.3 dievaluasi:

| ID | Risiko/Pertanyaan | Status di v0.4 |
|---|---|---|
| R-01 | arti API breaking changes | **Resolved** — arti-client 0.43.0 (1 Jun 2026) stabil; breaking change arti 2.0.0 hanya ke binary `arti`, tidak ke crate `arti-client`. Stay di 0.43.0. |
| R-02 | obfs4proxy binary bundling | **Re-opened** — obfs4proxy **unmaintained sejak 2022**. Fork resmi Tor Project adalah **lyrebird** (Go, aktif). ALTER harus detect lyrebird sebagai prioritas. Lihat SEC-14. |
| R-03 | Target binary < 20 MB | **Resolved** — runtime detection (bukan bundle) = binary ALTER tidak berubah ukurannya karena lyrebird/obfs4proxy. Target < 20 MB tetap valid. |
| R-04 | Implikasi institusional | Tidak berubah — pertanyaan administratif, di luar scope teknis PRD. |
| R-05 | Online presence leak | **Dijawab sebagian** — Tor v3 Restricted Discovery siap di arti-client 0.43. Full Gosling (gosling crate 0.5.3) belum production-ready (`arti-client-tor-provider` masih "not for production use"). Lihat SEC-13. |
| R-06 | snow belum diaudit formal | **Tetap terbuka** — v0.10.0 (Juli 2024) masih tanpa audit. MSRV naik ke 1.85. Acknowledged risk. |
| SEC-04 (parsial) | mlock() belum diimplementasikan | **Gap ditemukan** — v0.3 mendefinisikan SEC-04 (`zeroize + mlock`) tapi implementasi M0–M4 hanya selesaikan `zeroize`. `mlock()` via `memsec` crate belum ada. Dijadwalkan di M5. |

---

## 3. Scope Baru v0.4 — Ringkasan

Dua milestone, fokus keamanan:

1. **M5 — Presence Privacy + Contact Management** — Menutup lubang terbesar yang tersisa: siapapun dengan onion address dapat memonitor kapan user online/offline. Solusi: Tor v3 Restricted Discovery (SEC-13) + upgrade deteksi lyrebird (SEC-14) + mlock completion (SEC-04) + contact management UX (rename/delete). Contact management masuk M5 karena user sudah harus re-add semua kontak akibat breaking change invite code — momen alami untuk ship fitur ini.

2. **M6 — Password Manager Decoy Front** — ALTER menyamar sebagai password manager terminal. Passphrase biasa membuka password manager yang fungsional; passphrase rahasia membuka ALTER chat. File vault identik dari luar untuk kedua mode. Milestone tersendiri agar dapat fokus penuh; vault adalah komponen paling kritis di codebase.

**Tidak termasuk v0.4:** File transfer ephemeral (FT-01) dipindah ke PRD selanjutnya — ALTER tetap fokus sebagai aplikasi chat teks dulu; fitur tersebut akan dikerjakan setelah M5–M6 stabil.

---

## 4. Spesifikasi Keamanan Baru (Tier v0.4)

Ini adalah tambahan ke tabel SEC dari PRD v0.3. Semua SEC-01 s/d SEC-12 tetap berlaku.

### Tier 1 — Dampak besar, effort sedang

| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-13 | **Tor v3 Client Authorization (Restricted Discovery)** per kontak — onion service descriptor dienkripsi dengan x25519 pubkey kontak; pihak yang tidak punya key tidak bisa resolve descriptor, tidak bisa probe online/offline | Menutup R-05: presence detection oleh unauthenticated party. Ini subset Gosling yang sudah didukung arti-client 0.43 via `restricted_discovery` flag. |
| SEC-14 | **Upgrade deteksi PT binary** — detect `lyrebird` (prioritas) DAN `obfs4proxy` (fallback legacy) di PATH; badge TUI diperbarui | `obfs4proxy` unmaintained sejak 2022. Tor Project resmi merekomendasikan `lyrebird` sebagai pengganti. ALTER harus detect keduanya. |

### Tier 2 — "Tricky", cost rendah, value tinggi

| ID | Fitur | Justifikasi |
|---|---|---|
| SEC-04 | **Lengkapi `mlock()` via `memsec` crate** — lock halaman RAM berisi secret (vault key, noise private key, passphrase) agar tidak masuk swap disk; tambah `madvise(MADV_DONTDUMP)` di Linux untuk keluarkan dari core dump | v0.3 hanya selesaikan `zeroize`. `mlock` adalah separuh lain dari SEC-04. Tanpa ini, secret bisa tersimpan di swap file forensik. |

### Tier 3 — Disetujui untuk v0.4

| ID | Fitur | Justifikasi | Status |
|---|---|---|---|
| SEC-15 | **Password Manager Decoy Front** — ALTER menyamar sebagai password manager terminal. Passphrase A membuka password manager yang fungsional (simpan/lihat credentials). Passphrase B membuka ALTER chat. File vault identik secara bit dari luar; dua slot Argon2id independen; tidak pernah return error untuk passphrase apapun. | Perlindungan saat dipaksa membuka perangkat. Password manager dipilih karena: (1) verifikasi adversary tidak instan — password tidak bisa langsung di-test di tempat, (2) natural untuk user CLI, (3) fungsional = convincing. | ✅ **Disetujui — M6** |
| UX-01 | **Contact management** — rename, delete kontak dari daftar; validasi sebelum hapus (konfirmasi eksplisit) | User harus re-add semua kontak saat upgrade M5 (breaking change invite code) — momen alami untuk ship fitur ini. Tanpa ini, UX M5 terasa disruptif tanpa kompensasi. | ✅ **Disetujui — M5** |

---

## 5. Arsitektur Teknis — Perubahan dari v0.3

### 5.1 SEC-13: Tor v3 Client Authorization

**Cara kerja Restricted Discovery:**

Setiap user memiliki `identity_keypair` (Ed25519) — sudah ada. Untuk setiap *kontak*, user men-generate pasangan x25519 `(client_auth_privkey, client_auth_pubkey)`. Saat membuat onion service:
- Set `restricted_discovery = true` di `OnionServiceConfigBuilder`
- Tambahkan `client_auth_pubkey` kontak ke authorized clients list

Akibatnya:
- Descriptor onion service dienkripsi dengan pubkey kontak tersebut
- Tor relay tidak bisa membaca descriptor
- Pihak tanpa `client_auth_privkey` tidak bisa resolve descriptor → tidak bisa detect online/offline

**Perubahan kode:**
- `contacts/` — tambahkan field `client_auth_key: Option<x25519_dalek::PublicKey>` ke contact store
- `transport/tor.rs` — saat `launch_onion_service()`, pass authorized keys dari contacts yang sudah ada
- `contacts/invite.rs` — invite code diperpanjang: sertakan `client_auth_pubkey` (base64, ~44 char tambahan) agar peer bisa men-generate restricted service untuk kita

**Implikasi invite code:**
Invite code saat ini: `keys@<onion_address>`. Setelah SEC-13:
```
keys@<onion_address>@<client_auth_pubkey_base64>
```
Format tidak kompatibel ke belakang — **ini breaking change yang disengaja dan disetujui** (26 Juni 2026). Alasan: optional backward-compat menciptakan "privacy second class" di mana kontak lama tetap bisa di-probe meskipun fitur aktif — bertentangan langsung dengan prinsip "fail closed". ALTER masih v0.2.0 (user base kecil), ini momen terbaik untuk breaking change bersih. User harus regenerate invite code dan re-add semua kontak setelah upgrade ke M5.

**Catatan implementasi:**
- `x25519-dalek` sudah ada di Cargo.toml (untuk Noise handshake), tidak perlu dep baru
- `tor-hsservice` (sudah di Cargo.toml) menyediakan `HsClientDescEncKey` untuk client authorization
- API: `OnionServiceConfigBuilder::authorized_client()` — tersedia di arti-client 0.43

**⚠️ Risiko arsitektur: authorized list saat app berjalan (hot-add)**

Onion service di-launch satu kali saat startup dengan daftar kontak yang sudah ada. Masalah: jika user menambah kontak baru saat app sedang berjalan, kontak baru tersebut belum ada di authorized list → tidak bisa connect via restricted onion sampai app di-restart.

Dua opsi mitigasi (pilih satu sebelum implementasi):
1. **Restart onion service** saat kontak baru ditambah — bersih tapi ada downtime ~5–10 detik saat onion service re-register ke Tor network
2. **Tolak koneksi masuk dari kontak tidak dikenal** di application layer — onion service tetap unrestricted di level Tor, tapi tambahkan auth challenge sebelum handshake Noise_IK. Lebih lemah secara privasi (descriptor tetap bisa di-resolve) tapi tidak ada downtime

Opsi 1 lebih konsisten dengan prinsip "fail closed". **Rekomendasi: pilih opsi 1, dokumentasikan "menambah kontak baru membutuhkan beberapa detik reload" di UI.**

**Referensi:** [Arti 1.7.0 release (Restricted Discovery)](https://blog.torproject.org/arti_1_7_0_released/), `tor-hsservice` docs

---

### 5.2 SEC-14: Upgrade Deteksi PT Binary

`obfs4proxy` (Yawning/obfs4) tidak diperbarui sejak 2022. Fork aktif resminya adalah **lyrebird** (Go, Tor Project, commit terakhir Jan 2026).

**Perubahan di `src/transport/obfs4.rs`:**

```
Deteksi sekarang (M3):  cari "obfs4proxy" / "obfs4proxy.exe" di PATH
Deteksi v0.4:           cari "lyrebird" ATAU "obfs4proxy" di PATH (lyrebird prioritas)
```

Badge TUI diperbarui: `" lyrebird "` jika lyrebird ditemukan, `" obfs4 "` jika hanya obfs4proxy.

Tidak ada perubahan arsitektur — hanya logika deteksi. Effort rendah, dampak tinggi untuk user baru yang sudah pakai lyrebird.

**Referensi:** [lyrebird GitLab Tor Project](https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/lyrebird)

---

### 5.3 SEC-04 Completion: mlock()

**Crate yang dipakai:** `memsec` (port Rust dari `libsodium/utils`).

```toml
memsec = "0.6"
```

**Halaman memori yang perlu di-lock:**
1. `SelfKeys.noise_sk` (noise private key, 32 bytes) — sudah `ZeroizeOnDrop`, perlu di-mlock saat load
2. Vault key (32 bytes ChaCha20 key, saat decrypt) — ephemeral, perlu lock selama dipakai
3. Passphrase buffer (`Zeroizing<String>`, dalam `read_passphrase()`) — lock sebelum input, unlock + zeroize setelah KDF selesai

**Pattern implementasi:**
```rust
// Lock saat alokasi secret
unsafe { memsec::mlock(ptr, len) };
// Di Linux: tambah MADV_DONTDUMP (exclude dari core dump)
#[cfg(target_os = "linux")]
unsafe { libc::madvise(ptr as *mut _, len, libc::MADV_DONTDUMP) };

// Drop: zeroize dulu, baru unlock
secret.zeroize();
unsafe { memsec::munlock(ptr, len) };
```

**Peringatan platform (dokumentasikan di kode):**
- **Linux**: `RLIMIT_MEMLOCK` default 64 KB; ALTER hanya lock beberapa ratus byte, aman
- **macOS**: `mlock()` berfungsi; `MADV_DONTDUMP` tidak tersedia (return `Unsupported`, abaikan)
- **Windows**: `VirtualLock()` via `memsec` — butuh elevated privilege. Jika gagal, log warning tapi jangan crash; fallback ke `zeroize` saja (sudah ada)
- **Laptop suspend/hibernate**: `mlock` tidak melindungi dari RAM dump saat hibernate — ini limitasi fundamental, catat di Bagian 10

---

### 5.4 SEC-15: Password Manager Decoy Front

#### Konsep & Threat Model

ALTER menyamar sebagai password manager terminal. Dari sudut pandang adversary yang memeriksa perangkat:

```
$ alter
Passphrase: ••••••••          ← passphrase A (decoy)
                               → TUI password manager biasa terbuka

$ alter
Passphrase: ••••••••••••      ← passphrase B (rahasia, lebih panjang)
                               → ALTER chat terbuka
```

Tidak ada indikasi visual atau teknikal yang membedakan kedua mode. File vault identik secara bit dari luar. ALTER tidak pernah return error untuk passphrase apapun.

**Mengapa password manager (bukan TOTP, journal, atau decoy identity)?**

| Opsi | Verifikasi adversary instan? | Kesimpulan |
|---|---|---|
| TOTP / 2FA manager | **Ya** — kode 6-digit bisa langsung dicek di service | Berbahaya: kode palsu langsung ketahuan |
| Password manager | **Tidak** — perlu buka browser, buka service, test login | Aman: friction verifikasi tinggi |
| Encrypted journal | Tidak | Aman, tapi kurang convincing untuk user teknis |
| Decoy identity ALTER | Tidak | Masih kelihatan seperti app chat — belum cukup menyamarkan |

Password manager terpilih karena kombinasi: friction verifikasi tinggi + natural untuk user CLI + dapat diisi konten real low-stakes tanpa risiko immediate.

---

#### UX Flow Password Manager (Mode A — Decoy)

```
┌─────────────────────────────────────────────────────┐
│  VAULT  —  personal credential store                │
│  ─────────────────────────────────────────────────  │
│  [a] tambah   [d] hapus   [e] edit   [/] cari       │
│  ─────────────────────────────────────────────────  │
│  > github.com          user@example.com    ••••••   │
│    digitalocean.com    deploy@example.com  ••••••   │
│    protonmail.com      user@pm.me          ••••••   │
│  ─────────────────────────────────────────────────  │
│  [Enter] reveal   [q] keluar                        │
└─────────────────────────────────────────────────────┘
```

Fitur minimum yang wajib fungsional agar convincing:
- **Tambah entry**: service name + username + password (di-enkripsi)
- **Lihat/reveal password**: tekan Enter pada entry → tampilkan password sebentar, auto-hide setelah 5 detik
- **Hapus entry**: dengan konfirmasi
- **Cari**: filter list by service name

Semua data password manager disimpan di **Slot A** vault (terenkripsi dengan kunci dari passphrase A).

---

#### Format Vault Baru (v2)

Vault format baru — tidak kompatibel ke belakang (user v0.2.x perlu migrasi saat upgrade ke M6):

```
Vault file layout (v2) — fixed size 4096 bytes:
┌───────────────────────────────────────────────────┐
│  32B  salt_a  (Argon2id salt untuk slot A)        │
│  32B  salt_b  (Argon2id salt untuk slot B)        │
│  12B  nonce_a                                     │
│  N B  ciphertext_a  (password manager entries)   │
│  12B  nonce_b                                     │
│  M B  ciphertext_b  (ALTER keypair)               │
│  ...  CSPRNG padding sampai genap 4096 bytes      │
└───────────────────────────────────────────────────┘
```

Derivasi kunci per slot — independen, tidak saling terkait:
```
key_a = Argon2id(passphrase_a, salt_a, m=64MB, t=3, p=1) → 32B
key_b = Argon2id(passphrase_b, salt_b, m=64MB, t=3, p=1) → 32B

ciphertext_a = XChaCha20-Poly1305.encrypt(key_a, nonce_a, pm_entries_json)
ciphertext_b = XChaCha20-Poly1305.encrypt(key_b, nonce_b, alter_keypair_bytes)
```

Saat buka vault: coba decrypt slot B dulu dengan passphrase yang diberikan. Jika MAC valid → ALTER mode. Jika MAC invalid → coba decrypt slot A. Jika valid → password manager mode. Jika keduanya invalid → buka password manager kosong (passphrase baru). **Tidak pernah return error.**

**Mengapa dua salt terpisah (bukan satu salt shared)?**
Satu salt shared berarti `key_a` dan `key_b` berkorelasi secara matematis (derived dari input yang sama). Salt independen memastikan dua kunci benar-benar tidak terkait — bahkan dengan pengetahuan `key_a`, tidak ada informasi yang bocor tentang `key_b`.

---

#### Invariant Kritis

- **Never-fail**: semua passphrase selalu berhasil "dibuka" — tidak ada error yang bisa membedakan passphrase yang dikenal vs tidak dikenal
- **Indistinguishable size**: file vault selalu 4096 bytes, terlepas dari isi atau mode yang dipakai
- **Independent slots**: `key_a` dan `key_b` berasal dari salt berbeda — satu tidak bisa diturunkan dari yang lain
- **No mode indicator**: tidak ada field di vault yang menyebut "ini decoy" atau "ini mode asli"
- **Fungsional**, bukan sekedar tampilan: password manager harus bisa simpan dan buka entry sungguhan

---

#### Risiko Implementasi (wajib dimitigasi sebelum merge)

1. **Bug logika slot → kehilangan kunci permanen** — tidak ada recovery. Mitigasi: test round-trip exhaustive sebelum merge; backup prompt saat migrasi vault.
2. **Migrasi vault v1 → v2** — user v0.2.x perlu convert vault. ALTER harus detect format lama dan minta migrasi eksplisit (bukan silent).
3. **Password manager harus convincing** — jika adversary lihat password manager kosong tanpa entry apapun, itu mencurigakan. Dokumentasikan kepada user: isi decoy vault dengan beberapa entry real low-stakes setelah setup.
4. **Belum ada formal review pattern ini** di Rust ecosystem. Referensi: `tomb` (Linux), `VeraCrypt` hidden volume. Tidak ada crate yang diaudit untuk pattern ini — implementasi dari scratch.

---

#### Checklist Test Wajib Sebelum Merge (non-negotiable)

- [ ] **Round-trip ALTER**: buka dengan passphrase B → Ed25519 keypair benar (verifikasi via sign + verify)
- [ ] **Round-trip password manager**: buka dengan passphrase A → entries terbaca utuh, tidak ada korupsi
- [ ] **Never-fail**: passphrase acak (bukan A atau B) selalu buka sesuatu — tidak pernah `Err` yang observable
- [ ] **Indistinguishability**: `stat()` pada file vault → ukuran selalu 4096 bytes; tidak ada metadata mode yang bocor
- [ ] **No timing leak**: `vault_open(passphrase_a)` vs `vault_open(passphrase_b)` — durasi tidak berbeda signifikan. Argon2id mendominasi; verifikasi dengan `cargo bench`
- [ ] **Independence**: ekstrak `key_a`, tidak bisa derive `key_b` dari informasi apapun di file vault
- [ ] **Migration**: vault format v1 (v0.2.x) terdeteksi, migrasi ke v2 berjalan tanpa kehilangan kunci ALTER
- [ ] **Stress**: 1000 open/close cycles tanpa memory leak atau panic
- [ ] **Fungsionalitas PM**: tambah → simpan → buka ulang → entry masih ada; hapus → entry hilang permanen

---

## 6. Roadmap v0.4

| Fase | Scope | Status | Catatan |
|---|---|---|---|
| **M5 — Presence Privacy + Contact Management** | SEC-13 (Restricted Discovery per kontak), SEC-14 (lyrebird detection), SEC-04 (mlock), UX-01 (rename/delete kontak) | ✅ Locked | Breaking change invite code disetujui. Dep baru: `memsec`. Resolver hot-add: opsi 1 (restart onion service). |
| **M6 — Password Manager Decoy Front** | SEC-15 (password manager TUI fungsional, dual-slot vault format v2) | ✅ Locked | High-risk — wajib lulus semua 9 test checklist Bagian 5.4 sebelum merge. Dep baru: tidak ada (semua crypto sudah ada). |

**File transfer (FT-01) dipindah ke PRD v0.5** — ALTER tetap fokus sebagai aplikasi chat teks untuk v0.4.

---

## 7. Primitif Kriptografi — Update dari v0.3

> Semua baris dari v0.3 tetap berlaku. Baris baru dan update:

| Fungsi | Primitif | Library (Rust) | Versi | Catatan |
|---|---|---|---|---|
| Noise handshake | Noise_IK | `snow` | **0.10.0** (Juli 2024) | ⚠️ **Masih belum ada formal security audit** (pernyataan resmi di README v0.10.0). Breaking: builder functions return `Result`. MSRV naik ke 1.85. v0.9.5 fix nonce increment bug (GHSA-7g9j-g5jg-3vv3). ALTER tetap gunakan v0.10.x. Risiko R-06 dipertahankan terbuka. |
| Memory locking | mlock | `memsec` | **0.6** | Port Rust dari libsodium utils. Tambah di M5. |
| Decoy vault AEAD | XChaCha20-Poly1305 | `chacha20poly1305` | **0.10.1** | Sudah ada. Dipakai untuk SEC-15 (M6). |
| Presence privacy | Tor v3 client-auth | `tor-hsservice` | **0.43.0** | `HsClientDescEncKey`, `restricted_discovery`. Sudah ada di Cargo.toml. |

---

## 8. Keputusan Arsitektur — Status (26 Juni 2026)

Semua keputusan dikunci per 26 Juni 2026.

| Keputusan | Pilihan | Alasan |
|---|---|---|
| **A: Format invite code baru (SEC-13)** | ✅ **Breaking change** — user regenerate semua kontak saat upgrade ke M5 | Optional backward-compat = "privacy second class"; kontak lama tetap bisa di-probe. Bertentangan prinsip "fail closed". ALTER v0.2.0 — momen terbaik untuk breaking change bersih. |
| **B: Scope file transfer (FT-01)** | ✅ **Dipindah ke PRD v0.5** — ALTER tetap chat teks di v0.4 | File transfer menambah kompleksitas UI dan dep baru (`tokio-util`); lebih baik M5–M6 selesai dulu sebelum expand scope. |
| **C: Decoy vault (SEC-15)** | ✅ **Dimasukkan ke M6** — milestone tersendiri agar dapat fokus penuh | Risiko implementasi tinggi (bug = kehilangan kunci permanen). Wajib lulus 9 test checklist di Bagian 5.4 sebelum merge. |
| **F: Decoy front — password manager vs opsi lain (SEC-15)** | ✅ **Password manager** (bukan TOTP, journal, atau decoy identity ALTER) | TOTP: verifikasi adversary instan (kode 6-digit bisa langsung dicek). Journal: plausibel tapi kurang convincing untuk user CLI. Decoy identity ALTER: masih kelihatan seperti app chat. Password manager: friction verifikasi tertinggi (tidak bisa langsung di-test di tempat) + natural untuk user teknis + dapat diisi konten real. |
| **D: Contact management (UX-01)** | ✅ **Dimasukkan ke M5** — bukan M6 | User sudah harus re-add semua kontak saat M5 upgrade. Aneh kalau rename/delete belum tersedia di momen itu. |
| **E: Hot-add key problem (SEC-13)** | ✅ **Opsi 1: restart onion service saat kontak baru ditambah** | Konsisten dengan prinsip "fail closed"; downtime ~5–10 detik per add-contact dianggap acceptable. |

---

## 9. Metrik Keberhasilan (v0.4)

| Metrik | Target | Catatan |
|---|---|---|
| Presence detectability | Koneksi dari pihak tanpa client_auth_key gagal resolve descriptor | Verifikasi: test dengan koneksi Tor tanpa key |
| mlock coverage | 100% secret pages di-mlock saat di-RAM | Verifikasi: `/proc/[pid]/smaps` tidak tunjukkan swap-eligible secret pages di Linux |
| lyrebird detection | Detect lyrebird binary jika ada di PATH, badge TUI tampil | Test: tambah lyrebird ke PATH sementara, verifikasi badge |
| Contact management | Rename + delete kontak berfungsi; delete meminta konfirmasi | Test: hapus kontak, verifikasi tidak ada sisa di contact store |
| Onion service reload | Tambah kontak baru → onion service restart < 15 detik | Test manual; TUI harus tampilkan status "memuat ulang..." |
| Password manager decoy | File vault selalu 4096 bytes; 9 test checklist Bagian 5.4 pass; password manager bisa tambah/lihat/hapus entry | Semua test wajib pass sebelum merge |
| Adversary verification friction | Passphrase A membuka password manager dengan entries yang convincing; tidak ada cara verify instan di tempat | Test manual: simulasi skenario pemeriksaan |

---

## 10. Batasan & Yang Tidak Dijamin — Tambahan v0.4

Ini tambahan ke Bagian 10 PRD v0.3. Semua item v0.3 tetap berlaku.

- **Restricted Discovery bukan jaminan anonimitas mutlak.** Tor v3 client-auth mencegah pihak tanpa key dari *me-resolve* descriptor. Namun relay Tor yang berada di posisi strategis masih bisa mengamati timing koneksi (traffic correlation attack). Gosling full architecture (belum production-ready per Juni 2026) memberikan isolasi lebih kuat via per-kontak onion endpoint terpisah.
- **mlock tidak melindungi saat hibernate/suspend.** RAM dump ke disk terjadi saat laptop masuk mode hibernate (sleep-to-disk). Ini limitasi hardware, bukan aplikasi.
- **mlock di Windows membutuhkan privilege.** `VirtualLock` pada Windows memerlukan elevated privilege atau penambahan working set quota. ALTER tetap berfungsi tanpa mlock di Windows; hanya zeroize yang dijamin.

---

## 11. Risiko & Open Questions (Update)

### Resolved di v0.4

| Item | Status | Jawaban |
|---|---|---|
| R-02 (obfs4proxy bundling) | ✅ **Resolved** | Runtime detection dipilih di M3. v0.4 update: detect lyrebird (aktif) DAN obfs4proxy (legacy). |
| R-03 (binary size) | ✅ **Resolved** | Runtime detection strategy menjaga binary ALTER < 5 MB. |
| R-01 (arti breaking changes) | ✅ **Resolved** | arti-client 0.43.0 stabil, breaking change arti 2.0.0 tidak mempengaruhi crate `arti-client`. |

### Open — Tetap Terbuka

| ID | Risiko/Pertanyaan | Catatan | Prioritas |
|---|---|---|---|
| R-05 | **Online presence leak** | Resolved parsial di M5 via Restricted Discovery. Full Gosling (per-peer isolated onion) ditunda sampai `gosling` crate v1.x production-ready. | Sedang (M5 mengurangi surface) |
| R-06 | **snow — belum ada formal audit** | v0.10.0 Juli 2024 masih tanpa audit. Acknowledged risk. Monitor audit announcements. | Sedang |
| R-08 | **Invite code breaking change** | ✅ **Resolved** — Breaking change dipilih (Bagian 8, Keputusan A). User upgrade M5 harus regenerate invite code dan re-add kontak. | — |
| R-09 | **Password manager decoy implementation risk** | Bug dalam dual-slot vault = kehilangan kunci asli permanen. Dimitigasi via 9 test checklist wajib (Bagian 5.4) yang harus lulus sebelum merge M6. Password manager kosong = mencurigakan; user perlu diingatkan mengisi decoy entries setelah setup. | Tinggi — aktif |
| R-10 | **snow MSRV naik ke 1.85** | snow v0.10.0 memerlukan rustc >= 1.85. ALTER sudah gunakan MSRV 1.89 (dari M2) — tidak ada masalah. | ✅ Resolved |
| R-11 | **SEC-13 hot-add key problem** | ✅ **Resolved** — Opsi 1 dipilih: restart onion service saat kontak baru ditambah. TUI harus tampilkan status loading selama proses (~5–10 detik). | — |

---

## 12. Glosarium — Tambahan v0.4

- **Tor v3 Client Authorization / Restricted Discovery**: fitur onion service di mana descriptor dienkripsi untuk klien spesifik (via x25519 pubkey). Klien tanpa key tidak bisa membuka descriptor; descriptor tetap ada di relay tapi tidak terbaca → efektif mencegah presence probing.
- **lyrebird**: fork aktif resmi Tor Project dari `obfs4proxy` (Go). Menggantikan obfs4proxy yang unmaintained sejak 2022. Fungsi identik — menyamarkan traffic Tor via obfs4 pluggable transport.
- **mlock**: syscall Unix (+ `VirtualLock` Windows) yang mencegah kernel mem-paging halaman RAM tertentu ke swap disk. Digunakan untuk memori yang berisi kunci kriptografi.
- **Gosling architecture**: framework dua-tier onion service (dikembangkan Blueprint for Free Speech). Tier 1 = identity server publik (boleh di-probe). Tier 2 = endpoint server per-kontak (descriptor dienkripsi, hanya diketahui pasangan). Digunakan di Ricochet-Refresh v4 (dalam pengembangan).
- **Password Manager Decoy Front**: ALTER menyamar sebagai password manager terminal. Passphrase A membuka TUI password manager yang fungsional (simpan/lihat credentials). Passphrase B membuka ALTER chat. File vault selalu 4096 bytes, identik dari luar untuk kedua mode. Dipilih karena verifikasi adversary tidak instan — password tidak bisa langsung di-test di tempat, berbeda dengan TOTP yang bisa dicek seketika.
- **Dual-slot vault**: vault format v2 dengan dua slot Argon2id independen (salt berbeda, kunci tidak berkorelasi). Slot A berisi data password manager, Slot B berisi ALTER keypair. Tidak pernah return error untuk passphrase apapun — passphrase tidak dikenal membuka password manager kosong.

---

## 13. Changelog

### v0.4 (26 Juni 2026) — Planning pasca-v1, M5–M7

**Konteks:** ALTER v0.2.0 menyelesaikan seluruh scope v1 (M0–M4). PRD v0.4 mendefinisikan v2 dengan dua milestone: M5 (Presence Privacy + Contact Management) dan M6 (Password Manager Decoy Front). File transfer (FT-01) dipindah ke PRD v0.5.

**Perubahan dari v0.3:**

1. **Bagian 1 (Konteks)** — Menggantikan Executive Summary v0.3; dokumentasikan status M0–M4 selesai dan konteks v2.

2. **Bagian 2 (Open Questions update)** — Evaluasi ulang semua R-01 s/d R-07; tutup R-01/R-02/R-03; buka R-08/R-09/R-10/R-11.

3. **Bagian 3 (Scope)** — Dua tema: M5 (Presence Privacy + UX-01 contact management) dan M6 (Password Manager Decoy Front). FT-01 eksplisit dipindah ke v0.5.

4. **Bagian 4 (Security Tiers)** — Tambah SEC-13, SEC-14, update SEC-04, tambah SEC-15 (M6) dan UX-01 (M5).

5. **Bagian 5 (Arsitektur)** — 5.1: SEC-13 + catatan risiko hot-add key + keputusan opsi 1. 5.2: SEC-14 lyrebird. 5.3: SEC-04 mlock via memsec. 5.4: SEC-15 **password manager decoy front** — rewrite total dari konsep "decoy identity" ke "password manager TUI fungsional" + dual-slot vault dengan salt independen + **9-item test checklist wajib**.

6. **Bagian 6 (Roadmap)** — M5 dan M6 (bukan M5/M6/M7). FT-01 → PRD v0.5.

7. **Bagian 7 (Primitif)** — Update row chacha20 (dari FT-01 → SEC-15). Tambah memsec.

8. **Bagian 8 (Keputusan)** — Enam keputusan locked: A (breaking change invite), B (FT-01 → v0.5), C (SEC-15 → M6), D (UX-01 → M5), E (hot-add opsi 1), **F (password manager sebagai decoy front — menggantikan konsep decoy identity ALTER; alasan: friction verifikasi adversary tertinggi)**.

9. **Bagian 9 (Metrik)** — Update: hapus file transfer, tambah contact management + onion reload + password manager decoy (9 test) + adversary friction metric.

10. **Bagian 11 (Risiko)** — R-08 resolved, R-10 resolved, R-11 baru (resolved). R-09 diperbarui: password manager kosong = suspicious, user perlu diingatkan mengisi entries.

11. **Bagian 12 (Glosarium)** — Tambah: Restricted Discovery, lyrebird, mlock, Gosling architecture, Password Manager Decoy Front, Dual-slot vault.

**Yang TIDAK berubah (locked dari v0.3):**
- Room-Bound Sync model
- Threat model (state-level adversary)
- Primitif kriptografi inti (Noise_IK, ChaCha20, Argon2id)
- Arsitektur dual transport (LAN + Tor)
- Nama proyek "ALTER"
- SEC-01 s/d SEC-12 dari v0.3

---

### v0.3 (22 Juni 2026) — Room-Bound Sync Model
Lihat versi lengkap di `PRD-alter-v0.3.md`.

### v0.2 (22 Juni 2026) — Validasi Teknis
Lihat versi lengkap di `PRD-alter-v0.2.md`.

### v0.1 (22 Juni 2026) — Draft awal
Lihat versi lengkap di `PRD-alter.md`.

---

*Akhir dokumen v0.4. Semua keputusan di Bagian 8 sudah locked — M5 selesai dieksekusi (26 Juni 2026), M6 siap diimplementasi.*
