//! Session state machine — "Room" dalam terminologi PRD v0.3.
//!
//! Lifecycle: Connecting → Handshaking → Active → Closed.
//!
//! Saat Closed, `EncryptedSession` (yang membungkus `snow::TransportState`)
//! di-drop; snow menghapus key transport secara internal. Ini SEC-03a:
//! session key tidak bisa dipulihkan setelah room ditutup — ephemeral-by-design,
//! bukan ephemeral-by-policy.

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::crypto::handshake::HandshakeSession;
use crate::error::Error;
use crate::transport::frame::{read_frame, write_frame, MAX_FRAME_LEN};
use crate::transport::Role;

/// State room yang dilaporkan ke UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Handshaking,
    Active,
    Closed,
}

/// Event dari session task ke UI.
#[derive(Clone, Debug)]
pub enum SessionEvent {
    StateChanged(SessionState),
    /// Pesan plaintext yang diterima dari peer.
    Message(String),
    /// Peer menutup koneksi (keluar dari room).
    PeerLeft,
    /// Error fatal — koneksi gagal atau handshake gagal.
    Error(String),
}

/// Jalankan satu sesi sampai selesai (peer keluar atau UI menutup outgoing).
///
/// - `peer_noise_pk`: untuk initiator WAJIB (static key responder yang dituju).
///   Untuk responder, jika `Some`, dipakai memverifikasi bahwa remote static
///   key cocok dengan kontak yang diharapkan (fail closed bila tidak cocok).
/// - `outgoing`: teks dari UI yang akan dienkripsi dan dikirim.
/// - `events`: kanal event ke UI.
pub async fn run_session<S>(
    stream: S,
    role: Role,
    local_noise_sk: [u8; 32],
    peer_noise_pk: Option<[u8; 32]>,
    mut outgoing: UnboundedReceiver<String>,
    events: UnboundedSender<SessionEvent>,
) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let _ = events.send(SessionEvent::StateChanged(SessionState::Handshaking));

    // Split lebih dulu supaya jalur read & write independen — bekerja untuk
    // TcpStream (LAN) maupun arti DataStream (Tor) lewat bound generic.
    let (mut rd, mut wr) = tokio::io::split(stream);

    let mut buf = vec![0u8; MAX_FRAME_LEN + 16];
    let mut scratch = vec![0u8; MAX_FRAME_LEN];

    // --- Handshake Noise_IK (2 message) ---
    let session = match role {
        Role::Initiator => {
            let peer_pk = peer_noise_pk.ok_or(Error::InvalidKey)?;
            let mut hs = HandshakeSession::new_initiator(&local_noise_sk, &peer_pk)?;

            // -> e, es, s, ss
            let n = hs.write_message(&[], &mut scratch)?;
            write_frame(&mut wr, &scratch[..n]).await?;

            // <- e, ee, se
            let n = read_frame(&mut rd, &mut buf).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed);
            }
            hs.read_message(&buf[..n], &mut scratch)?;

            hs.into_transport()?
        }
        Role::Responder => {
            let mut hs = HandshakeSession::new_responder(&local_noise_sk)?;

            // <- e, es, s, ss
            let n = read_frame(&mut rd, &mut buf).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed);
            }
            hs.read_message(&buf[..n], &mut scratch)?;

            // Verifikasi identitas peer bila kontak diketahui (fail closed).
            if let Some(expected) = peer_noise_pk {
                match hs.remote_static() {
                    Some(rs) if rs == expected => {}
                    _ => return Err(Error::IdentityMismatch),
                }
            }

            // -> e, ee, se
            let n = hs.write_message(&[], &mut scratch)?;
            write_frame(&mut wr, &scratch[..n]).await?;

            hs.into_transport()?
        }
    };

    let _ = events.send(SessionEvent::StateChanged(SessionState::Active));

    // --- Active loop ---
    // `session` adalah EncryptedSession; di-drop di akhir fungsi (SEC-03a).
    let mut session = session;

    loop {
        tokio::select! {
            maybe_text = outgoing.recv() => {
                match maybe_text {
                    Some(text) => {
                        let mut ct = vec![0u8; text.len() + 16];
                        let n = session.encrypt(text.as_bytes(), &mut ct)?;
                        if write_frame(&mut wr, &ct[..n]).await.is_err() {
                            break;
                        }
                    }
                    None => break, // UI menutup room
                }
            }
            res = read_frame(&mut rd, &mut buf) => {
                let n = match res {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    let _ = events.send(SessionEvent::PeerLeft);
                    break;
                }
                let mut pt = vec![0u8; n];
                match session.decrypt(&buf[..n], &mut pt) {
                    Ok(m) => {
                        let text = String::from_utf8_lossy(&pt[..m]).to_string();
                        let _ = events.send(SessionEvent::Message(text));
                    }
                    Err(_) => break, // dekripsi gagal → putus (fail closed)
                }
            }
        }
    }

    let _ = events.send(SessionEvent::StateChanged(SessionState::Closed));
    Ok(())
    // `session` drop di sini → snow menghapus transport key.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keypair::NoiseKey;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::mpsc;

    async fn wait_for_message(rx: &mut mpsc::UnboundedReceiver<SessionEvent>) -> String {
        loop {
            match rx.recv().await {
                Some(SessionEvent::Message(m)) => return m,
                Some(_) => continue,
                None => panic!("kanal tertutup sebelum pesan diterima"),
            }
        }
    }

    /// Integration test: dua sesi nyata di atas TCP loopback, handshake penuh,
    /// pesan bolak-balik terenkripsi. Membuktikan frame + crypto + session
    /// bekerja end-to-end tanpa TUI/mDNS.
    #[tokio::test]
    async fn lan_session_message_roundtrip() {
        let server = NoiseKey::generate();
        let client = NoiseKey::generate();
        let server_sk = server.secret_bytes();
        let server_pk = server.public_bytes();
        let client_sk = client.secret_bytes();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Responder
        let (resp_out_tx, resp_out_rx) = mpsc::unbounded_channel::<String>();
        let (resp_ev_tx, mut resp_ev_rx) = mpsc::unbounded_channel::<SessionEvent>();
        let resp = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            run_session(
                stream,
                Role::Responder,
                server_sk,
                None,
                resp_out_rx,
                resp_ev_tx,
            )
            .await
        });

        // Initiator
        let (init_out_tx, init_out_rx) = mpsc::unbounded_channel::<String>();
        let (init_ev_tx, mut init_ev_rx) = mpsc::unbounded_channel::<SessionEvent>();
        let init = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            run_session(
                stream,
                Role::Initiator,
                client_sk,
                Some(server_pk),
                init_out_rx,
                init_ev_tx,
            )
            .await
        });

        // Initiator → Responder
        init_out_tx.send("halo dari client".to_string()).unwrap();
        assert_eq!(wait_for_message(&mut resp_ev_rx).await, "halo dari client");

        // Responder → Initiator
        resp_out_tx.send("balasan server".to_string()).unwrap();
        assert_eq!(wait_for_message(&mut init_ev_rx).await, "balasan server");

        // Tutup kedua sisi
        drop(init_out_tx);
        drop(resp_out_tx);
        let _ = resp.await;
        let _ = init.await;
    }

    /// Responder menolak handshake bila remote static key tidak cocok dengan
    /// kontak yang diharapkan (fail closed — SEC, mencegah impersonation).
    #[tokio::test]
    async fn responder_rejects_unknown_peer() {
        let server = NoiseKey::generate();
        let client = NoiseKey::generate();
        let wrong = NoiseKey::generate(); // identitas yang TIDAK diharapkan

        let server_sk = server.secret_bytes();
        let server_pk = server.public_bytes();
        let client_sk = client.secret_bytes();
        let wrong_pk = wrong.public_bytes();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (_resp_out_tx, resp_out_rx) = mpsc::unbounded_channel::<String>();
        let (resp_ev_tx, _resp_ev_rx) = mpsc::unbounded_channel::<SessionEvent>();
        let resp = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // Responder mengharapkan `wrong_pk`, tapi client memakai `client_sk`.
            run_session(
                stream,
                Role::Responder,
                server_sk,
                Some(wrong_pk),
                resp_out_rx,
                resp_ev_tx,
            )
            .await
        });

        let (_init_out_tx, init_out_rx) = mpsc::unbounded_channel::<String>();
        let (init_ev_tx, _init_ev_rx) = mpsc::unbounded_channel::<SessionEvent>();
        let init = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            run_session(
                stream,
                Role::Initiator,
                client_sk,
                Some(server_pk),
                init_out_rx,
                init_ev_tx,
            )
            .await
        });

        let resp_result = resp.await.unwrap();
        assert!(
            matches!(resp_result, Err(Error::IdentityMismatch)),
            "responder harus menolak peer yang tidak dikenal"
        );
        let _ = init.await;
    }
}
