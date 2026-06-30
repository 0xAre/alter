//! Main screen key handler, connection management, messaging, session events.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use crate::contacts;
use crate::session::{self, SessionEvent, SessionState};
use crate::transport::{self, LanMode};

use super::app::App;
use super::contact::{persist_contacts, copy_invite, trigger_tor_restart};
use super::types::{ChatLine, Mode, RoomState};

pub(super) fn handle_main_key(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
    key: KeyEvent,
) -> bool {
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
            KeyCode::Char(c) => app.add_buffer.push(c),
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
            KeyCode::Char(c) => app.rename_buffer.push(c),
            _ => {}
        },

        Mode::InRoom => match key.code {
            KeyCode::Esc => leave_room(app, out_tx, ev_rx),
            KeyCode::Backspace => { app.input.pop(); }
            KeyCode::Enter => send_message(app, out_tx),
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
    }
    false
}

fn start_connection(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
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

    let (o_tx, o_rx) = mpsc::unbounded_channel::<String>();
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

fn send_message(app: &mut App, out_tx: &Option<mpsc::UnboundedSender<String>>) {
    if app.room != RoomState::Open {
        return;
    }
    let text = std::mem::take(&mut app.input);
    if text.is_empty() {
        return;
    }
    if let Some(tx) = out_tx {
        if tx.send(text.clone()).is_ok() {
            app.messages.push(ChatLine::me(text));
        }
    }
}

fn leave_room(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
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
    app.set_notif_info("Keluar dari sesi. Riwayat dibuang.");
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
            }
        },
        SessionEvent::Message(text) => app.messages.push(ChatLine::peer(text)),
        SessionEvent::PeerLeft => {
            app.room = RoomState::PeerLeft;
            app.messages.push(ChatLine::system("Peer keluar dari sesi.".into()));
        }
        SessionEvent::Error(e) => {
            app.room = RoomState::Closed;
            app.set_notif_error(format!("Koneksi gagal: {e}"));
            app.messages.push(ChatLine::system(format!("Error: {e}")));
        }
    }
}
