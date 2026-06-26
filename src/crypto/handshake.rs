/// Noise_IK handshake untuk ALTER.
///
/// Pattern: Noise_IK_25519_ChaChaPoly_BLAKE2s
///
/// IK berarti:
///   I = initiator mengirim static key di message pertama (0-RTT identity)
///   K = responder's static key sudah diketahui initiator sejak awal (dari contact code)
///
/// Alur 2-message:
///   -> e, es, s, ss     (initiator ke responder — 1 RTT)
///   <- e, ee, se        (responder ke initiator)
///   [selesai — kedua sisi punya TransportState]
///
/// Properti keamanan:
///   - Mutual authentication: kedua pihak saling memverifikasi static key
///   - Forward secrecy: dari ephemeral keys (e)
///   - Identity hiding: static key initiator dienkripsi (es) sebelum dikirim
use snow::{Builder, HandshakeState, TransportState};
use crate::error::Error;

const NOISE_PATTERN: &str = "Noise_IK_25519_ChaChaPoly_BLAKE2s";

// SEC-08: payload di setiap write_message selalu &[] (kosong). Noise pattern
// string di atas hanya dipakai untuk inisialisasi snow — TIDAK dikirim lewat
// jaringan. snow tidak embed version/identifier di transport messages.

/// State machine handshake — wrapper tipis di atas snow::HandshakeState.
pub struct HandshakeSession {
    state: HandshakeState,
}

impl HandshakeSession {
    /// Buat sesi sebagai initiator.
    /// `local_noise_privkey` = X25519 private key kita (dari KeyBundle.noise)
    /// `peer_noise_pubkey`   = X25519 public key peer (dari contact code)
    pub fn new_initiator(local_noise_privkey: &[u8; 32], peer_noise_pubkey: &[u8; 32]) -> Result<Self, Error> {
        let state = Builder::new(NOISE_PATTERN.parse()?)
            .local_private_key(local_noise_privkey)?
            .remote_public_key(peer_noise_pubkey)?
            .build_initiator()?;
        Ok(Self { state })
    }

    /// Buat sesi sebagai responder.
    /// `local_noise_privkey` = X25519 private key kita (dari KeyBundle.noise)
    /// Responder tidak perlu tahu peer's pubkey di awal — akan diverifikasi dari message 1.
    pub fn new_responder(local_noise_privkey: &[u8; 32]) -> Result<Self, Error> {
        let state = Builder::new(NOISE_PATTERN.parse()?)
            .local_private_key(local_noise_privkey)?
            .build_responder()?;
        Ok(Self { state })
    }

    /// Tulis handshake message berikutnya ke `out`.
    /// Kembalikan jumlah bytes yang ditulis.
    pub fn write_message(&mut self, payload: &[u8], out: &mut [u8]) -> Result<usize, Error> {
        self.state.write_message(payload, out).map_err(Error::Noise)
    }

    /// Proses handshake message yang diterima dari peer.
    /// `buf` = buffer untuk payload yang mungkin ada di message.
    pub fn read_message(&mut self, input: &[u8], buf: &mut [u8]) -> Result<usize, Error> {
        self.state.read_message(input, buf).map_err(Error::Noise)
    }



    /// Pindah ke transport mode setelah handshake selesai.
    /// Memanggil ini sebelum handshake selesai akan menghasilkan error.
    pub fn into_transport(self) -> Result<EncryptedSession, Error> {
        let transport = self.state.into_transport_mode().map_err(Error::Noise)?;
        Ok(EncryptedSession { state: transport })
    }

    /// Setelah handshake selesai, ambil remote static pubkey yang sudah diverifikasi.
    /// Ini yang dipakai untuk memverifikasi bahwa kita terhubung ke contact yang benar.
    pub fn remote_static(&self) -> Option<&[u8]> {
        self.state.get_remote_static()
    }
}

/// Session aktif setelah handshake selesai — wrapper tipis di atas snow::TransportState.
/// Semua pesan yang lewat sini terenkripsi dan terauthentikasi.
pub struct EncryptedSession {
    state: TransportState,
}

impl EncryptedSession {
    /// Enkripsi `plaintext` ke `out`. Kembalikan jumlah bytes ciphertext.
    /// `out` harus punya kapasitas setidaknya `plaintext.len() + 16` (overhead AEAD).
    pub fn encrypt(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, Error> {
        self.state.write_message(plaintext, out).map_err(Error::Noise)
    }

    /// Dekripsi `ciphertext` ke `out`. Kembalikan jumlah bytes plaintext.
    pub fn decrypt(&mut self, ciphertext: &[u8], out: &mut [u8]) -> Result<usize, Error> {
        self.state.read_message(ciphertext, out).map_err(Error::Noise)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keypair::NoiseKey;

    const HS_BUF: usize = 65535;

    fn do_handshake(
        initiator: &mut HandshakeSession,
        responder: &mut HandshakeSession,
    ) -> Result<(), Error> {
        // Message 1: Initiator → Responder
        let mut msg1 = vec![0u8; HS_BUF];
        let len1 = initiator.write_message(&[], &mut msg1)?;

        let mut buf = vec![0u8; HS_BUF];
        responder.read_message(&msg1[..len1], &mut buf)?;

        // Message 2: Responder → Initiator
        let mut msg2 = vec![0u8; HS_BUF];
        let len2 = responder.write_message(&[], &mut msg2)?;

        initiator.read_message(&msg2[..len2], &mut buf)?;

        Ok(())
    }

    #[test]
    fn handshake_ik_roundtrip() {
        let alice_noise = NoiseKey::generate();
        let bob_noise = NoiseKey::generate();

        let mut alice = HandshakeSession::new_initiator(
            &alice_noise.secret_bytes(),
            &bob_noise.public_bytes(),
        )
        .unwrap();

        let mut bob = HandshakeSession::new_responder(&bob_noise.secret_bytes()).unwrap();

        do_handshake(&mut alice, &mut bob).unwrap();
    }

    #[test]
    fn transport_encrypt_decrypt_roundtrip() {
        let alice_noise = NoiseKey::generate();
        let bob_noise = NoiseKey::generate();

        let mut alice = HandshakeSession::new_initiator(
            &alice_noise.secret_bytes(),
            &bob_noise.public_bytes(),
        )
        .unwrap();
        let mut bob = HandshakeSession::new_responder(&bob_noise.secret_bytes()).unwrap();

        do_handshake(&mut alice, &mut bob).unwrap();

        let mut alice_sess = alice.into_transport().unwrap();
        let mut bob_sess = bob.into_transport().unwrap();

        let plaintext = b"hello from alice";
        let mut ciphertext = vec![0u8; plaintext.len() + 16];
        let ct_len = alice_sess.encrypt(plaintext, &mut ciphertext).unwrap();

        let mut decrypted = vec![0u8; plaintext.len()];
        let pt_len = bob_sess.decrypt(&ciphertext[..ct_len], &mut decrypted).unwrap();

        assert_eq!(&decrypted[..pt_len], plaintext);
    }

    #[test]
    fn responder_verifies_initiator_identity() {
        let alice_noise = NoiseKey::generate();
        let bob_noise = NoiseKey::generate();

        let mut alice = HandshakeSession::new_initiator(
            &alice_noise.secret_bytes(),
            &bob_noise.public_bytes(),
        )
        .unwrap();
        let mut bob = HandshakeSession::new_responder(&bob_noise.secret_bytes()).unwrap();

        do_handshake(&mut alice, &mut bob).unwrap();

        // Responder (bob) harus bisa melihat alice's static pubkey setelah handshake
        let remote_static = bob.remote_static().expect("remote static harus tersedia");
        assert_eq!(remote_static, alice_noise.public_bytes().as_slice());
    }

    #[test]
    fn wrong_peer_key_fails_handshake() {
        let alice_noise = NoiseKey::generate();
        let bob_noise = NoiseKey::generate();
        let eve_noise = NoiseKey::generate(); // attacker

        // Alice mengira terhubung ke Bob, tapi menggunakan Eve's pubkey sebagai responder
        let mut alice = HandshakeSession::new_initiator(
            &alice_noise.secret_bytes(),
            &eve_noise.public_bytes(), // SALAH — alice harusnya gunakan bob's key
        )
        .unwrap();

        let mut bob = HandshakeSession::new_responder(&bob_noise.secret_bytes()).unwrap();

        // Message 1: Alice mengirim — `es` DH pakai eve's pubkey, bukan bob's
        let mut msg1 = vec![0u8; HS_BUF];
        let len1 = alice.write_message(&[], &mut msg1).unwrap();

        let mut buf = vec![0u8; HS_BUF];
        // Bob tidak bisa dekripsi: `es` DH result tidak cocok (eve_key ≠ bob_key)
        // Noise_IK message 1 langsung gagal di sisi responder
        let result = bob.read_message(&msg1[..len1], &mut buf);
        assert!(result.is_err(), "handshake harus gagal saat peer key tidak cocok");
    }
}
