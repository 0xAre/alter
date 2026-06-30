//! TUI layer (ratatui + crossterm).
//!
//! Alur layar:
//!   Splash  →  Unlock / Create  →  Init  →  Main (kontak + room + chat)

pub(crate) mod app;
pub(crate) mod auth;
pub(crate) mod chat;
pub(crate) mod contact;
pub(crate) mod pm;
pub(crate) mod types;
mod ui;

// Re-export untuk main.rs
pub use app::{App, ConnectKind, build_self_keys};

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use zeroize::Zeroize;

use crate::contacts::Contact;
use crate::error::Error;
use crate::session::SessionEvent;
use crate::transport::tor::TorContext;

use app::refresh_invite;
use auth::{apply_unlock_result, handle_create_key, handle_init_key, handle_migrate_key, handle_unlock_key};
use chat::{handle_main_key, handle_session_event};
use contact::{inject_all_client_auth_keys, trigger_tor_restart};
use pm::{handle_pm_add_key, handle_pm_main_key};
use types::{Notification, NotifLevel, Screen, UnlockComputed};

const SPLASH_TICKS: u64 = 12;

pub async fn run(
    vault_path: PathBuf,
    vault_exists: bool,
    connect_kind: ConnectKind,
    contacts: Vec<Contact>,
    mut tor_rx: Option<mpsc::UnboundedReceiver<Result<Arc<TorContext>, String>>>,
) -> Result<(), Error> {
    // Channel untuk menerima hasil restart service dari background task (SEC-13).
    let (restart_tx, mut restart_rx) =
        mpsc::unbounded_channel::<Result<Arc<TorContext>, String>>();

    let mut app = App::new(vault_path, vault_exists, connect_kind, contacts, restart_tx);
    app.screen = Screen::Splash;

    if tor_rx.is_some() {
        app.tor_connecting = true;
        app.set_notif_info("Menyambung ke Tor di latar belakang (~30-60 dtk)…");
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<KeyEvent>();
    spawn_input_thread(input_tx);

    let mut out_tx: Option<mpsc::UnboundedSender<String>> = None;
    let mut ev_rx: Option<mpsc::UnboundedReceiver<SessionEvent>> = None;

    let mut tick = tokio::time::interval(Duration::from_millis(100));

    let result = loop {
        if let Err(e) = terminal.draw(|f| ui::render(f, &app)) {
            break Err(Error::from(e));
        }

        tokio::select! {
            maybe_key = input_rx.recv() => {
                match maybe_key {
                    Some(key) => {
                        if handle_key(&mut app, &mut out_tx, &mut ev_rx, key) {
                            break Ok(());
                        }
                    }
                    None => break Ok(()),
                }
            }
            maybe_ev = recv_session(&mut ev_rx) => {
                match maybe_ev {
                    Some(se) => handle_session_event(&mut app, se),
                    None => {
                        ev_rx = None;
                        out_tx = None;
                    }
                }
            }
            maybe_tor = recv_tor(&mut tor_rx) => {
                match maybe_tor {
                    Some(Ok(ctx)) => {
                        app.tor = Some(ctx);
                        refresh_invite(&mut app);
                        app.set_notif_success("Tor siap — sekarang online (LAN + Tor).");
                        // SEC-13: inject auth keys + restart service bila kontak sudah ada.
                        inject_all_client_auth_keys(&app);
                        let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
                        if has_restricted {
                            trigger_tor_restart(&mut app);
                        }
                    }
                    Some(Err(e)) => {
                        app.set_notif_warn(format!("Tor gagal: {e}. Jalan mode LAN saja."));
                    }
                    None => {}
                }
                app.tor_connecting = false;
                tor_rx = None;
            }
            maybe_restart = restart_rx.recv() => {
                if let Some(res) = maybe_restart {
                    match res {
                        Ok(new_ctx) => {
                            app.tor = Some(new_ctx);
                            refresh_invite(&mut app);
                            app.tor_restarting = false;
                            app.set_notif_success("Restricted discovery diaktifkan.");
                        }
                        Err(e) => {
                            app.tor_restarting = false;
                            app.set_notif_warn(
                                format!("Tor service restart gagal: {e}. Restricted discovery tidak aktif.")
                            );
                        }
                    }
                }
            }
            maybe_unlock = recv_unlock(&mut app.unlock_rx) => {
                if let Some(computed) = maybe_unlock {
                    apply_unlock_result(&mut app, computed);
                }
            }
            _ = tick.tick() => {
                app.tick_count += 1;

                if app.screen == Screen::Splash {
                    app.splash_ticks += 1;
                    if app.splash_ticks >= SPLASH_TICKS {
                        let vault_exists = app.vault_path.exists();
                        app.screen = if vault_exists { Screen::Unlock } else { Screen::Create };
                    }
                }

                // M6: auto-hide password setelah 5 detik (50 tick × 100ms)
                if let Some(reveal_tick) = app.pm_reveal_tick {
                    if app.tick_count.saturating_sub(reveal_tick) >= 50 {
                        app.pm_reveal_tick = None;
                    }
                }

                if app.screen == Screen::Init {
                    let elapsed = app.tick_count.saturating_sub(app.init_start_tick);
                    app.init_step = match elapsed {
                        0..=2  => 1,
                        3..=5  => 2,
                        6..=8  => 3,
                        _      => 4,
                    };
                    if elapsed >= 14 {
                        if app.show_onboard_after_init {
                            app.screen = Screen::Onboard;
                        } else {
                            app.screen = Screen::Main;
                        }
                    }
                }

                if let Some(n) = &app.notification {
                    if let Some(dismiss_at) = n.dismiss_at {
                        if app.tick_count >= dismiss_at {
                            app.notification = None;
                        }
                    }
                }

                if let Some(armed_tick) = app.panic_armed_tick {
                    if app.tick_count.saturating_sub(armed_tick) > 30 {
                        app.panic_armed_tick = None;
                        if matches!(&app.notification, Some(n) if n.text.contains("PANIC")) {
                            app.notification = None;
                        }
                    }
                }
            }
        }
    };

    if app.should_panic_wipe {
        if let Some(h) = app.conn_task.take() {
            h.abort();
        }
        app.tor = None;
        app.keys = None;
        app.pass_input.zeroize();
        app.pass_confirm.zeroize();
        app.pm_pass_input.zeroize();
        app.pm_pass_confirm.zeroize();
        app.messages.clear();
        app.input.clear();
        app.add_buffer.clear();
        app.rename_buffer.clear();
        // M6: wipe PM data sensitif
        for entry in &mut app.pm_entries {
            entry.password.zeroize();
            entry.username.zeroize();
            entry.service.zeroize();
        }
        app.pm_entries.clear();
        app.pm_key = None;
        app.pm_vault_bytes = None;
        app.pm_add_password.zeroize();
        app.pm_add_service.clear();
        app.pm_add_username.clear();
        app.migration_bundle = None;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn recv_session(
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
) -> Option<SessionEvent> {
    match ev_rx.as_mut() {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn recv_unlock(
    rx: &mut Option<mpsc::UnboundedReceiver<UnlockComputed>>,
) -> Option<UnlockComputed> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

async fn recv_tor(
    rx: &mut Option<mpsc::UnboundedReceiver<Result<Arc<TorContext>, String>>>,
) -> Option<Result<Arc<TorContext>, String>> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

fn spawn_input_thread(tx: mpsc::UnboundedSender<KeyEvent>) {
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                if tx.send(k).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });
}

fn handle_panic_hotkey(app: &mut App) -> bool {
    if let Some(armed_tick) = app.panic_armed_tick {
        if app.tick_count.saturating_sub(armed_tick) <= 30 {
            app.should_panic_wipe = true;
            return true;
        }
    }
    app.panic_armed_tick = Some(app.tick_count);
    app.notification = Some(Notification {
        level: NotifLevel::Error,
        text: "⚠ PANIC — tekan Ctrl+Shift+X lagi dalam 3 detik untuk wipe & exit.".into(),
        dismiss_at: None,
    });
    false
}

fn handle_key(
    app: &mut App,
    out_tx: &mut Option<mpsc::UnboundedSender<String>>,
    ev_rx: &mut Option<mpsc::UnboundedReceiver<SessionEvent>>,
    key: KeyEvent,
) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('X') {
        return handle_panic_hotkey(app);
    }

    match app.screen {
        Screen::Splash => {
            let vault_exists = app.vault_path.exists();
            app.screen = if vault_exists { Screen::Unlock } else { Screen::Create };
            false
        }
        Screen::Unlock => handle_unlock_key(app, key),
        Screen::Unlocking => false, // abaikan semua input saat KDF berjalan di background
        Screen::Create => handle_create_key(app, key),
        Screen::Init => handle_init_key(app, key),
        Screen::Onboard => {
            app.screen = Screen::Main;
            false
        }
        Screen::Main => handle_main_key(app, out_tx, ev_rx, key),
        Screen::PmMain => handle_pm_main_key(app, key),
        Screen::PmAdd => handle_pm_add_key(app, key),
        Screen::Migrate => handle_migrate_key(app, key),
    }
}
