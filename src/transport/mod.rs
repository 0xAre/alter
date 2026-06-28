//! Transport layer: framing, LAN discovery, Tor, dan orkestrasi koneksi.
//!
//! M1: LAN (mDNS + direct TCP).
//! M2: Tor onion service via arti-client, dengan fallback LAN-first → Tor.
//! M5 (SEC-13): sebelum dial, inject our client auth key ke keystore arti.

pub mod frame;
pub mod lan;
pub mod obfs4;
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

const LAN_AUTO_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

#[derive(Clone, Copy, Debug)]
pub enum LanMode {
    Auto,
    Listen(u16),
    Dial(SocketAddr),
    #[allow(dead_code)]
    Off,
}

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

const TOR_DIAL_TOTAL_TIMEOUT: Duration = Duration::from_secs(120);
const TOR_DIAL_RETRY_DELAY: Duration = Duration::from_secs(8);
const TOR_ACCEPT_TIMEOUT: Duration = Duration::from_secs(120);

/// Bangun koneksi ke peer dengan fallback LAN-first → Tor.
///
/// `our_tor_auth_secret`: secret seed kunci client auth kita (SEC-13). Dipakai
/// untuk inject ke keystore arti sebelum dial ke onion peer yang punya
/// restricted discovery. Wajib diisi bila online; None berarti tidak di-inject.
pub async fn establish(
    my_fp: &str,
    target_fp: &str,
    lan: LanMode,
    onion: Option<&str>,
    tor: Option<&Arc<TorContext>>,
    our_tor_auth_secret: Option<[u8; 32]>,
) -> Result<(Conn, Role), Error> {
    let role = match lan {
        LanMode::Dial(_) => Role::Initiator,
        LanMode::Listen(_) => Role::Responder,
        LanMode::Auto | LanMode::Off => role_from_fp(my_fp, target_fp),
    };

    let tor_available = tor.is_some() && onion.is_some();

    if !matches!(lan, LanMode::Off) {
        let timeout = match lan {
            LanMode::Auto if tor_available => Some(LAN_AUTO_TIMEOUT),
            _ => None,
        };
        match try_lan(role, my_fp, target_fp, lan, timeout).await {
            Ok(tcp) => return Ok((Conn::Tcp(tcp), role)),
            Err(e) => {
                if !tor_available {
                    return Err(e);
                }
            }
        }
    }

    let tor = tor.ok_or_else(|| Error::Tor("Tor tidak aktif".into()))?;
    match role {
        Role::Initiator => {
            let host =
                onion.ok_or_else(|| Error::Tor("kontak tidak punya onion address".into()))?;
            // SEC-13: inject our client auth key sebelum dial agar arti bisa
            // decrypt descriptor peer yang restricted. Gagal secara diam-diam
            // — koneksi tetap dicoba (peer mungkin tidak pakai restricted discovery).
            if let Some(secret) = our_tor_auth_secret {
                let _ = tor.register_client_auth_key(host, secret).await;
            }
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

async fn tor_dial_with_retry(tor: &Arc<TorContext>, host: &str) -> Result<DataStream, Error> {
    let deadline = tokio::time::Instant::now() + TOR_DIAL_TOTAL_TIMEOUT;

    loop {
        match tor.connect(host, tor::TOR_VIRTUAL_PORT).await {
            Ok(ds) => return Ok(ds),
            Err(e) => {
                if tokio::time::Instant::now() + TOR_DIAL_RETRY_DELAY > deadline {
                    return Err(e);
                }
                tokio::time::sleep(TOR_DIAL_RETRY_DELAY).await;
            }
        }
    }
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
            let mut tried = std::collections::HashSet::new();
            let mut last_err = None;
            loop {
                match rx.recv().await {
                    Some(peer) if peer.fingerprint == target_fp => {
                        if !tried.insert(peer.addr) {
                            continue;
                        }
                        match TcpStream::connect(peer.addr).await {
                            Ok(stream) => {
                                let _ = daemon.shutdown();
                                return Ok(stream);
                            }
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
