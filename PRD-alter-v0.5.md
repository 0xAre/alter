# PRD: ALTER — FT-01 Ephemeral File Transfer

> **Status dokumen:** Draft v0.5 — Finalized (semua open questions resolved)
> **Tanggal v0.5:** 30 Juni 2026
> **Base dokumen:** PRD-alter-v0.4.md (semua keputusan v0.4 tetap berlaku)

---

## 1. Konteks & Motivasi

ALTER v0.5.0 (M0–M6 selesai) menyelesaikan seluruh scope awal: chat teks P2P end-to-end terenkripsi, Tor built-in, vault dual-slot, password manager decoy front. FT-01 menambahkan kemampuan transfer file ke atas infrastruktur yang sudah ada — tanpa server, tanpa relay.

Filosofi core ALTER tidak berubah: **ephemeral by design**. File tidak pernah disimpan otomatis. Penerima memilih secara eksplisit — simpan, lihat inline, atau tolak. Jika room tutup saat transfer berlangsung, semua data in-memory di-drop dan di-zeroize.

> **Prinsip panduan:** Pengirim memilih berbagi. Penerima memilih menyimpan. Tidak ada keputusan yang dibuat atas nama user tanpa konfirmasi eksplisit.

**Tidak termasuk FT-01:** streaming media, transfer saat peer offline, file hosting/relay, protocol resumption, transfer dari mode non-TUI (`alter id`).

---

## 2. Scope FT-01

| Item | Status | Catatan |
|---|---|---|
| Transfer file arbitrary | ✅ In scope | Semua tipe file. Tidak ada whitelist ekstensi. |
| Render gambar inline | ✅ In scope | image/* ≤ 10 MB. Kitty/Sixel/half-block fallback. |
| Simpan ke disk (pilihan) | ✅ In scope | `~/Documents/vault-exports/` — hanya jika penerima pilih [S]. |
| Progress indicator + in-transit chat line | ✅ In scope | Status bar di chat selama transfer berlangsung. |
| Streaming media | ❌ Out of scope | Butuh buffering khusus. Defer ke FT-02. |
| Transfer saat peer offline | ❌ Out of scope | Bertentangan dengan model ephemeral ALTER. |
| Resume setelah disconnect | ❌ Out of scope | Butuh state persistence. Defer ke FT-02. |
| Transfer multi-file sekaligus | ❌ Out of scope | FT-01: satu transfer aktif per sesi. |

---

## 3. Keputusan Desain (Locked)

Semua keputusan di bawah diputuskan dalam sesi desain 30 Juni 2026. Tidak perlu dipertanyakan ulang saat implementasi.

| ID | Keputusan | Rasional |
|---|---|---|
| FT-D01 | Transport: **in-session multiplexing** via type byte | Reuse enkripsi Noise_IK; tidak perlu koneksi/port baru; zero infrastructure tambahan. |
| FT-D02 | Batas "Lihat" inline: **≤ 10 MB dan image/* only** | Di atas threshold, rendering di RAM tidak praktis. User dipaksa pilih simpan atau tolak. |
| FT-D03 | Room close mid-transfer: **drop + zeroize** | Konsisten dengan model ephemeral ALTER. Tidak ada partial file di disk. |
| FT-D04 | Inisiasi: **Ctrl+F** di dalam chat room | Pola konsisten dengan shortcut ALTER lain. Input bar di bawah — path bisa diketik atau di-paste. |
| FT-D05 | Integritas: **SHA-256 end-to-end** | Verifikasi setelah semua chunk diterima, sebelum prompt ke user. `sha2` sudah transitive dep — tidak perlu tambah ke Cargo.toml. |
| FT-D06 | Simpan ke: **~/Documents/vault-exports/\<timestamp\>\_\<filename\>** | Nama folder menyamar sebagai export dari password manager decoy (SEC-15) — konsisten dengan persona ALTER. |
| FT-D07 | Render gambar: **viuer** (Kitty → Sixel → half-block) | Auto-detect terminal capability; degraded gracefully di semua terminal. |
| FT-D08 | Chunk size: **64 KB** per chunk | Standard P2P size; cocok untuk progress tracking; tidak overload buffer Noise. |
| FT-D09 | Protocol negotiation: **post-handshake Capability frame** | Lebih clean daripada modifikasi handshake payload — tidak menyentuh kriptografi yang delicate. Mudah extend ke depan. |
| FT-D10 | Timeout: **dynamic** — `30s + ceil(total_bytes / 10_240)` detik | Base 30s + estimasi 10 KB/s throughput Tor. Mencegah false timeout untuk file besar via Tor yang lambat. |
| FT-D11 | In-transit chat indicator | Saat transfer aktif, render status line di area chat (bukan di history): `→ Mengirim foto.jpg... 62%` / `← Menerima foto.jpg... 62%`. Completed transfer → tambah ChatLine permanen ke history. |

---

## 4. Spesifikasi Protokol

### 4.1 Protocol Capability Negotiation (Post-Handshake)

Setelah Noise_IK handshake selesai dan session masuk state `Active`, **kedua peer langsung mengirim Capability frame** sebagai pesan pertama:

```
Frame type: 0x05 Capability
Payload: JSON UTF-8

{
  "version": 2,
  "features": ["file_transfer"]
}
```

Keduanya mengirim dan menunggu Capability dari sisi lain (satu RTT tambahan). Hasil negosiasi:

| Kondisi | Hasil |
|---|---|
| Kedua peer: `version ≥ 2`, `features` include `"file_transfer"` | FT-01 aktif — type-byte framing dipakai |
| Salah satu peer: `version = 1` atau tidak mengirim Capability dalam 5 detik | Fall back ke text-only. `Ctrl+F` di-disable. |

**Mengapa post-handshake, bukan di handshake payload:**
- Tidak menyentuh format handshake kriptografis yang sudah teruji
- Mudah diextend — fitur baru cukup tambah string ke array `features`
- Jika future ALTER perlu negosiasi lebih kompleks, cukup versi JSON, tidak perlu ubah handshake

### 4.2 Frame Types

Pada sesi dengan FT-01 aktif (kedua peer v2+), setiap plaintext payload (setelah decrypt Noise AEAD) diawali 1 byte type:

```
[type: 1 byte][payload: variable]

0x00  TextMsg     — pesan teks (payload = UTF-8 string)
0x01  FileHeader  — metadata file (payload = JSON UTF-8)
0x02  FileChunk   — data chunk (payload = [4B LE u32: index][raw bytes ≤ 65536])
0x03  FileAck     — respons penerima (payload = [1B: 0x00=accept, 0x01=reject])
0x04  FileCancel  — pembatalan (payload = UTF-8 reason string)
0x05  Capability  — negosiasi kapabilitas (payload = JSON UTF-8)
```

Pada sesi text-only (fall back), seluruh plaintext payload dianggap TextMsg tanpa type byte.

### 4.3 FileHeader Payload

JSON-encoded, UTF-8, dikirim sebagai satu frame sebelum chunk pertama:

```json
{
  "name":        "foto_liburan.jpg",
  "mime":        "image/jpeg",
  "total_bytes": 2347851,
  "chunk_count": 36,
  "sha256":      "a3f2...9c1d"
}
```

- `name`: filename saja (bukan path); di-sanitize via `Path::file_name()` di sisi penerima
- `mime`: MIME type dari magic bytes file — pakai crate `infer`, bukan ekstensi (bisa dipalsukan)
- `chunk_count`: `ceil(total_bytes / 65536)`
- `sha256`: hex SHA-256 seluruh file sebelum chunking

### 4.4 Chunk Protocol

```
// Layout per chunk frame (payload setelah type byte 0x02)
[index: 4 bytes, little-endian u32][chunk data: 1..=65536 bytes]

// Chunk terakhir boleh lebih kecil dari 65536 bytes
// Penerima buffer semua chunk berdasarkan index
// Transfer selesai saat semua index 0..(chunk_count-1) diterima
```

Pengirim tidak menunggu ack per chunk — kirim semua chunk sequential. Noise transport handle flow control via TCP/Tor.

**Dynamic timeout per chunk:**
```
timeout_per_chunk = 30 + ceil(chunk_size / 10_240) detik
total_transfer_timeout = 30 + ceil(total_bytes / 10_240) detik
```

Baseline throughput Tor: ~10 KB/s (konservatif). Timeout di-reset setiap kali chunk baru diterima.

### 4.5 Integrity Verification

Setelah chunk terakhir diterima, sebelum prompt ke user:

```
1. Reassemble semua chunk berdasarkan index (urut 0..chunk_count)
2. SHA-256(reassembled_bytes) == header.sha256
     → match:    lanjut ke prompt [S/L/T]
     → mismatch: kirim 0x04 FileCancel, drop buffer, notif error ke user
3. Buffer di-zeroize jika tidak disimpan ke disk
```

### 4.6 Cancellation

Siapa pun (pengirim atau penerima) boleh kirim `0x04 FileCancel` kapan saja selama transfer aktif. Pihak yang menerima cancel:
- Drop semua chunk yang sudah ada di buffer
- Zeroize buffer
- Tampilkan di chat: system line "Transfer dibatalkan."

---

## 5. UX Flow

### 5.1 Pengirim

| # | Aksi | UI Feedback |
|---|---|---|
| 1 | Di dalam room (Mode::Chat), tekan `Ctrl+F` | Input bar muncul di bawah area chat |
| 2 | Ketik atau paste path file | `Kirim file › /home/user/foto.jpg▏` |
| 3 | Tekan Enter | ALTER validasi: file ada, bisa dibaca, ukuran < 4 GB |
| 4 | Konfirmasi preview | `foto.jpg · 2.3 MB · image/jpeg — [Enter] kirim [Esc] batal` |
| 5 | Tekan Enter lagi | Status line muncul di chat: `→ Mengirim foto.jpg (2.3 MB)... [████░░░░] 58%` |
| 6 | Selesai | Status line diganti ChatLine permanen: `→ [✓] foto.jpg terkirim.` |

### 5.2 In-Transit Chat Indicator

Selama transfer berlangsung, **satu baris status** dirender di bagian bawah area chat (di atas input box, tidak masuk ke message history). Baris ini update setiap tick render:

```
// Sisi pengirim
→ Mengirim foto.jpg (2.3 MB)... [████████░░] 80%   [Ctrl+C batal]

// Sisi penerima (saat sedang menerima chunk setelah pilih [S] atau [L])
← Menerima foto.jpg dari Bob... [████████░░] 80%
```

Saat transfer selesai atau dibatalkan, status line hilang dan ChatLine permanen ditambahkan ke message history:

```
// Selesai - pengirim
·  [✓] foto.jpg terkirim.

// Selesai - penerima simpan
·  [✓] foto.jpg disimpan.

// Selesai - penerima lihat inline
·  [gambar dirender, tidak disimpan]

// Dibatalkan
·  Transfer foto.jpg dibatalkan.
```

### 5.3 Penerima — Prompt

Saat `FileHeader` diterima (sebelum transfer mulai), modal prompt muncul:

**Untuk image ≤ 10 MB (semua opsi tersedia):**
```
┌─ Transfer file masuk ───────────────────────────────┐
│                                                     │
│  ◎ Bob mengirim: foto_liburan.jpg                   │
│  2.3 MB · image/jpeg                                │
│                                                     │
│  [S] Simpan   [L] Lihat inline   [T] Tolak          │
│                                                     │
└─────────────────────────────────────────────────────┘
```

**Untuk file > 10 MB atau bukan gambar:**
```
┌─ Transfer file masuk ───────────────────────────────┐
│                                                     │
│  ◎ Bob mengirim: laporan_keuangan.pdf               │
│  45.2 MB · application/pdf                          │
│  ⚠  File besar — transfer bisa membutuhkan waktu   │
│                                                     │
│  [S] Simpan ke disk   [T] Tolak                     │
│                                                     │
└─────────────────────────────────────────────────────┘
```

| Pilihan | Behavior |
|---|---|
| **[S] Simpan** | Transfer mulai. Status line muncul di chat. Setelah SHA-256 verified → tulis ke `~/Documents/vault-exports/<unix_ts>_<name>`. ChatLine: `[✓] foto.jpg disimpan.` |
| **[L] Lihat** | Hanya untuk image/* ≤ 10 MB. Transfer di background. Status line muncul. Setelah verified → render inline via `viuer`. File tidak ada di disk. Buffer di-zeroize setelah render. |
| **[T] Tolak** | Kirim `0x04 FileCancel`. Modal tutup. ChatLine: `Transfer ditolak.` Tidak ada transfer dimulai. |

### 5.4 Rendering Gambar Inline

Crate `viuer` auto-detect kemampuan terminal:

| Level | Protokol | Terminal | Kualitas |
|---|---|---|---|
| 1 | Kitty Graphics Protocol | Kitty, WezTerm, Ghostty | Full color, pixel-perfect |
| 2 | Sixel | XTerm, WezTerm fallback, Windows Terminal (exp.) | Good |
| 3 | Unicode half-block (▀▄) | Semua terminal | Pixelated, universal |

Gambar dirender langsung di area chat di bawah sistem line yang sesuai. Lebar max = lebar panel chat. Buffer di-zeroize setelah render.

---

## 6. Batasan & Edge Cases

| Skenario | Perilaku yang Diharapkan |
|---|---|
| Room ditutup saat transfer in-progress | Drop chunk buffer, zeroize, kirim `0x04 FileCancel` ke peer jika koneksi masih hidup. |
| Peer disconnect mid-transfer | Dynamic timeout habis → cancel otomatis, notif error, buffer di-zeroize. |
| File dihapus setelah path diinput | Error saat buka file. Notifikasi ke pengirim. Batal tanpa mengirim apapun ke peer. |
| Nama file berisi karakter berbahaya (`/`, `\`, `..`) | `Path::file_name()` ambil komponen terakhir saja. Reject jika hasilnya kosong atau dimulai `.`. |
| SHA-256 mismatch setelah semua chunk | Auto-cancel, notif error, tidak ada file di disk, buffer di-zeroize. |
| Disk penuh saat simpan | Error dengan pesan jelas. Cleanup file partial. Tidak ada data tersisa. |
| Transfer kedua saat satu aktif | FT-01: satu transfer per sesi. Permintaan baru ditolak dengan notif, tidak interrupt yang berjalan. |
| File > 4 GB | Ditolak di sisi pengirim sebelum dikirim. Notif: "File terlalu besar untuk FT-01." |
| Peer protocol v1 (ALTER < v0.6) | `Ctrl+F` disabled. Notif: "Peer tidak mendukung file transfer — minta upgrade ke ALTER ≥ v0.6.0." |
| `~/Documents/vault-exports/` belum ada | Dibuat lazy saat pertama kali file disimpan (bukan saat startup). |
| Capability frame tidak datang dalam 5 detik | Session fall back ke text-only, `Ctrl+F` disabled, tidak error/crash. |

---

## 7. Keamanan

| Properti | Implementasi |
|---|---|
| **E2E Enkripsi** | File data melewati Noise_IK transport — ChaCha20Poly1305 AEAD per chunk, otomatis. |
| **Integritas** | SHA-256 end-to-end + per-chunk AEAD via Noise. Dua lapisan perlindungan. |
| **No server / no relay** | Transfer langsung P2P. Tidak ada upload ke pihak ketiga. |
| **Ephemeral by default** | File tidak disimpan kecuali penerima eksplisit pilih [S]. Room close = data hilang. |
| **Zeroize on drop** | Chunk buffer dan image buffer di-`zeroize()` setelah dipakai atau saat room tutup. |
| **Path sanitization** | Nama file dari FileHeader peer di-sanitize via `Path::file_name()` sebelum dipakai sebagai nama file lokal. |
| **No auto-execute** | File yang disimpan tidak pernah di-execute. ALTER hanya tulis bytes ke disk. |
| **Folder plausible deniability** | `~/Documents/vault-exports/` — jika ditemukan adversary, tampak sebagai export dari password manager decoy (konsisten dengan SEC-15). |

**Tidak dilindungi FT-01:** Metadata ukuran file dan durasi transfer diketahui peer (by design — perlu progress bar). Keamanan file setelah disimpan ke disk di luar scope ALTER. Traffic correlation via Tor relay tetap mungkin.

---

## 8. Arsitektur Implementasi

### 8.1 File Baru / Diubah

| File | Perubahan |
|---|---|
| `src/session/mod.rs` | Tambah `enum MessageType` (6 variant). Modifikasi `recv_message()` / `send_message()`: parse/prepend type byte untuk sesi v2. Tambah capability negotiation logic setelah handshake. |
| `src/session/file_transfer.rs` *(baru)* | `struct FileTransferState` (chunk buffer, metadata, progress, timeout). `fn receive_chunk()`, `fn verify_and_finalize()`, `fn cancel()`. Dynamic timeout tracking. |
| `src/tui/types.rs` | Tambah `enum FileTransferUiState`: None \| Receiving(progress) \| Prompting(FileHeader) \| Sending(progress). Field baru di `App`: `file_transfer: FileTransferUiState`, `peer_ft_capable: bool`. |
| `src/tui/chat.rs` | Handler `Ctrl+F` (cek `peer_ft_capable` dulu). `handle_send_file_key()`, `start_file_transfer()`. |
| `src/tui/ui/main.rs` | `render_file_prompt_modal()`. `render_transfer_status_line()` di bawah chat area. |
| `src/tui/ui/image.rs` *(baru)* | `render_image_inline()` — wrapper `viuer` dengan lebar panel sebagai constraint. |
| `Cargo.toml` | Tambah `viuer`, `infer`. **Tidak** tambah `sha2` (sudah transitive via arti/snow). |

### 8.2 Crate Baru

| Crate | Version | Kegunaan | Cargo.toml |
|---|---|---|---|
| `viuer` | 0.9 | Render gambar di terminal — auto-detect Kitty/Sixel/half-block. | Tambah eksplisit |
| `infer` | 0.16 | MIME type detection dari magic bytes — bukan ekstensi file. | Tambah eksplisit |
| `sha2` | 0.10 | SHA-256 end-to-end integrity. | **Sudah ada transitive** (`sha2 v0.10.9`) — tidak perlu tambah |

---

## 9. Acceptance Criteria

FT-01 dianggap selesai jika semua item berikut pass:

- [ ] **FT-AC-01** — File < 10 MB terkirim dan diterima. SHA-256 verified. Konten byte-identical.
- [ ] **FT-AC-02** — File > 100 MB dapat terkirim dengan warning di prompt. Bukan hard block.
- [ ] **FT-AC-03** — SHA-256 mismatch → auto-cancel. Tidak ada file di disk. Buffer di-zeroize.
- [ ] **FT-AC-04** — Room tutup mid-transfer → chunk buffer di-zeroize. Tidak ada partial file. Peer dapat FileCancel.
- [ ] **FT-AC-05** — Gambar JPEG/PNG ≤ 10 MB tampil inline di terminal (minimal half-block mode).
- [ ] **FT-AC-06** — File > 10 MB tidak tampilkan opsi [L] Lihat di prompt modal.
- [ ] **FT-AC-07** — Path traversal di FileHeader `name` (`../`, `/etc/passwd`) → di-strip. File disimpan aman di `~/Documents/vault-exports/`.
- [ ] **FT-AC-08** — Peer protocol v1 (tidak kirim Capability) → `Ctrl+F` disabled. Notifikasi jelas di TUI.
- [ ] **FT-AC-09** — Penerima pilih [T] Tolak → pengirim terima notifikasi. Tidak ada retry otomatis. Transfer tidak dimulai.
- [ ] **FT-AC-10** — File tersimpan dengan format `~/Documents/vault-exports/<unix_ts>_<name>`. Tidak overwrite file lain.
- [ ] **FT-AC-11** — Transfer kedua saat satu aktif → ditolak dengan notif. Transfer pertama tidak terganggu.
- [ ] **FT-AC-12** — Dynamic timeout habis → cancel otomatis. Tidak ada data di disk.
- [ ] **FT-AC-13** — In-transit status line tampil di chat selama transfer (`→ Mengirim... 62%`). Hilang saat selesai.
- [ ] **FT-AC-14** — Setelah transfer selesai (sukses atau gagal), ChatLine permanen ditambahkan ke message history.

---

## 10. Changelog

**v0.5** — 30 Juni 2026: Initial draft FT-01.
**v0.5.1** — 30 Juni 2026: Semua open questions resolved:
- FT-Q01: Capability negotiation → post-handshake (Bagian 4.1 ditulis ulang, FT-D09 diperbarui)
- FT-Q02: Timeout → dynamic formula + in-transit chat indicator ditambahkan (FT-D10, FT-D11, Bagian 5.2 baru)
- FT-Q03: Folder simpan → `~/Documents/vault-exports/` (FT-D06 diperbarui, lazy creation)
- FT-Q04: `sha2` confirmed transitive dep (v0.10.9) — tidak perlu tambah ke Cargo.toml
