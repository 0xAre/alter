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
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};
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

/// Koneksi transport: TCP (LAN), DataStream (Tor), atau DuplexStream (channel
/// bridge dipakai setelah role negotiation agar DataStream bisa di-unsplit).
pub enum Conn {
    Tcp(TcpStream),
    Tor(DataStream),
    /// Channel bridge: dihasilkan oleh `negotiate_role` untuk membungkus
    /// sepasang ReadHalf/WriteHalf DataStream yang tidak bisa di-unsplit langsung.
    Duplex(DuplexStream),
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
            Conn::Duplex(s) => Pin::new(s).poll_read(cx, buf),
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
            Conn::Duplex(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_flush(cx),
            Conn::Tor(s) => Pin::new(s).poll_flush(cx),
            Conn::Duplex(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Tor(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Duplex(s) => Pin::new(s).poll_shutdown(cx),
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

/// Byte yang dikirim saat role negotiation untuk menunjukkan preferensi role.
const ROLE_HINT_INITIATOR: u8 = 0x49; // 'I'
const ROLE_HINT_RESPONDER: u8 = 0x52; // 'R'

/// Timeout menunggu koneksi Tor masuk (sisi Responder dalam symmetric connect).
const TOR_ACCEPT_TIMEOUT: Duration = Duration::from_secs(90);

/// Bangun koneksi ke peer dengan fallback LAN-first → Tor.
///
/// - `lan`: mode jalur LAN.
/// - `onion`: onion address peer (untuk fallback Tor); None bila kontak LAN-only.
/// - `tor`: konteks Tor aktif (None bila `--tor` tidak diaktifkan).
pub async fn establish(
    my_fp: &str,
    target_fp: &str,
    lan: LanMode,
    onion: Option<&str>,
    tor: Option<&Arc<TorContext>>,
) -> Result<(Conn, Role), Error> {
    let lan_role = match lan {
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
        match try_lan(lan_role, my_fp, target_fp, lan, timeout).await {
            Ok(tcp) => return Ok((Conn::Tcp(tcp), lan_role)),
            Err(e) => {
                // Hanya fallback bila Tor tersedia; jika tidak, kembalikan error LAN.
                if !tor_available {
                    return Err(e);
                }
            }
        }
    }

    // 2) Fallback Tor — gunakan Symmetric Connect.
    let tor = tor.ok_or_else(|| Error::Tor("Tor tidak aktif".into()))?;
    let onion = onion.ok_or_else(|| Error::Tor("kontak tidak punya onion address".into()))?;
    establish_tor_symmetric(my_fp, target_fp, onion, tor).await
}

/// Symmetric Connect via Tor: kedua sisi race dial-peer vs accept-incoming.
///
/// Siapapun yang menang (dial atau accept), role di-negosiasikan via 1 byte
/// sebelum Noise handshake. Ini menghilangkan ketergantungan pada onion descriptor
/// peer sudah terpublikasikan: jika peer-lah yang berhasil dial kita duluan,
/// kita tetap terhubung tanpa perlu menunggu onion kita sendiri.
async fn establish_tor_symmetric(
    my_fp: &str,
    target_fp: &str,
    onion: &str,
    tor: &Arc<TorContext>,
) -> Result<(Conn, Role), Error> {
    // Kloning Arc supaya bisa di-move ke dua branch sekaligus.
    let tor_dial = Arc::clone(tor);
    let tor_accept = Arc::clone(tor);
    let onion_owned = onion.to_string();

    tokio::select! {
        // Branch A: kita dial .onion peer → prefer Initiator
        dial_result = async move { tor_dial.connect(&onion_owned, tor::TOR_VIRTUAL_PORT).await } => {
            let ds = dial_result?;
            negotiate_role(ds, ROLE_HINT_INITIATOR, my_fp, target_fp).await
        }
        // Branch B: peer dial kita → prefer Responder
        accept_result = tor_accept.accept_timeout(TOR_ACCEPT_TIMEOUT) => {
            let ds = accept_result.ok_or(Error::ConnectionClosed)?;
            negotiate_role(ds, ROLE_HINT_RESPONDER, my_fp, target_fp).await
        }
    }
}

/// Negosiasikan role Initiator/Responder via 1-byte mini-protocol sebelum Noise.
///
/// Kedua sisi mengirim byte hint mereka **dan** membaca byte hint peer secara
/// simultan (tanpa tunggu dulu). Jika ada konflik (dua Initiator atau dua
/// Responder), fingerprint lebih kecil menang menjadi Initiator.
///
/// Mengembalikan `(DataStream, Role)` — stream yang sama dikembalikan untuk
/// dipakai oleh session layer di atasnya.
async fn negotiate_role(
    ds: arti_client::DataStream,
    my_hint: u8,
    my_fp: &str,
    target_fp: &str,
) -> Result<(Conn, Role), Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

    // Wrap DataStream ke tokio-compat agar bisa pakai tokio AsyncRead/AsyncWrite.
    // Kita butuh baca + tulis 1 byte secara simultan → split dulu.
    let (rd_compat, wr_compat) = ds.split();
    let mut rd = rd_compat.compat();
    let mut wr = wr_compat.compat_write();

    // Kirim hint kita + baca hint peer secara simultan.
    let hint_buf = [my_hint];
    let send_fut = wr.write_all(&hint_buf);
    let recv_fut = async {
        let mut buf = [0u8; 1];
        rd.read_exact(&mut buf).await?;
        Ok::<u8, std::io::Error>(buf[0])
    };

    let (send_res, recv_res) = tokio::join!(send_fut, recv_fut);
    send_res?;
    let peer_hint = recv_res?;

    // Flush setelah write.
    wr.flush().await?;

    // Tentukan role berdasarkan hint yang diterima.
    let role = match (my_hint, peer_hint) {
        (ROLE_HINT_INITIATOR, ROLE_HINT_RESPONDER) => Role::Initiator,
        (ROLE_HINT_RESPONDER, ROLE_HINT_INITIATOR) => Role::Responder,
        // Konflik: keduanya prefer Initiator atau keduanya prefer Responder.
        // Tiebreak: fingerprint lebih kecil jadi Initiator.
        _ => role_from_fp(my_fp, target_fp),
    };

    // DataStream dari arti-client tidak bisa di-unsplit setelah split().
    // Solusi: buat channel bridge via tokio::io::duplex — client_side dipakai
    // oleh session layer di atas, server_side dihubungkan ke DataStream halves
    // lewat dua copy task yang berjalan di background.
    let (client_side, server_side) = tokio::io::duplex(65536);
    tokio::spawn(async move {
        let (mut ds_rd, mut ds_wr) = tokio::io::split(server_side);
        tokio::select! {
            _ = tokio::io::copy(&mut rd, &mut ds_wr) => {}
            _ = tokio::io::copy(&mut ds_rd, &mut wr) => {}
        }
    });

    Ok((Conn::Duplex(client_side), role))
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
