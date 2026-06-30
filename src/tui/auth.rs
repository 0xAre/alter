//! Authentication handlers: unlock, create, migrate.

use crossterm::event::{KeyCode, KeyEvent};
use zeroize::Zeroize;

use crate::contacts;
use crate::identity::vault;
use crate::identity::keypair::KeyBundle;
use crate::platform;

use super::app::{App, build_self_keys, refresh_invite};
use super::contact::{inject_all_client_auth_keys, load_contacts_into, trigger_tor_restart};
use super::types::{CreatePhase, Screen, UnlockComputed};

pub(super) fn handle_unlock_key(app: &mut App, key: KeyEvent) -> bool {
    use tokio::sync::mpsc;
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter => {
            match vault::read_vault_raw(&app.vault_path) {
                Err(_) => {
                    app.auth_error = Some("Vault tidak terbaca.".into());
                }
                Ok(bytes) => {
                    // Salin passphrase ke Vec agar bisa dipindah ke spawn_blocking
                    let passphrase = zeroize::Zeroizing::new(app.pass_input.as_bytes().to_vec());
                    let (tx, rx) = mpsc::unbounded_channel::<UnlockComputed>();
                    app.unlock_rx = Some(rx);
                    app.auth_error = None;
                    app.screen = Screen::Unlocking;
                    tokio::task::spawn_blocking(move || {
                        let computed = compute_unlock(bytes, &passphrase);
                        let _ = tx.send(computed);
                    });
                }
            }
        }
        KeyCode::Backspace => { app.pass_input.pop(); }
        KeyCode::Char(c) => app.pass_input.push(c),
        _ => {}
    }
    false
}

/// Komputasi berat (KDF + dekripsi) — dipanggil dari spawn_blocking, tidak menyentuh App.
pub(super) fn compute_unlock(bytes: Vec<u8>, passphrase: &[u8]) -> UnlockComputed {
    match vault::detect_version(&bytes) {
        vault::VaultVersion::V1 => {
            let v1: [u8; vault::VAULT_SIZE] = match bytes.try_into() {
                Ok(v) => v,
                Err(_) => return UnlockComputed::Err("Vault tidak valid.".into()),
            };
            match vault::unseal(&v1, passphrase) {
                Ok(bundle) => UnlockComputed::NeedsMigration(bundle),
                Err(_) => UnlockComputed::Err("Passphrase salah atau vault rusak.".into()),
            }
        }
        vault::VaultVersion::V2 => {
            let v2: [u8; vault::VAULT_V2_SIZE] = match bytes.try_into() {
                Ok(v) => v,
                Err(_) => return UnlockComputed::Err("Vault tidak valid.".into()),
            };
            match vault::open_v2(&v2, passphrase) {
                vault::VaultOpenResult::AlterMode(bundle) => UnlockComputed::AlterMode(bundle),
                vault::VaultOpenResult::PmMode { pm_entries, pm_key } => {
                    UnlockComputed::PmMode { pm_entries, pm_key, vault: Box::new(v2) }
                }
                vault::VaultOpenResult::EmptyPm => UnlockComputed::EmptyPm(Box::new(v2)),
            }
        }
        vault::VaultVersion::Unknown => {
            UnlockComputed::Err("Format vault tidak dikenal.".into())
        }
    }
}

/// Terapkan hasil KDF ke App — berjalan kembali di main thread setelah spawn_blocking selesai.
pub(super) fn apply_unlock_result(app: &mut App, computed: UnlockComputed) {
    app.unlock_rx = None;
    match computed {
        UnlockComputed::AlterMode(bundle) => {
            app.pass_input.zeroize();
            app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
            app.keys = Some(build_self_keys(&bundle, None));
            refresh_invite(app);
            app.auth_error = None;
            load_contacts_into(app);
            inject_all_client_auth_keys(app);
            let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
            if app.tor.is_some() && has_restricted {
                trigger_tor_restart(app);
            }
            app.init_step = 1;
            app.init_start_tick = app.tick_count;
            app.screen = Screen::Init;
        }
        UnlockComputed::PmMode { pm_entries, pm_key, vault } => {
            app.pass_input.zeroize();
            app.pm_entries = pm_entries;
            let mut zk = zeroize::Zeroizing::new(pm_key);
            platform::try_mlock((&mut *zk).as_mut_ptr(), 32);
            app.pm_key = Some(zk);
            app.pm_vault_bytes = Some(vault);
            app.pm_is_readonly = false;
            app.screen = Screen::PmMain;
        }
        UnlockComputed::EmptyPm(vault) => {
            app.pass_input.zeroize();
            app.pm_entries = Vec::new();
            app.pm_key = None;
            app.pm_vault_bytes = Some(vault);
            app.pm_is_readonly = true;
            app.screen = Screen::PmMain;
        }
        UnlockComputed::NeedsMigration(bundle) => {
            // JANGAN zeroize pass_input — akan dipakai sebagai passphrase B saat migrasi
            app.migration_bundle = Some(bundle);
            app.auth_error = None;
            app.migration_phase = 0;
            app.screen = Screen::Migrate;
        }
        UnlockComputed::Err(msg) => {
            app.pass_input.zeroize();
            app.auth_error = Some(msg);
            app.screen = Screen::Unlock;
        }
    }
}

pub(super) fn handle_create_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Backspace => match app.create_phase {
            CreatePhase::AlterPass    => { app.pass_input.pop(); }
            CreatePhase::AlterConfirm => { app.pass_confirm.pop(); }
            CreatePhase::PmPass       => { app.pm_pass_input.pop(); }
            CreatePhase::PmConfirm    => { app.pm_pass_confirm.pop(); }
        },
        KeyCode::Char(c) => match app.create_phase {
            CreatePhase::AlterPass    => app.pass_input.push(c),
            CreatePhase::AlterConfirm => app.pass_confirm.push(c),
            CreatePhase::PmPass       => app.pm_pass_input.push(c),
            CreatePhase::PmConfirm    => app.pm_pass_confirm.push(c),
        },
        KeyCode::Enter => match app.create_phase {
            CreatePhase::AlterPass => {
                if app.pass_input.is_empty() {
                    app.auth_error = Some("Passphrase ALTER tidak boleh kosong.".into());
                } else {
                    app.auth_error = None;
                    app.create_phase = CreatePhase::AlterConfirm;
                }
            }
            CreatePhase::AlterConfirm => {
                if app.pass_confirm != app.pass_input {
                    app.auth_error = Some("Passphrase ALTER tidak cocok. Ulangi.".into());
                    app.pass_input.zeroize();
                    app.pass_confirm.zeroize();
                    app.create_phase = CreatePhase::AlterPass;
                } else {
                    app.pass_confirm.zeroize();
                    app.auth_error = None;
                    app.create_phase = CreatePhase::PmPass;
                }
            }
            CreatePhase::PmPass => {
                if app.pm_pass_input.is_empty() {
                    app.auth_error = Some("Passphrase PM tidak boleh kosong.".into());
                } else if app.pm_pass_input == app.pass_input {
                    app.auth_error = Some(
                        "Passphrase PM tidak boleh sama dengan passphrase ALTER.".into(),
                    );
                } else {
                    app.auth_error = None;
                    app.create_phase = CreatePhase::PmConfirm;
                }
            }
            CreatePhase::PmConfirm => {
                if app.pm_pass_confirm != app.pm_pass_input {
                    app.auth_error = Some("Passphrase PM tidak cocok. Ulangi.".into());
                    app.pm_pass_input.zeroize();
                    app.pm_pass_confirm.zeroize();
                    app.create_phase = CreatePhase::PmPass;
                } else {
                    match create_vault_v2(app) {
                        Ok(()) => {
                            app.pass_input.zeroize();
                            app.pass_confirm.zeroize();
                            app.pm_pass_input.zeroize();
                            app.pm_pass_confirm.zeroize();
                            app.auth_error = None;
                            app.create_phase = CreatePhase::AlterPass;
                            app.init_step = 1;
                            app.init_start_tick = app.tick_count;
                            app.screen = Screen::Init;
                            app.show_onboard_after_init = true;
                        }
                        Err(_) => {
                            app.auth_error = Some("Gagal membuat vault.".into());
                        }
                    }
                }
            }
        },
        _ => {}
    }
    false
}

pub(super) fn handle_init_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter if app.init_step >= 4 => {
            app.screen = Screen::Main;
        }
        _ => {}
    }
    false
}

fn create_vault_v2(app: &mut App) -> Result<(), crate::error::Error> {
    let bundle = KeyBundle::generate();
    let v2 = vault::create_v2(
        &bundle,
        app.pass_input.as_bytes(),
        app.pm_pass_input.as_bytes(),
    )?;
    vault::write_vault_v2(&app.vault_path, &v2)?;
    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
    app.keys = Some(build_self_keys(&bundle, None));
    refresh_invite(app);
    load_contacts_into(app);
    Ok(())
}

pub(super) fn handle_migrate_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Backspace => {
            if app.migration_phase == 0 {
                app.pm_pass_input.pop();
            } else {
                app.pm_pass_confirm.pop();
            }
        }
        KeyCode::Char(c) => {
            if app.migration_phase == 0 {
                app.pm_pass_input.push(c);
            } else {
                app.pm_pass_confirm.push(c);
            }
        }
        KeyCode::Enter => {
            if app.migration_phase == 0 {
                if app.pm_pass_input.is_empty() {
                    app.auth_error = Some("Passphrase PM tidak boleh kosong.".into());
                } else if app.pm_pass_input == app.pass_input {
                    app.auth_error = Some(
                        "Passphrase PM tidak boleh sama dengan passphrase ALTER.".into(),
                    );
                } else {
                    app.auth_error = None;
                    app.migration_phase = 1;
                }
            } else {
                if app.pm_pass_confirm != app.pm_pass_input {
                    app.auth_error = Some("Passphrase tidak cocok. Ulangi.".into());
                    app.pm_pass_input.zeroize();
                    app.pm_pass_confirm.zeroize();
                    app.migration_phase = 0;
                } else {
                    do_migrate(app);
                }
            }
        }
        _ => {}
    }
    false
}

pub(super) fn do_migrate(app: &mut App) {
    let bundle = match app.migration_bundle.take() {
        Some(b) => b,
        None => {
            app.auth_error = Some("State error: bundle tidak ada.".into());
            return;
        }
    };

    let v2 = match vault::create_v2(
        &bundle,
        app.pass_input.as_bytes(),
        app.pm_pass_input.as_bytes(),
    ) {
        Ok(v) => v,
        Err(_) => {
            app.auth_error = Some("Gagal membuat vault v2.".into());
            // Kembalikan bundle supaya bisa retry
            // (sudah di-take — jika retry butuh re-unlock)
            return;
        }
    };

    if let Err(_) = vault::write_vault_v2(&app.vault_path, &v2) {
        app.auth_error = Some("Gagal menulis vault ke disk.".into());
        return;
    }

    // Setup ALTER mode
    app.contacts_key = Some(contacts::derive_contacts_key(&bundle));
    app.keys = Some(build_self_keys(&bundle, None));
    refresh_invite(app);
    load_contacts_into(app);
    inject_all_client_auth_keys(app);
    let has_restricted = app.contacts.iter().any(|c| c.tor_client_auth_pub.is_some());
    if app.tor.is_some() && has_restricted {
        trigger_tor_restart(app);
    }

    // Bersihkan state sensitif
    app.pass_input.zeroize();
    app.pm_pass_input.zeroize();
    app.pm_pass_confirm.zeroize();
    app.migration_bundle = None;
    app.migration_phase = 0;
    app.auth_error = None;

    app.init_step = 1;
    app.init_start_tick = app.tick_count;
    app.show_onboard_after_init = false;
    app.screen = Screen::Init;
}
