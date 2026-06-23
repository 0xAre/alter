//! LAN discovery (mDNS) + helper TCP.
//!
//! Strategi discovery:
//!   - Responder mengiklankan service `_alter._tcp.local.` dengan TXT record
//!     berisi fingerprint Ed25519-nya (`fp=<hex>`).
//!   - Initiator mem-browse service yang sama, mencocokkan fingerprint dengan
//!     kontak yang dikenal, lalu dial ke alamat:port yang diiklankan.
//!
//! Yang diiklankan hanya fingerprint Ed25519 (identitas yang sudah diketahui
//! kontak), BUKAN X25519 Noise key — jadi mDNS tidak membocorkan material
//! handshake. Tetap ada metadata leak level LAN (kehadiran identitas), yang
//! dapat diterima untuk M1; pengerasan menyusul.

use std::net::{IpAddr, SocketAddr};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::mpsc::UnboundedSender;

use crate::error::Error;

pub const SERVICE_TYPE: &str = "_alter._tcp.local.";

/// Batas aman panjang satu DNS label. Spec DNS membatasi label ≤ 63 byte, dan
/// `mdns-sd` menegakkannya lewat `assert!(s.len() < 64)` saat encode paket —
/// melebihi ini akan mem-panic thread `mDNS_daemon` (bukan error yang bisa
/// ditangani). Kita pakai margin di 32 byte: cukup unik antar peer, jauh dari batas.
const MAX_LABEL_LEN: usize = 32;

/// Pendekkan sembarang string menjadi satu DNS label yang aman (≤ [`MAX_LABEL_LEN`]).
///
/// Pemotongan dilakukan pada batas char (bukan byte mentah) supaya tidak pernah
/// memecah karakter multi-byte UTF-8 — walau saat ini input selalu hex ASCII.
fn safe_label(s: &str) -> String {
    let mut out = String::with_capacity(MAX_LABEL_LEN);
    for ch in s.chars() {
        if out.len() + ch.len_utf8() > MAX_LABEL_LEN {
            break;
        }
        out.push(ch);
    }
    out
}

/// Apakah `ip` masuk akal untuk di-dial oleh peer lain di LAN?
///
/// Buang loopback, unspecified (0.0.0.0), dan link-local IPv4 (169.254.x —
/// alamat APIPA yang muncul di adapter virtual/disconnected dan TIDAK bisa
/// di-route antar mesin). IPv6 di-skip: discovery LAN kita fokus IPv4, dan
/// IPv6 link-local butuh scope id yang merepotkan lintas mesin.
fn is_lan_dialable(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_unspecified() && !v4.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

/// IP LAN lokal yang layak diiklankan (hanya yang `is_lan_dialable`).
fn local_lan_ips() -> Vec<IpAddr> {
    if_addrs::get_if_addrs()
        .map(|ifaces| {
            ifaces
                .into_iter()
                .map(|i| i.ip())
                .filter(is_lan_dialable)
                .collect()
        })
        .unwrap_or_default()
}

/// Peer yang ter-resolve dari mDNS.
pub struct DiscoveredPeer {
    pub fingerprint: String,
    pub addr: SocketAddr,
}

/// Buat daemon mDNS baru. Daemon harus tetap hidup selama advertise/browse aktif.
pub fn new_daemon() -> Result<ServiceDaemon, Error> {
    ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))
}

/// Iklankan kehadiran kita di LAN pada `port`, membawa `fingerprint` di TXT.
pub fn advertise(daemon: &ServiceDaemon, fingerprint: &str, port: u16) -> Result<(), Error> {
    // Hostname dan instance name masing-masing jadi satu DNS label. Fingerprint
    // Ed25519 hex panjangnya 64 char — melebihi batas 63 byte — jadi keduanya
    // WAJIB dipendekkan via `safe_label`, kalau tidak thread mDNS akan panic.
    // Fingerprint lengkap tetap dibawa di TXT (`fp`), dan itu yang dipakai untuk
    // matching saat browse, jadi label pendek cukup unik-ish per peer.
    let host = format!("{}.local.", safe_label(fingerprint));
    let instance = safe_label(fingerprint);
    let props = [("fp", fingerprint)];

    // Iklankan HANYA IP LAN asli. `enable_addr_auto` menyiarkan semua interface
    // termasuk link-local 169.254.x (adapter virtual: WSL/Hyper-V/Bluetooth) —
    // initiator bisa salah pilih dan kena "connection refused". Dengan menyetel
    // alamat eksplisit, peer (bahkan versi lama) hanya menerima IP yang dapat
    // di-route. Fallback ke auto-detect bila enumerasi gagal/kosong.
    let ips = local_lan_ips();
    let info = if ips.is_empty() {
        ServiceInfo::new(SERVICE_TYPE, &instance, &host, "", port, &props[..])
            .map_err(|e| Error::Mdns(e.to_string()))?
            .enable_addr_auto()
    } else {
        let ip_csv = ips
            .iter()
            .map(|ip| ip.to_string())
            .collect::<Vec<_>>()
            .join(",");
        ServiceInfo::new(SERVICE_TYPE, &instance, &host, ip_csv.as_str(), port, &props[..])
            .map_err(|e| Error::Mdns(e.to_string()))?
    };
    daemon
        .register(info)
        .map_err(|e| Error::Mdns(e.to_string()))
}

/// Mulai browsing. Setiap peer yang ter-resolve dikirim ke `tx`.
///
/// Browsing dijalankan di thread tersendiri (receiver mDNS bersifat blocking),
/// supaya tidak memblokir runtime async. Thread berhenti saat `tx` ditutup.
pub fn spawn_browse(daemon: &ServiceDaemon, tx: UnboundedSender<DiscoveredPeer>) -> Result<(), Error> {
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| Error::Mdns(e.to_string()))?;

    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if let ServiceEvent::ServiceResolved(info) = event {
                let fp = match info.get_property_val_str("fp") {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let port = info.get_port();
                for ip in info.get_addresses() {
                    let ip_addr = ip.to_ip_addr();
                    // Lewati alamat yang tak mungkin di-dial dari mesin lain
                    // (loopback, unspecified, link-local 169.254.x dari adapter
                    // virtual). Defense-in-depth: peer versi lama mungkin masih
                    // menyiarkan alamat-alamat ini lewat `enable_addr_auto`.
                    if !is_lan_dialable(&ip_addr) {
                        continue;
                    }
                    let peer = DiscoveredPeer {
                        fingerprint: fp.clone(),
                        addr: SocketAddr::new(ip_addr, port),
                    };
                    if tx.send(peer).is_err() {
                        return; // konsumer berhenti → hentikan thread
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_label_truncates_full_fingerprint_below_dns_limit() {
        // Fingerprint Ed25519 hex = 64 char; harus dipendekkan ke ≤ 63 (batas DNS).
        let fp = "a".repeat(64);
        let label = safe_label(&fp);
        assert!(label.len() <= MAX_LABEL_LEN);
        assert!(label.len() < 64, "label harus < 64 agar mdns-sd tidak panic");
    }

    #[test]
    fn safe_label_leaves_short_input_untouched() {
        assert_eq!(safe_label("abc123"), "abc123");
    }

    #[test]
    fn lan_dialable_accepts_private_rejects_linklocal_and_loopback() {
        use std::net::Ipv4Addr;
        // IP LAN privat → boleh dial.
        assert!(is_lan_dialable(&IpAddr::V4(Ipv4Addr::new(192, 168, 94, 99))));
        assert!(is_lan_dialable(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))));
        // Link-local APIPA (169.254.x), loopback, unspecified → tolak.
        assert!(!is_lan_dialable(&IpAddr::V4(Ipv4Addr::new(169, 254, 157, 99))));
        assert!(!is_lan_dialable(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_lan_dialable(&IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        // IPv6 di-skip untuk discovery LAN.
        assert!(!is_lan_dialable(&IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn safe_label_never_splits_multibyte_char() {
        // 17 karakter 2-byte (é) = 34 byte; harus berhenti pada batas char,
        // tidak pernah memotong di tengah byte UTF-8.
        let s = "é".repeat(17);
        let label = safe_label(&s);
        assert!(label.len() <= MAX_LABEL_LEN);
        assert!(label.is_char_boundary(label.len()));
    }
}
