//! Main screen key handler, connection management, messaging, session events.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use zeroize::Zeroize;

use crate::contacts;
use crate::session::{self, SessionCmd, SessionEvent, SessionState};
use crate::session::file_transfer::CHUNK_SIZE;
use crate::transport::{self, LanMode};

use super::app::App;
use super::contact::{persist_contacts, copy_invite, trigger_tor_restart};
use super::types::{ChatLine, FileTransferUiState, Mode, RoomState};

pub(super) fn handle_main_key(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<SessionCmd>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
    key: KeyEvent,
) -> bool {
    // FT-01: intercept [s/l/t] saat modal "file diterima" aktif, apapun mode-nya.
    if matches!(&app.file_transfer, FileTransferUiState::Received { .. }) {
        return handle_received_file_key(app, key);
    }

    match app.mode {
        Mode::Browsing => {
            if let Some(idx) = app.pending_delete {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => delete_contact(app, idx),
                    _ => {
                        app.pending_delete = None;
                        app.set_notif_info("Hapus dibatalkan.");
                    }
                }
                return false;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return true,
                KeyCode::Char('i') => app.show_invite = !app.show_invite,
                KeyCode::Char('c') => copy_invite(app),
                KeyCode::Char('a') => {
                    app.mode = Mode::AddContact;
                    app.add_buffer.clear();
                    app.notification = None;
                }
                KeyCode::Char('r') => {
                    if !app.contacts.is_empty() {
                        app.rename_buffer = app.contacts[app.selected].nickname.clone();
                        app.mode = Mode::RenameContact;
                    }
                }
                KeyCode::Char('d') => {
                    if app.contacts.is_empty() {
                        app.set_notif_info("Belum ada kontak untuk dihapus.");
                    } else {
                        app.pending_delete = Some(app.selected);
                    }
                }
                KeyCode::Up => {
                    if app.selected > 0 {
                        app.selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.selected + 1 < app.contacts.len() {
                        app.selected += 1;
                    }
                }
                KeyCode::Enter => start_connection(app, out_tx, ev_rx),
                _ => {}
            }
        }

        Mode::AddContact => match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Browsing;
                app.add_buffer.clear();
            }
            KeyCode::Backspace => { app.add_buffer.pop(); }
            KeyCode::Enter => add_contact_from_buffer(app),
            KeyCode::Char(c) => {
                if app.add_buffer.len() < 200 { // invite code v2 = 128 chars base64
                    app.add_buffer.push(c);
                }
            }
            _ => {}
        },

        Mode::RenameContact => match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Browsing;
                app.rename_buffer.clear();
            }
            KeyCode::Backspace => { app.rename_buffer.pop(); }
            KeyCode::Enter => {
                let new_name = app.rename_buffer.trim().to_string();
                if !new_name.is_empty() && !app.contacts.is_empty() {
                    let old_name = app.contacts[app.selected].nickname.clone();
                    app.contacts[app.selected].nickname = new_name.clone();
                    persist_contacts(app);
                    app.set_notif_success(format!("[✓] '{old_name}' → '{new_name}'"));
                }
                app.mode = Mode::Browsing;
                app.rename_buffer.clear();
            }
            KeyCode::Char(c) => {
                if app.rename_buffer.len() < 64 {
                    app.rename_buffer.push(c);
                }
            }
            _ => {}
        },

        Mode::InRoom => {
            // FT-01: Ctrl+F buka modal kirim file
            if key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if !app.peer_ft_capable {
                    app.set_notif_warn("[!] Peer belum mendukung file transfer.");
                } else if app.room == RoomState::Open {
                    if matches!(&app.file_transfer,
                        FileTransferUiState::Prompting(..)
                        | FileTransferUiState::Sending { .. }
                        | FileTransferUiState::Receiving { .. })
                    {
                        app.set_notif_warn("[!] Transfer sedang berlangsung.");
                    } else {
                        app.mode = Mode::SendFile;
                        app.send_file_buffer.clear();
                    }
                }
                return false;
            }
            match key.code {
                KeyCode::Esc => {
                    if app.replying_to.take().is_some() {
                        // Batalkan reply tanpa keluar dari room
                    } else {
                        leave_room(app, out_tx, ev_rx);
                    }
                }
                KeyCode::Backspace => { app.input.pop(); }
                KeyCode::Enter => send_message(app, out_tx),
                // Scroll chat: PageUp / PageDown
                KeyCode::PageUp => {
                    app.chat_scroll = app.chat_scroll.saturating_add(SCROLL_STEP);
                }
                KeyCode::PageDown => {
                    app.chat_scroll = app.chat_scroll.saturating_sub(SCROLL_STEP);
                }
                // Reply: 'r' saat input kosong → kutip pesan terakhir dari peer
                KeyCode::Char('r') if app.input.is_empty() && app.replying_to.is_none() => {
                    let last_peer = app.messages.iter().rev()
                        .find(|m| matches!(m.who, super::types::Who::Peer))
                        .map(|m| m.text.clone());
                    if let Some(raw) = last_peer {
                        // Jika pesan itu sendiri sudah reply, kutip bagian actual-nya saja
                        let quote = if let Some((_q, actual)) = parse_reply_text(&raw) {
                            actual.to_string()
                        } else {
                            raw
                        };
                        let truncated = truncate_quote(&quote);
                        app.replying_to = Some(truncated);
                    } else {
                        app.set_notif_info("[!] Belum ada pesan untuk dibalas.");
                    }
                }
                KeyCode::Char(c) => {
                    // Batas 512 karakter untuk pesan chat
                    if app.input.len() < MAX_INPUT_LEN {
                        app.input.push(c);
                    }
                }
                _ => {}
            }
        }

        Mode::SendFile => handle_send_file_key(app, key),
    }
    false
}

fn start_connection(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<SessionCmd>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) {
    if app.contacts.is_empty() {
        app.set_notif_info("Belum ada kontak. Tekan 'a' untuk menambah.");
        return;
    }
    let keys = match &app.keys {
        Some(k) => k,
        None => return,
    };
    let contact = app.contacts[app.selected].clone();

    if let Some(h) = app.conn_task.take() {
        h.abort();
    }

    let (o_tx, o_rx) = mpsc::unbounded_channel::<SessionCmd>();
    let (e_tx, e_rx) = mpsc::unbounded_channel::<SessionEvent>();
    *out_tx = Some(o_tx);
    *ev_rx = Some(e_rx);

    app.mode = Mode::InRoom;
    app.room = RoomState::Connecting;
    app.peer_name = Some(contact.nickname.clone());
    app.messages.clear();

    let my_fp = keys.fingerprint.clone();
    let target_fp = contacts::fingerprint(&contact.ed25519_pub);
    let local_sk = keys.noise_sk;
    let peer_pk = contact.noise_pub;
    let onion = contact.onion.clone();
    let lan: LanMode = app.connect_kind.into();
    let tor = app.tor.clone();
    // SEC-13: sertakan our_tor_auth_secret untuk inject ke arti keystore saat dial.
    let our_auth_secret = keys.tor_client_auth_secret;

    let handle = tokio::spawn(async move {
        let _ = e_tx.send(SessionEvent::StateChanged(SessionState::Connecting));
        match transport::establish(
            &my_fp,
            &target_fp,
            lan,
            onion.as_deref(),
            tor.as_ref(),
            Some(our_auth_secret),
        )
        .await
        {
            Ok((conn, role)) => {
                let _ =
                    session::run_session(conn, role, local_sk, Some(peer_pk), o_rx, e_tx).await;
            }
            Err(err) => {
                let _ = e_tx.send(SessionEvent::Error(err.to_string()));
            }
        }
    });
    app.conn_task = Some(handle);
}

fn add_contact_from_buffer(app: &mut App) {
    let line = app.add_buffer.trim().to_string();
    let mut parts = line.splitn(2, char::is_whitespace);
    let code = parts.next().unwrap_or("");
    let nick = parts.next().unwrap_or("").trim();

    match contacts::decode_invite(code) {
        Ok(inv) => {
            if let Some(keys) = &app.keys {
                if inv.ed25519_pub == keys.ed25519_pub {
                    app.set_notif_error("[!] Tidak bisa menambah diri sendiri sebagai kontak.");
                    return;
                }
            }
            let nickname = if nick.is_empty() {
                format!("peer-{}", &contacts::fingerprint(&inv.ed25519_pub)[..8])
            } else {
                nick.to_string()
            };
            let via = match (&inv.onion, &inv.client_auth_pub) {
                (Some(_), Some(_)) => "LAN+Tor (restricted discovery)",
                (Some(_), None) => "LAN+Tor",
                _ => "LAN",
            };
            let has_cap = inv.client_auth_pub.is_some();
            app.contacts.insert(
                0,
                crate::contacts::Contact {
                    nickname: nickname.clone(),
                    ed25519_pub: inv.ed25519_pub,
                    noise_pub: inv.noise_pub,
                    onion: inv.onion,
                    tor_client_auth_pub: inv.client_auth_pub,
                },
            );
            app.selected = 0;
            app.mode = Mode::Browsing;
            app.add_buffer.clear();
            persist_contacts(app);

            // SEC-13: bila kontak v2 (punya cap), restart service dengan semua kunci.
            if has_cap {
                trigger_tor_restart(app);
            }

            app.set_notif_success(format!("[✓] Kontak '{nickname}' ditambahkan ({via})."));
        }
        Err(_) => {
            app.set_notif_error(
                "[!] Invite code tidak valid. (Format v1 tidak lagi didukung — minta invite baru.)",
            );
        }
    }
}

fn delete_contact(app: &mut App, idx: usize) {
    app.pending_delete = None;
    if idx >= app.contacts.len() {
        return;
    }
    let removed = app.contacts.remove(idx);
    if app.selected >= app.contacts.len() {
        app.selected = app.contacts.len().saturating_sub(1);
    }
    persist_contacts(app);
    app.set_notif_info(format!("Kontak '{}' dihapus.", removed.nickname));
}

fn send_message(app: &mut App, out_tx: &Option<mpsc::UnboundedSender<SessionCmd>>) {
    if app.room != RoomState::Open {
        return;
    }
    let text = std::mem::take(&mut app.input);
    if text.is_empty() {
        return;
    }
    // Sertakan kutipan reply jika ada
    let full_text = if let Some(quote) = app.replying_to.take() {
        format!("↩ \"{quote}\"\n{text}")
    } else {
        text
    };
    app.chat_scroll = 0; // scroll ke bawah setelah kirim
    if let Some(tx) = out_tx {
        if tx.send(SessionCmd::Text(full_text.clone())).is_ok() {
            app.messages.push(ChatLine::me(full_text));
        }
    }
}

fn leave_room(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<SessionCmd>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) {
    if let Some(h) = app.conn_task.take() {
        h.abort();
    }
    *out_tx = None;
    *ev_rx = None;
    app.mode = Mode::Browsing;
    app.room = RoomState::None;
    app.peer_name = None;
    app.input.clear();
    app.messages.clear();
    app.peer_ft_capable = false;
    app.file_transfer = FileTransferUiState::None;
    app.send_file_buffer.clear();
    app.chat_scroll = 0;
    app.replying_to = None;
    app.set_notif_info("Keluar dari sesi. Riwayat dibuang.");
}

fn handle_send_file_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::InRoom;
            app.send_file_buffer.clear();
        }
        KeyCode::Backspace => { app.send_file_buffer.pop(); }
        KeyCode::Enter => {
            let raw = app.send_file_buffer.trim().to_string();
            if raw.is_empty() { return; }
            match prepare_file_header(&raw) {
                Ok((header, data)) => {
                    let name = header.name.clone();
                    let total_bytes = header.total_bytes;
                    app.file_transfer = FileTransferUiState::Prompting(header, data);
                    app.mode = Mode::InRoom;
                    app.send_file_buffer.clear();
                    if total_bytes > INLINE_VIEW_LIMIT {
                        let mb = total_bytes / 1_048_576;
                        app.set_notif_warn(format!(
                            "↑ '{name}' ({mb} MB) — peer hanya bisa simpan, tidak bisa lihat inline."
                        ));
                    } else {
                        app.set_notif_info(format!("↑ '{name}' siap dikirim."));
                    }
                }
                Err(msg) => app.set_notif_error(msg),
            }
        }
        KeyCode::Char(c) => app.send_file_buffer.push(c),
        _ => {}
    }
}

/// Handler key [S/L/T/Esc] untuk modal "file diterima".
/// Dipanggil saat `app.file_transfer == Received` — intercept semua input.
fn handle_received_file_key(app: &mut App, key: KeyEvent) -> bool {
    let is_image = match &app.file_transfer {
        FileTransferUiState::Received { is_image, .. } => *is_image,
        _ => return false,
    };

    let handled = match key.code {
        KeyCode::Char('s') | KeyCode::Char('S') => true,
        KeyCode::Char('l') | KeyCode::Char('L') if is_image => true,
        KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Esc => true,
        _ => false,
    };

    if !handled {
        return false;
    }

    let ft = std::mem::replace(&mut app.file_transfer, FileTransferUiState::None);
    if let FileTransferUiState::Received { name, mut data, .. } = ft {
        match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => {
                match save_received_file(&name, &data) {
                    Ok(path) => {
                        data.zeroize();
                        app.messages.push(ChatLine::system(format!("↓ [✓] '{name}' disimpan.")));
                        app.set_notif_success(format!("↓ Disimpan: {}", path.display()));
                    }
                    Err(e) => {
                        data.zeroize();
                        app.messages.push(ChatLine::system(format!("↓ Gagal simpan '{name}'.")));
                        app.set_notif_error(format!("[!] Gagal simpan: {e}"));
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Serahkan ke event loop utama (mod.rs) untuk render via viuer
                app.pending_image_render = Some(data);
                app.messages.push(ChatLine::system(format!("↓ Menampilkan '{name}' — tekan sembarang tombol untuk kembali.")));
            }
            _ => {
                data.zeroize();
                app.messages.push(ChatLine::system(format!("↓ Transfer '{name}' ditolak.")));
                app.set_notif_info("Transfer ditolak — data dihapus dari memori.");
            }
        }
    }
    false
}

// Batas ukuran file pengirim: 4 GB hard limit (PRD edge case), 10 MB threshold untuk opsi [L].
const MAX_SEND_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const INLINE_VIEW_LIMIT: u64 = 10 * 1024 * 1024;

// UX: batas panjang input chat dan jumlah pesan per scroll step.
const MAX_INPUT_LEN: usize = 512;
const SCROLL_STEP: usize = 5;

/// Parse format reply: `↩ "quote"\nactual`. Mengembalikan (quote, actual) jika cocok.
pub(super) fn parse_reply_text(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("↩ \"")?;
    let (quote, tail) = rest.split_once("\"\n")?;
    Some((quote, tail))
}

/// Potong teks kutipan ke maks 60 karakter + ellipsis jika lebih panjang.
fn truncate_quote(text: &str) -> String {
    let text = text.lines().next().unwrap_or(text); // kutip hanya baris pertama
    if text.chars().count() > 60 {
        let truncated: String = text.chars().take(60).collect();
        format!("{truncated}…")
    } else {
        text.to_string()
    }
}

/// Baca file sekali, hitung semua metadata, kembalikan header + bytes asli.
/// Data di-return bersama header untuk mencegah TOCTOU: SHA-256 dan bytes yang
/// dikirim selalu dari pembacaan yang sama.
fn prepare_file_header(raw_path: &str) -> Result<(crate::session::file_transfer::FileHeader, Vec<u8>), String> {
    use sha2::{Digest, Sha256};

    let path = std::path::Path::new(raw_path);
    if !path.exists() {
        return Err(format!("[!] File tidak ditemukan: {raw_path}"));
    }

    let meta = std::fs::metadata(path)
        .map_err(|e| format!("[!] Tidak bisa baca metadata: {e}"))?;
    let total_bytes = meta.len();
    if total_bytes == 0 {
        return Err("[!] File kosong (0 byte) tidak bisa dikirim.".into());
    }
    if total_bytes > MAX_SEND_BYTES {
        let gb = total_bytes / 1_073_741_824;
        return Err(format!("[!] File terlalu besar: {gb} GB (maks 4 GB untuk FT-01)."));
    }

    let data = std::fs::read(path)
        .map_err(|e| format!("[!] Gagal membaca file: {e}"))?;

    let mime = infer::get(&data)
        .map(|k| k.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let sha256 = hex::encode(Sha256::digest(&data));
    let chunk_count = data.len().div_ceil(CHUNK_SIZE) as u32;

    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(raw_path)
        .to_string();

    let header = crate::session::file_transfer::FileHeader { name, mime, total_bytes, chunk_count, sha256 };
    Ok((header, data))
}

pub(super) fn handle_session_event(app: &mut App, se: SessionEvent) {
    match se {
        SessionEvent::StateChanged(state) => match state {
            SessionState::Connecting => { app.room = RoomState::Connecting; }
            SessionState::Handshaking => { app.room = RoomState::Handshaking; }
            SessionState::Active => {
                app.room = RoomState::Open;
                app.messages.push(ChatLine::system("Sesi aman terbuka.".into()));
            }
            SessionState::Closed => {
                if app.room == RoomState::Open {
                    app.room = RoomState::Closed;
                }
                app.file_transfer = FileTransferUiState::None;
                app.send_file_buffer.clear();
                app.peer_ft_capable = false;
                app.replying_to = None;
                app.chat_scroll = 0;
            }
        },
        SessionEvent::Message(text) => app.messages.push(ChatLine::peer(text)),
        SessionEvent::PeerLeft => {
            app.room = RoomState::PeerLeft;
            app.file_transfer = FileTransferUiState::None;
            app.send_file_buffer.clear();
            app.peer_ft_capable = false;
            app.replying_to = None;
            app.chat_scroll = 0;
            app.messages.push(ChatLine::system("Peer keluar dari sesi.".into()));
        }
        SessionEvent::Error(e) => {
            app.room = RoomState::Closed;
            app.file_transfer = FileTransferUiState::None;
            app.send_file_buffer.clear();
            app.peer_ft_capable = false;
            app.replying_to = None;
            app.chat_scroll = 0;
            app.set_notif_error(format!("Koneksi gagal: {e}"));
            app.messages.push(ChatLine::system(format!("Error: {e}")));
        }
        SessionEvent::PeerCapable { file_transfer } => {
            app.peer_ft_capable = file_transfer;
            if file_transfer {
                app.messages.push(ChatLine::system(
                    "Peer mendukung file transfer. [Ctrl+F] untuk kirim.".into(),
                ));
            }
        }
        SessionEvent::FileProgress { name, received, total } => {
            if received == 0 {
                app.messages.push(ChatLine::system(format!("↓ Menerima '{name}'…")));
            }
            app.file_transfer = FileTransferUiState::Receiving { name, total, received };
        }
        SessionEvent::FileReceived { name, data } => {
            // Tidak auto-save: tunjukkan prompt [S/L/T] ke user terlebih dahulu.
            let mime = infer::get(&data)
                .map(|k| k.mime_type())
                .unwrap_or("application/octet-stream");
            let is_image = mime.starts_with("image/") && data.len() <= 10 * 1024 * 1024;
            app.messages.push(ChatLine::system(format!(
                "↓ File diterima: '{name}' — [S] Simpan  {}[T] Tolak",
                if is_image { "[L] Lihat  " } else { "" }
            )));
            app.file_transfer = FileTransferUiState::Received { name, data, is_image };
        }
        SessionEvent::FileError(e) => {
            app.file_transfer = FileTransferUiState::None;
            app.set_notif_error(format!("[!] Transfer gagal: {e}"));
        }
        SessionEvent::FileSent { name } => {
            app.file_transfer = FileTransferUiState::None;
            app.messages.push(ChatLine::system(format!("↑ File terkirim: '{name}'")));
        }
    }
}

fn get_vault_exports_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .map_err(|_| "Tidak bisa menentukan direktori home (USERPROFILE/HOME tidak tersedia).".to_string())?;
    Ok(home.join("Documents").join("vault-exports"))
}

fn save_received_file(name: &str, data: &[u8]) -> Result<std::path::PathBuf, String> {
    // Sanitasi: cegah path traversal + karakter kontrol/NUL dari peer tidak terpercaya.
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("received_file");
    let safe_name: String = base
        .chars()
        .filter(|c| !c.is_control() && *c != '\0')
        .take(200)
        .collect();
    let safe_name = if safe_name.is_empty() { "received_file".to_string() } else { safe_name };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let dir = get_vault_exports_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Gagal buat direktori: {e}"))?;
    let path = dir.join(format!("{ts}_{safe_name}"));
    std::fs::write(&path, data)
        .map_err(|e| format!("Gagal tulis file: {e}"))?;
    Ok(path)
}
