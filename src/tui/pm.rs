//! Password Manager key handlers dan helper functions.

use crossterm::event::{KeyCode, KeyEvent};
use zeroize::Zeroize;

use crate::identity::pm::{BackupCode, PM_CODES_MAX};
use crate::identity::vault;

use super::app::App;
use super::types::Screen;

pub(super) fn handle_pm_main_key(app: &mut App, key: KeyEvent) -> bool {
    // ── Modal backup codes aktif — routing ke handler tersendiri ────────────
    if app.pm_codes_open {
        return handle_pm_codes_key(app, key);
    }

    // Mode pencarian aktif
    if app.pm_search_active {
        match key.code {
            KeyCode::Esc => {
                app.pm_search_active = false;
                app.pm_search.clear();
                app.pm_selected = 0;
            }
            KeyCode::Backspace => { app.pm_search.pop(); app.pm_selected = 0; }
            KeyCode::Char(c) => { app.pm_search.push(c); app.pm_selected = 0; }
            _ => {}
        }
        return false;
    }

    // Konfirmasi hapus
    if let Some(idx) = app.pm_pending_delete {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => pm_delete_entry(app, idx),
            _ => {
                app.pm_pending_delete = None;
                app.set_notif_info("Hapus dibatalkan.");
            }
        }
        return false;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('/') => {
            app.pm_search_active = true;
            app.pm_search.clear();
        }
        KeyCode::Char('n') if !app.pm_is_readonly => {
            app.pm_add_service.clear();
            app.pm_add_username.clear();
            app.pm_add_password.clear();
            app.pm_add_field = 0;
            app.pm_add_codes.clear();
            app.pm_add_code_input.clear();
            app.auth_error = None;
            app.screen = Screen::PmAdd;
        }
        KeyCode::Char('n') => {
            app.set_notif_warn("Mode baca — passphrase tidak dikenal, tidak bisa tambah entri.");
        }
        KeyCode::Char('d') if !app.pm_is_readonly => {
            let visible = pm_visible_entries(app);
            if visible.is_empty() {
                app.set_notif_info("Tidak ada entri untuk dihapus.");
            } else {
                app.pm_pending_delete = Some(visible[app.pm_selected]);
            }
        }
        KeyCode::Char('d') => {
            app.set_notif_warn("Mode baca — tidak bisa hapus entri.");
        }
        KeyCode::Char('k') => {
            // Buka modal backup codes untuk entry yang dipilih
            let visible = pm_visible_entries(app);
            if !visible.is_empty() {
                app.pm_codes_open = true;
                app.pm_codes_selected = 0;
                app.pm_codes_add_mode = false;
                app.pm_codes_input.clear();
            } else {
                app.set_notif_info("Pilih entri terlebih dahulu.");
            }
        }
        KeyCode::Char('c') => {
            // Copy password ke clipboard
            let visible = pm_visible_entries(app);
            if !visible.is_empty() {
                let pass = app.pm_entries[visible[app.pm_selected]].password.clone();
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(pass)) {
                    Ok(()) => app.set_notif_success("[✓] Password disalin ke clipboard"),
                    Err(_) => app.set_notif_warn("Clipboard tidak tersedia"),
                }
            }
        }
        KeyCode::Char('u') => {
            // Copy username ke clipboard
            let visible = pm_visible_entries(app);
            if !visible.is_empty() {
                let uname = app.pm_entries[visible[app.pm_selected]].username.clone();
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(uname)) {
                    Ok(()) => app.set_notif_success("[✓] Username disalin ke clipboard"),
                    Err(_) => app.set_notif_warn("Clipboard tidak tersedia"),
                }
            }
        }
        KeyCode::Enter => {
            // Toggle reveal password — tampil/sembunyikan di detail panel
            let visible = pm_visible_entries(app);
            if !visible.is_empty() {
                if app.pm_reveal_tick.is_some() {
                    app.pm_reveal_tick = None;
                } else {
                    app.pm_reveal_tick = Some(app.tick_count);
                }
            }
        }
        KeyCode::Up => {
            if app.pm_selected > 0 {
                app.pm_selected -= 1;
            }
        }
        KeyCode::Down => {
            let visible_count = pm_visible_entries(app).len();
            if visible_count > 0 && app.pm_selected + 1 < visible_count {
                app.pm_selected += 1;
            }
        }
        _ => {}
    }
    false
}

fn handle_pm_codes_key(app: &mut App, key: KeyEvent) -> bool {
    let visible = pm_visible_entries(app);
    if visible.is_empty() {
        app.pm_codes_open = false;
        return false;
    }
    let entry_idx = visible[app.pm_selected];

    // ── Mode input tambah code baru ──────────────────────────────────────
    if app.pm_codes_add_mode {
        match key.code {
            KeyCode::Esc => {
                app.pm_codes_add_mode = false;
                app.pm_codes_input.clear();
            }
            KeyCode::Enter => {
                let raw = app.pm_codes_input.trim().to_string();
                if raw.is_empty() {
                    app.pm_codes_add_mode = false;
                } else {
                    let codes = app.pm_entries[entry_idx]
                        .codes
                        .get_or_insert_with(Vec::new);
                    if codes.len() >= PM_CODES_MAX {
                        app.set_notif_warn(format!("Maksimum {} codes per entri.", PM_CODES_MAX));
                    } else {
                        codes.push(BackupCode { code: raw, used: false });
                        pm_save(app);
                        let count = app.pm_entries[entry_idx]
                            .codes.as_ref().map(|c| c.len()).unwrap_or(0);
                        app.set_notif_success(format!("[✓] Code ditambahkan ({}/{})", count, PM_CODES_MAX));
                    }
                    app.pm_codes_input.clear();
                    app.pm_codes_add_mode = false;
                }
            }
            KeyCode::Backspace => { app.pm_codes_input.pop(); }
            KeyCode::Char(c) => app.pm_codes_input.push(c),
            _ => {}
        }
        return false;
    }

    // ── Navigasi & aksi di list codes ───────────────────────────────────
    let codes_len = app.pm_entries[entry_idx]
        .codes.as_ref().map(|c| c.len()).unwrap_or(0);

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pm_codes_open = false;
            app.pm_codes_input.clear();
        }
        KeyCode::Up => {
            if app.pm_codes_selected > 0 {
                app.pm_codes_selected -= 1;
            }
        }
        KeyCode::Down => {
            if codes_len > 0 && app.pm_codes_selected + 1 < codes_len {
                app.pm_codes_selected += 1;
            }
        }
        KeyCode::Char('n') if !app.pm_is_readonly => {
            if codes_len >= PM_CODES_MAX {
                app.set_notif_warn(format!("Maksimum {} codes per entri.", PM_CODES_MAX));
            } else {
                app.pm_codes_add_mode = true;
                app.pm_codes_input.clear();
            }
        }
        KeyCode::Char('m') if !app.pm_is_readonly && codes_len > 0 => {
            // Toggle used/unused pada code yang dipilih
            let sel = app.pm_codes_selected;
            if let Some(codes) = app.pm_entries[entry_idx].codes.as_mut() {
                if sel < codes.len() {
                    codes[sel].used = !codes[sel].used;
                    let status = if codes[sel].used { "used" } else { "aktif" };
                    pm_save(app);
                    app.set_notif_success(format!("[✓] Code ditandai {status}."));
                }
            }
        }
        KeyCode::Char('y') if codes_len > 0 => {
            // Copy code yang dipilih ke clipboard
            let sel = app.pm_codes_selected;
            let code_text = app.pm_entries[entry_idx]
                .codes.as_ref()
                .and_then(|c| c.get(sel))
                .map(|bc| bc.code.clone());
            if let Some(text) = code_text {
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                    Ok(()) => app.set_notif_success("[✓] Code disalin ke clipboard"),
                    Err(_) => app.set_notif_warn("Clipboard tidak tersedia"),
                }
            }
        }
        KeyCode::Char('d') if !app.pm_is_readonly && codes_len > 0 => {
            // Hapus code yang dipilih
            let sel = app.pm_codes_selected;
            if let Some(codes) = app.pm_entries[entry_idx].codes.as_mut() {
                if sel < codes.len() {
                    codes.remove(sel);
                    if codes.is_empty() {
                        app.pm_entries[entry_idx].codes = None;
                    }
                    pm_save(app);
                    app.set_notif_success("[✓] Code dihapus.");
                }
            }
            if app.pm_codes_selected > 0
                && app.pm_codes_selected >= app.pm_entries[entry_idx]
                    .codes.as_ref().map(|c| c.len()).unwrap_or(0)
            {
                app.pm_codes_selected -= 1;
            }
        }
        _ => {}
    }
    false
}

pub(super) fn handle_pm_add_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.pm_add_service.clear();
            app.pm_add_username.clear();
            app.pm_add_password.zeroize();
            app.pm_add_codes.clear();
            app.pm_add_code_input.clear();
            app.pm_add_field = 0;
            app.screen = Screen::PmMain;
        }
        KeyCode::Tab | KeyCode::Down => {
            if app.pm_add_field < 3 {
                app.pm_add_field += 1;
            }
        }
        KeyCode::Up => {
            if app.pm_add_field > 0 {
                app.pm_add_field -= 1;
            }
        }
        KeyCode::Backspace => match app.pm_add_field {
            0 => { app.pm_add_service.pop(); }
            1 => { app.pm_add_username.pop(); }
            2 => { app.pm_add_password.pop(); }
            _ => { app.pm_add_code_input.pop(); }
        },
        KeyCode::Char(c) => match app.pm_add_field {
            0 => app.pm_add_service.push(c),
            1 => app.pm_add_username.push(c),
            2 => app.pm_add_password.push(c),
            _ => app.pm_add_code_input.push(c),
        },
        KeyCode::Enter => {
            if app.pm_add_field < 2 {
                app.pm_add_field += 1;
            } else if app.pm_add_field == 2 {
                // Dari password → lanjut ke step codes (opsional)
                app.pm_add_field = 3;
            } else {
                // Step 4: backup codes
                let raw = app.pm_add_code_input.trim().to_string();
                if !raw.is_empty() && app.pm_add_codes.len() < PM_CODES_MAX {
                    // Enter dengan isi → tambah code ke list, tunggu code berikutnya
                    app.pm_add_codes.push(raw);
                    app.pm_add_code_input.clear();
                } else {
                    // Enter kosong → simpan entri
                    if app.pm_add_service.trim().is_empty() {
                        app.pm_add_field = 0;
                        app.set_notif_error("[!] Service tidak boleh kosong.");
                    } else {
                        let new_id = app.pm_entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                        let codes: Option<Vec<BackupCode>> = if app.pm_add_codes.is_empty() {
                            None
                        } else {
                            Some(app.pm_add_codes.iter().map(|c| BackupCode {
                                code: c.clone(), used: false,
                            }).collect())
                        };
                        let codes_count = codes.as_ref().map(|c| c.len()).unwrap_or(0);
                        let entry = crate::identity::pm::PmEntry {
                            id: new_id,
                            service: app.pm_add_service.trim().to_string(),
                            username: app.pm_add_username.trim().to_string(),
                            password: app.pm_add_password.clone(),
                            codes,
                        };
                        let svc_name = app.pm_add_service.trim().to_string();
                        app.pm_entries.push(entry);
                        app.pm_add_password.zeroize();
                        app.pm_add_service.clear();
                        app.pm_add_username.clear();
                        app.pm_add_codes.clear();
                        app.pm_add_code_input.clear();
                        app.pm_add_field = 0;
                        pm_save(app);
                        let notif = if codes_count > 0 {
                            format!("[✓] Entri '{}' + {} backup codes ditambahkan.", svc_name, codes_count)
                        } else {
                            format!("[✓] Entri '{}' ditambahkan.", svc_name)
                        };
                        app.set_notif_success(notif);
                        app.screen = Screen::PmMain;
                    }
                }
            }
        }
        _ => {}
    }
    false
}

/// Indeks entries yang terlihat (setelah filter pencarian).
pub(crate) fn pm_visible_entries(app: &App) -> Vec<usize> {
    let q = app.pm_search.to_lowercase();
    app.pm_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            q.is_empty() || e.service.to_lowercase().contains(&q)
                || e.username.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

fn pm_delete_entry(app: &mut App, idx: usize) {
    app.pm_pending_delete = None;
    if idx >= app.pm_entries.len() {
        return;
    }
    let svc = app.pm_entries[idx].service.clone();
    app.pm_entries.remove(idx);
    // Sesuaikan pm_selected
    let visible_count = pm_visible_entries(app).len();
    if app.pm_selected >= visible_count && app.pm_selected > 0 {
        app.pm_selected -= 1;
    }
    pm_save(app);
    app.set_notif_info(format!("Entri '{}' dihapus.", svc));
}

pub(crate) fn pm_save(app: &mut App) {
    let (vault_bytes, pm_key) = match (&app.pm_vault_bytes, &app.pm_key) {
        (Some(v), Some(k)) => (*v.clone(), *k.clone()),
        _ => {
            app.set_notif_error("[!] Tidak bisa simpan — state PM tidak valid.");
            return;
        }
    };

    match vault::update_pm(&vault_bytes, &pm_key, &app.pm_entries) {
        Ok(new_vault) => {
            if vault::write_vault_v2(&app.vault_path, &new_vault).is_ok() {
                app.pm_vault_bytes = Some(Box::new(new_vault));
                app.set_notif_success("[✓] Tersimpan.");
            } else {
                app.set_notif_error("[!] Gagal menulis vault ke disk.");
            }
        }
        Err(_) => app.set_notif_error("[!] Gagal enkripsi PM entries."),
    }
}
