//! SEC-06/SEC-14: Pluggable Transport runtime detection.
//!
//! M3 (SEC-06): deteksi `obfs4proxy` di PATH saat startup.
//! M5 (SEC-14): tambah deteksi `lyrebird` sebagai prioritas utama.
//!
//! Lyrebird adalah fork aktif dari obfs4proxy yang dikelola Tor Project
//! (obfs4proxy sendiri sudah tidak aktif dikembangkan sejak 2023).
//! ALTER mendeteksi lyrebird lebih dulu; obfs4proxy sebagai fallback.
//!
//! Strategy (R-02 resolved): runtime detection — binary ALTER tetap kecil;
//! user yang butuh PT cukup install `lyrebird` atau `obfs4proxy` secara terpisah.

use std::path::PathBuf;

/// Status ketersediaan pluggable transport binary di sistem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Obfs4Status {
    /// lyrebird atau obfs4proxy ditemukan di PATH — path ke binary disimpan.
    Available(PathBuf),
    /// Tidak ditemukan — ALTER jalan dengan Tor biasa (tanpa obfuscation).
    NotFound,
}

impl Obfs4Status {
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        matches!(self, Obfs4Status::Available(_))
    }

    /// Label singkat untuk badge TUI.
    /// Membaca nama binary dari path untuk membedakan lyrebird vs obfs4proxy.
    pub fn badge_label(&self) -> &'static str {
        match self {
            Obfs4Status::Available(path) => {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if name.starts_with("lyrebird") {
                    " lyrebird "
                } else {
                    " obfs4 "
                }
            }
            Obfs4Status::NotFound => "",
        }
    }
}

/// Cari pluggable transport binary di PATH sistem.
///
/// Urutan prioritas: lyrebird (aktif dikembangkan) → obfs4proxy (legacy fallback).
/// Dipanggil sekali saat startup — hasilnya di-cache di `App.obfs4_status`.
pub fn detect() -> Obfs4Status {
    let (lyrebird, obfs4proxy) = if cfg!(windows) {
        ("lyrebird.exe", "obfs4proxy.exe")
    } else {
        ("lyrebird", "obfs4proxy")
    };

    let path_var = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return Obfs4Status::NotFound,
    };

    let dirs: Vec<_> = std::env::split_paths(&path_var).collect();

    // Prioritas 1: lyrebird
    for dir in &dirs {
        let candidate = dir.join(lyrebird);
        if candidate.is_file() {
            return Obfs4Status::Available(candidate);
        }
    }

    // Prioritas 2: obfs4proxy (legacy)
    for dir in &dirs {
        let candidate = dir.join(obfs4proxy);
        if candidate.is_file() {
            return Obfs4Status::Available(candidate);
        }
    }

    Obfs4Status::NotFound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_a_status() {
        let status = detect();
        let label = status.badge_label();
        assert!(label == " lyrebird " || label == " obfs4 " || label == "");
    }

    #[test]
    fn not_found_is_not_available() {
        assert!(!Obfs4Status::NotFound.is_available());
    }

    #[test]
    fn available_lyrebird_badge() {
        let s = Obfs4Status::Available(PathBuf::from("/usr/bin/lyrebird"));
        assert_eq!(s.badge_label(), " lyrebird ");
        assert!(s.is_available());
    }

    #[test]
    fn available_obfs4proxy_badge() {
        let s = Obfs4Status::Available(PathBuf::from("/usr/bin/obfs4proxy"));
        assert_eq!(s.badge_label(), " obfs4 ");
        assert!(s.is_available());
    }
}
