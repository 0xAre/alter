//! Session state machine — "Room" dalam terminologi PRD v0.3.
//!
//! Lifecycle: Connecting → Handshaking → Active → Closed.
//!
//! Saat Closed, `EncryptedSession` (yang membungkus `snow::TransportState`)
//! di-drop; snow menghapus key transport secara internal. Ini SEC-03a:
//! session key tidak bisa dipulihkan setelah room ditutup — ephemeral-by-design,
//! bukan ephemeral-by-policy.

pub(crate) mod file_transfer;

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::crypto::handshake::HandshakeSession;
use crate::error::Error;
use crate::transport::frame::{read_frame, write_frame, MAX_FRAME_LEN};
use crate::transport::Role;

/// Perintah dari UI ke session task.
pub enum SessionCmd {
    Text(String),
    SendFile {
        header: file_transfer::FileHeader,
        data:   Vec<u8>,
    },
}

// FT-01: byte pertama setiap frame post-handshake menentukan tipe konten.
const TYPE_TEXT: u8 = 0x00; // chat plaintext
const TYPE_CAP:  u8 = 0x05; // capability negotiation

const CAP_JSON: &str = r#"{"version":2,"features":["file_transfer"]}"#;

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
    /// FT-01: kemampuan peer setelah capability negotiation.
    PeerCapable { file_transfer: bool },
    /// FT-01: progress transfer file yang sedang diterima.
    FileProgress { name: String, received: u64, total: u64 },
    /// FT-01: file berhasil diterima dan diverifikasi; data siap disimpan.
    FileReceived { name: String, data: Vec<u8> },
    /// FT-01: transfer gagal (timeout, SHA-256 mismatch, peer cancel).
    FileError(String),
    /// FT-01: semua chunk file berhasil dikirim ke peer.
    FileSent { name: String },
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
    mut outgoing: UnboundedReceiver<SessionCmd>,
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

    // FT-01: kirim capability frame segera setelah handshake — tidak menunggu peer.
    {
        let mut cap = Vec::with_capacity(1 + CAP_JSON.len());
        cap.push(TYPE_CAP);
        cap.extend_from_slice(CAP_JSON.as_bytes());
        let padded = crate::crypto::padding::pad(&cap);
        let mut ct = vec![0u8; padded.len() + 16];
        let n = session.encrypt(&padded, &mut ct)?;
        write_frame(&mut wr, &ct[..n]).await?;
    }

    let mut ft_rx: Option<file_transfer::FileTransferState> = None;
    let mut ft_deadline: Option<tokio::time::Instant> = None;

    'active: loop {
        tokio::select! {
            maybe_cmd = outgoing.recv() => {
                match maybe_cmd {
                    Some(SessionCmd::Text(text)) => {
                        let mut msg = Vec::with_capacity(1 + text.len());
                        msg.push(TYPE_TEXT);
                        msg.extend_from_slice(text.as_bytes());
                        let padded = crate::crypto::padding::pad(&msg);
                        let mut ct = vec![0u8; padded.len() + 16];
                        let n = session.encrypt(&padded, &mut ct)?;
                        if write_frame(&mut wr, &ct[..n]).await.is_err() {
                            break 'active;
                        }
                    }
                    Some(SessionCmd::SendFile { header, data }) => {
                        let name = header.name.clone();

                        // FileHeader frame: 0x01 | JSON
                        let fh_json = match serde_json::to_vec(&header) {
                            Ok(j) => j,
                            Err(_) => {
                                let _ = events.send(SessionEvent::FileError("Gagal encode FileHeader.".into()));
                                continue 'active;
                            }
                        };
                        let mut fh_frame = Vec::with_capacity(1 + fh_json.len());
                        fh_frame.push(file_transfer::TYPE_FH);
                        fh_frame.extend_from_slice(&fh_json);
                        {
                            let padded = crate::crypto::padding::pad(&fh_frame);
                            let mut ct = vec![0u8; padded.len() + 16];
                            let n = session.encrypt(&padded, &mut ct)?;
                            if write_frame(&mut wr, &ct[..n]).await.is_err() {
                                break 'active;
                            }
                        }

                        // Chunk frames: 0x02 | [idx_u32_BE] | chunk_data
                        for (i, chunk) in data.chunks(file_transfer::CHUNK_SIZE).enumerate() {
                            let mut cf = Vec::with_capacity(5 + chunk.len());
                            cf.push(file_transfer::TYPE_CHUNK);
                            cf.extend_from_slice(&(i as u32).to_be_bytes());
                            cf.extend_from_slice(chunk);
                            let padded = crate::crypto::padding::pad(&cf);
                            let mut ct = vec![0u8; padded.len() + 16];
                            let n = session.encrypt(&padded, &mut ct)?;
                            if write_frame(&mut wr, &ct[..n]).await.is_err() {
                                break 'active;
                            }
                        }

                        // FileEnd frame: 0x03
                        let end = [file_transfer::TYPE_END];
                        let padded = crate::crypto::padding::pad(&end);
                        let mut ct = vec![0u8; padded.len() + 16];
                        let n = session.encrypt(&padded, &mut ct)?;
                        if write_frame(&mut wr, &ct[..n]).await.is_err() {
                            break 'active;
                        }

                        let _ = events.send(SessionEvent::FileSent { name });
                    }
                    None => break 'active, // UI menutup room
                }
            }
            res = read_frame(&mut rd, &mut buf) => {
                let n = match res {
                    Ok(n) => n,
                    Err(_) => break 'active,
                };
                if n == 0 {
                    let _ = events.send(SessionEvent::PeerLeft);
                    break 'active;
                }
                let mut pt = vec![0u8; n];
                match session.decrypt(&buf[..n], &mut pt) {
                    Ok(m) => {
                        match crate::crypto::padding::unpad(&pt[..m]) {
                            Ok(data) => {
                                if data.is_empty() { break 'active; }
                                match data[0] {
                                    TYPE_TEXT => {
                                        let text = String::from_utf8_lossy(&data[1..]).to_string();
                                        let _ = events.send(SessionEvent::Message(text));
                                    }
                                    TYPE_CAP => {
                                        let ft = serde_json::from_slice::<serde_json::Value>(&data[1..])
                                            .ok()
                                            .and_then(|v| {
                                                v["features"].as_array().map(|arr| {
                                                    arr.iter().any(|f| f.as_str() == Some("file_transfer"))
                                                })
                                            })
                                            .unwrap_or(false);
                                        let _ = events.send(SessionEvent::PeerCapable { file_transfer: ft });
                                    }
                                    file_transfer::TYPE_FH => {
                                        if let Ok(header) = serde_json::from_slice::<file_transfer::FileHeader>(&data[1..]) {
                                            let secs = 30u64.saturating_add(header.total_bytes.div_ceil(10_240));
                                            ft_deadline = Some(tokio::time::Instant::now() + Duration::from_secs(secs));
                                            let _ = events.send(SessionEvent::FileProgress {
                                                name: header.name.clone(),
                                                received: 0,
                                                total: header.total_bytes,
                                            });
                                            ft_rx = Some(file_transfer::FileTransferState::new(header));
                                        }
                                    }
                                    file_transfer::TYPE_CHUNK => {
                                        if data.len() >= 5 {
                                            if let Some(state) = &mut ft_rx {
                                                let idx = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
                                                let chunk = data[5..].to_vec();
                                                let received = ((idx as u64 + 1) * file_transfer::CHUNK_SIZE as u64)
                                                    .min(state.header.total_bytes);
                                                let name = state.header.name.clone();
                                                let total = state.header.total_bytes;
                                                state.receive_chunk(idx, chunk);
                                                let _ = events.send(SessionEvent::FileProgress { name, received, total });
                                            }
                                        }
                                    }
                                    file_transfer::TYPE_END => {
                                        if let Some(state) = ft_rx.take() {
                                            ft_deadline = None;
                                            let name = state.header.name.clone();
                                            match state.verify_and_finalize() {
                                                Ok(file_data) => {
                                                    let _ = events.send(SessionEvent::FileReceived { name, data: file_data });
                                                }
                                                Err(e) => {
                                                    let _ = events.send(SessionEvent::FileError(e));
                                                }
                                            }
                                        }
                                    }
                                    file_transfer::TYPE_CANCEL => {
                                        ft_rx = None;
                                        ft_deadline = None;
                                        let _ = events.send(SessionEvent::FileError("Peer membatalkan transfer.".into()));
                                    }
                                    _ => {} // tipe tidak dikenal — abaikan (forward compat)
                                }
                            }
                            Err(()) => break 'active, // padding invalid → fail closed
                        }
                    }
                    Err(_) => break 'active, // dekripsi gagal → fail closed
                }
            }
            _ = async {
                match ft_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                ft_rx = None;
                ft_deadline = None;
                let _ = events.send(SessionEvent::FileError("Transfer melewati batas waktu.".into()));
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
        let (resp_out_tx, resp_out_rx) = mpsc::unbounded_channel::<SessionCmd>();
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
        let (init_out_tx, init_out_rx) = mpsc::unbounded_channel::<SessionCmd>();
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
        init_out_tx.send(SessionCmd::Text("halo dari client".to_string())).unwrap();
        assert_eq!(wait_for_message(&mut resp_ev_rx).await, "halo dari client");

        // Responder → Initiator
        resp_out_tx.send(SessionCmd::Text("balasan server".to_string())).unwrap();
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

        let (_resp_out_tx, resp_out_rx) = mpsc::unbounded_channel::<SessionCmd>();
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

        let (_init_out_tx, init_out_rx) = mpsc::unbounded_channel::<SessionCmd>();
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
