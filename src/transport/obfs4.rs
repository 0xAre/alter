//! SEC-06: obfs4proxy runtime detection.
//!
//! Cari binary `obfs4proxy` di PATH saat startup. Jika ditemukan, ALTER
//! menampilkan badge "obfs4" di header TUI dan bisa menggunakan PT untuk
//! menyamarkan traffic Tor dari Deep Packet Inspection (DPI).
//!
//! Strategy (R-02 resolved): runtime detection — binary ALTER tetap kecil;
//! user yang butuh obfs4 cukup install `obfs4proxy` secara terpisah.
//! Tidak ada → Tor berjalan normal, badge menunjukkan "(no obfs4)".

use std::path::PathBuf;

/// Status ketersediaan obfs4proxy di sistem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Obfs4Status {
    /// obfs4proxy ditemukan di PATH — path ke binary disimpan.
    Available(PathBuf),
    /// obfs4proxy tidak ditemukan — ALTER jalan dengan Tor biasa (tanpa obfuscation).
    NotFound,
}

impl Obfs4Status {
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        matches!(self, Obfs4Status::Available(_))
    }

    /// Label singkat untuk badge TUI.
    pub fn badge_label(&self) -> &'static str {
        match self {
            Obfs4Status::Available(_) => " obfs4 ",
            Obfs4Status::NotFound => "",
        }
    }
}

/// Cari binary `obfs4proxy` di direktori PATH sistem.
///
/// Dipanggil sekali saat startup — hasilnya di-cache di `App.obfs4_status`.
/// Jika `PATH` tidak tersedia (env error), mengembalikan `NotFound`.
pub fn detect() -> Obfs4Status {
    let binary = if cfg!(windows) { "obfs4proxy.exe" } else { "obfs4proxy" };

    let path_var = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return Obfs4Status::NotFound,
    };

    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
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
        // Tidak bisa assert value konkret (bergantung sistem), tapi harus tidak panic.
        let status = detect();
        // Badge label valid di kedua cabang
        let label = status.badge_label();
        assert!(label == " obfs4 " || label == "");
    }

    #[test]
    fn not_found_is_not_available() {
        assert!(!Obfs4Status::NotFound.is_available());
    }

    #[test]
    fn available_is_available() {
        let s = Obfs4Status::Available(PathBuf::from("/usr/bin/obfs4proxy"));
        assert!(s.is_available());
    }
}
