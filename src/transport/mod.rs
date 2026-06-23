//! Transport layer: framing, LAN discovery, Tor, dan orkestrasi koneksi.
//!
//! M1: LAN (mDNS + direct TCP).
//! M2: Tor onion service via arti-client, dengan fallback LAN-first → Tor.

pub mod frame;
pub mod lan;
pub mod tor;

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use arti_client::DataStream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::error::Error;
use tor::TorContext;

/// Timeout percobaan LAN sebelum fallback ke Tor (PRD: ~3 detik).
const LAN_AUTO_TIMEOUT: Duration = Duration::from_secs(3);

/// Peran dalam handshake Noise_IK. Deterministik dari perbandingan fingerprint
/// supaya kedua sisi tidak sama-sama mendial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

/// Mode jalur LAN.
#[derive(Clone, Copy, Debug)]
pub enum LanMode {
    /// Otomatis: role dari fingerprint, discovery via mDNS.
    Auto,
    /// Paksa responder, listen di port tertentu (testing satu mesin).
    Listen(u16),
    /// Paksa initiator, dial langsung (testing).
    Dial(SocketAddr),
    /// Lewati LAN sepenuhnya (Tor saja). Disediakan untuk kontak onion-only;
    /// belum di-wire ke UI (default app pakai Auto = LAN-first → Tor).
    #[allow(dead_code)]
    Off,
}

/// Koneksi transport: TCP (LAN) atau DataStream (Tor). Keduanya
/// mengimplementasikan tokio AsyncRead/AsyncWrite, didelegasikan di bawah.
pub enum Conn {
    Tcp(TcpStream),
    Tor(DataStream),
}

impl AsyncRead for Conn {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_read(cx, buf),
            Conn::Tor(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_write(cx, buf),
            Conn::Tor(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_flush(cx),
            Conn::Tor(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Tor(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

fn role_from_fp(my_fp: &str, target_fp: &str) -> Role {
    if my_fp < target_fp {
        Role::Initiator
    } else {
        Role::Responder
    }
}

/// Timeout total untuk Initiator mencoba dial .onion peer (dengan retry).
const TOR_DIAL_TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Delay antar retry dial Tor.
const TOR_DIAL_RETRY_DELAY: Duration = Duration::from_secs(8);

/// Timeout Responder menunggu koneksi masuk dari Initiator via Tor.
const TOR_ACCEPT_TIMEOUT: Duration = Duration::from_secs(120);

/// Bangun koneksi ke peer dengan fallback LAN-first → Tor.
///
/// - `lan`: mode jalur LAN.
/// - `onion`: onion address peer (untuk fallback Tor); None bila kontak LAN-only.
/// - `tor`: konteks Tor aktif (None bila online).
pub async fn establish(
    my_fp: &str,
    target_fp: &str,
    lan: LanMode,
    onion: Option<&str>,
    tor: Option<&Arc<TorContext>>,
) -> Result<(Conn, Role), Error> {
    let role = match lan {
        LanMode::Dial(_) => Role::Initiator,
        LanMode::Listen(_) => Role::Responder,
        LanMode::Auto | LanMode::Off => role_from_fp(my_fp, target_fp),
    };

    let tor_available = tor.is_some() && onion.is_some();

    // 1) Coba LAN dulu (kecuali Off).
    if !matches!(lan, LanMode::Off) {
        // Timeout LAN hanya berguna bila ada Tor untuk di-fallback. Di mode
        // LAN-only, JANGAN menyerah — tunggu peer masuk room (sampai Esc).
        let timeout = match lan {
            LanMode::Auto if tor_available => Some(LAN_AUTO_TIMEOUT),
            _ => None,
        };
        match try_lan(role, my_fp, target_fp, lan, timeout).await {
            Ok(tcp) => return Ok((Conn::Tcp(tcp), role)),
            Err(e) => {
                // Hanya fallback bila Tor tersedia; jika tidak, kembalikan error LAN.
                if !tor_available {
                    return Err(e);
                }
            }
        }
    }

    // 2) Fallback Tor — role deterministik, Initiator retry dial, Responder accept dengan timeout.
    let tor = tor.ok_or_else(|| Error::Tor("Tor tidak aktif".into()))?;
    match role {
        Role::Initiator => {
            let host = onion.ok_or_else(|| Error::Tor("kontak tidak punya onion address".into()))?;
            tor_dial_with_retry(tor, host).await.map(|ds| (Conn::Tor(ds), role))
        }
        Role::Responder => {
            let ds = tor
                .accept_timeout(TOR_ACCEPT_TIMEOUT)
                .await
                .ok_or(Error::ConnectionClosed)?;
            Ok((Conn::Tor(ds), role))
        }
    }
}

/// Dial .onion peer dengan retry dan exponential backoff.
///
/// Onion descriptor peer mungkin belum terpublikasikan ke Tor network (~1–3
/// menit setelah bootstrap). Retry memastikan kita tidak menyerah saat
/// descriptor belum siap — coba lagi setiap `TOR_DIAL_RETRY_DELAY` detik
/// sampai total timeout `TOR_DIAL_TOTAL_TIMEOUT`.
async fn tor_dial_with_retry(
    tor: &Arc<TorContext>,
    host: &str,
) -> Result<DataStream, Error> {
    let deadline = tokio::time::Instant::now() + TOR_DIAL_TOTAL_TIMEOUT;
    let mut last_err = Error::ConnectionClosed;

    loop {
        match tor.connect(host, tor::TOR_VIRTUAL_PORT).await {
            Ok(ds) => return Ok(ds),
            Err(e) => {
                last_err = e;
                if tokio::time::Instant::now() + TOR_DIAL_RETRY_DELAY > deadline {
                    break; // tidak cukup waktu untuk retry lagi
                }
                tokio::time::sleep(TOR_DIAL_RETRY_DELAY).await;
            }
        }
    }

    Err(last_err)
}



async fn try_lan(
    role: Role,
    my_fp: &str,
    target_fp: &str,
    lan: LanMode,
    timeout: Option<Duration>,
) -> Result<TcpStream, Error> {
    let fut = async {
        match lan {
            LanMode::Dial(addr) => TcpStream::connect(addr).await.map_err(Error::Io),
            LanMode::Listen(port) => {
                let listener = TcpListener::bind(("0.0.0.0", port)).await?;
                let (s, _) = listener.accept().await?;
                Ok(s)
            }
            LanMode::Auto => lan_auto(role, my_fp, target_fp).await,
            LanMode::Off => Err(Error::ConnectionClosed),
        }
    };

    match timeout {
        Some(d) => tokio::time::timeout(d, fut)
            .await
            .map_err(|_| Error::ConnectionClosed)?,
        None => fut.await,
    }
}

async fn lan_auto(role: Role, my_fp: &str, target_fp: &str) -> Result<TcpStream, Error> {
    let daemon = lan::new_daemon()?;
    match role {
        Role::Responder => {
            let listener = TcpListener::bind("0.0.0.0:0").await?;
            let port = listener.local_addr()?.port();
            lan::advertise(&daemon, my_fp, port)?;
            let (stream, _peer) = listener.accept().await?;
            let _ = daemon.shutdown();
            Ok(stream)
        }
        Role::Initiator => {
            let (tx, mut rx) = mpsc::unbounded_channel();
            lan::spawn_browse(&daemon, tx)?;
            // Peer multi-homed mengiklankan banyak IP (LAN asli + adapter virtual).
            // Coba SEMUA: jangan menyerah pada satu connection-refused — alamat
            // yang benar mungkin datang berikutnya. Konsisten dengan filosofi
            // LAN-only "tunggu sampai peer bisa dijangkau" (sampai user Esc).
            let mut tried = std::collections::HashSet::new();
            let mut last_err = None;
            loop {
                match rx.recv().await {
                    Some(peer) if peer.fingerprint == target_fp => {
                        if !tried.insert(peer.addr) {
                            continue; // alamat ini sudah dicoba & gagal
                        }
                        match TcpStream::connect(peer.addr).await {
                            Ok(stream) => {
                                let _ = daemon.shutdown();
                                return Ok(stream);
                            }
                            // Alamat tak bisa di-connect (mis. adapter virtual /
                            // port basi). Simpan error, lanjut coba alamat lain.
                            Err(e) => {
                                last_err = Some(Error::Io(e));
                                continue;
                            }
                        }
                    }
                    Some(_) => continue,
                    None => return Err(last_err.unwrap_or(Error::ConnectionClosed)),
                }
            }
        }
    }
}
