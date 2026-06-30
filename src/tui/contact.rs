//! Contact management: load, persist, copy invite, inject auth keys.

use std::path::{Path, PathBuf};
use crate::contacts;

use super::app::App;

pub(super) fn contacts_file_path(vault_path: &Path) -> PathBuf {
    let stem = vault_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "alter".to_string());
    vault_path.with_file_name(format!("{stem}-contacts"))
}

pub(super) fn load_contacts_into(app: &mut App) {
    let Some(key) = app.contacts_key else { return };
    let path = contacts_file_path(&app.vault_path);
    if let Ok(disk) = contacts::load_contacts(&path, &key) {
        let mut merged = disk;
        for c in std::mem::take(&mut app.contacts) {
            if !merged.iter().any(|d| d.ed25519_pub == c.ed25519_pub) {
                merged.insert(0, c);
            }
        }
        app.contacts = merged;
    }
    persist_contacts(app);
}

pub(super) fn persist_contacts(app: &mut App) {
    let Some(key) = app.contacts_key else { return };
    let path = contacts_file_path(&app.vault_path);
    if contacts::save_contacts(&path, &app.contacts, &key).is_err() {
        app.set_notif_warn("Peringatan: gagal menyimpan kontak ke disk.");
    }
}

pub(super) fn copy_invite(app: &mut App) {
    let invite = match &app.keys {
        Some(k) => k.invite.clone(),
        None => return,
    };
    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(invite)) {
        Ok(()) => app.set_notif_success("[✓] Identity disalin ke clipboard"),
        Err(_) => app.set_notif_warn("Clipboard tak tersedia — tekan 'i' untuk salin manual"),
    }
}

/// SEC-13: inject our client auth secret ke keystore arti untuk semua kontak
/// yang punya onion address. Fire-and-forget — tidak memblok.
pub(super) fn inject_all_client_auth_keys(app: &App) {
    let Some(tor) = app.tor.clone() else { return };
    let Some(keys) = &app.keys else { return };
    let our_secret = keys.tor_client_auth_secret;

    let targets: Vec<String> = app.contacts.iter().filter_map(|c| c.onion.clone()).collect();
    if targets.is_empty() {
        return;
    }

    tokio::spawn(async move {
        for onion in targets {
            let _ = tor.register_client_auth_key(&onion, our_secret).await;
        }
    });
}

/// SEC-13: kumpulkan semua tor_client_auth_pub dari kontak dan restart service.
///
/// Dipanggil saat: (1) kontak baru dengan cap ditambah, (2) vault baru dibuka
/// dengan kontak yang punya cap, (3) Tor baru siap dan sudah ada kontak cap.
pub(super) fn trigger_tor_restart(app: &mut App) {
    let Some(tor) = app.tor.clone() else { return };

    let auth_keys: Vec<[u8; 32]> = app
        .contacts
        .iter()
        .filter_map(|c| c.tor_client_auth_pub)
        .collect();

    app.tor_restarting = true;
    app.set_notif_info("Tor service restart untuk restricted discovery (~5s)…");

    let tx = app.tor_restart_result_tx.clone();
    tokio::spawn(async move {
        let result = tor.restart_with_authorized_keys(&auth_keys).await.map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
}
