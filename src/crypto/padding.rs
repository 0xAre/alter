//! Ciphertext padding (SEC-07) — pad plaintext ke kelipatan BLOCK sebelum
//! enkripsi untuk menyembunyikan panjang pesan asli dari analisis traffic.
//!
//! Format padded buffer: `[u16 LE: original_len][payload][zero fill ke block boundary]`.
//! Padding terjadi di layer session sebelum AEAD — verifier bisa mengaudit
//! tanpa memahami kriptografi Noise.

/// Ukuran blok padding dalam byte. Setiap plaintext yang dikirim selalu merupakan
/// kelipatan angka ini — observer hanya bisa melihat granularitas 256 byte.
pub const BLOCK: usize = 256;

/// Pad `data` ke kelipatan `BLOCK` dengan length-prefix 2 byte (u16 LE).
///
/// Ukuran hasil selalu ≥ BLOCK dan merupakan kelipatan BLOCK.
/// Overhead maksimum: BLOCK − 1 byte (worst case saat data.len() % BLOCK == 0
/// dan 2 header byte mendorong ke blok berikutnya).
pub fn pad(data: &[u8]) -> Vec<u8> {
    let needed = 2 + data.len(); // 2 byte untuk u16 length prefix
    let padded_len = needed.div_ceil(BLOCK) * BLOCK;
    let mut buf = vec![0u8; padded_len];
    buf[..2].copy_from_slice(&(data.len() as u16).to_le_bytes());
    buf[2..2 + data.len()].copy_from_slice(data);
    // Sisa byte sudah nol dari `vec![0u8; ...]`
    buf
}

/// Unpad buffer hasil `pad()` — kembalikan slice data asli.
///
/// Error bila buffer terlalu pendek atau length prefix menunjuk di luar buffer.
/// Pemanggil wajib treat error sebagai koneksi invalid (fail closed).
pub fn unpad(buf: &[u8]) -> Result<&[u8], ()> {
    if buf.len() < 2 {
        return Err(());
    }
    let orig_len = u16::from_le_bytes([buf[0], buf[1]]) as usize;
    if 2 + orig_len > buf.len() {
        return Err(());
    }
    Ok(&buf[2..2 + orig_len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_message_pads_to_one_block() {
        let padded = pad(&[]);
        assert_eq!(padded.len(), BLOCK);
        assert_eq!(unpad(&padded).unwrap(), &[] as &[u8]);
    }

    #[test]
    fn roundtrip_short_message() {
        let msg = b"halo dunia terenkripsi";
        let padded = pad(msg);
        assert_eq!(padded.len(), BLOCK);
        assert_eq!(unpad(&padded).unwrap(), msg);
    }

    #[test]
    fn roundtrip_exact_block_boundary() {
        // 254 byte = 2 (header) + 254 → padded = 256 (tepat 1 blok)
        let msg = vec![0x42u8; 254];
        let padded = pad(&msg);
        assert_eq!(padded.len(), BLOCK);
        assert_eq!(unpad(&padded).unwrap(), &msg[..]);
    }

    #[test]
    fn message_spanning_two_blocks() {
        // 255 byte: 2 + 255 = 257 → div_ceil(257, 256) = 2 blok = 512 byte
        let msg = vec![0xAAu8; 255];
        let padded = pad(&msg);
        assert_eq!(padded.len(), 2 * BLOCK);
        assert_eq!(unpad(&padded).unwrap(), &msg[..]);
    }

    #[test]
    fn padded_len_always_multiple_of_block() {
        for len in [0usize, 1, 100, 253, 254, 255, 256, 500, 1000, 10_000, 60_000] {
            let msg = vec![0u8; len];
            let padded = pad(&msg);
            assert_eq!(padded.len() % BLOCK, 0, "panjang {len} tidak menghasilkan kelipatan blok");
        }
    }

    #[test]
    fn padding_bytes_are_zero() {
        let msg = b"x";
        let padded = pad(msg);
        // byte dari 3 hingga akhir harus nol (padding)
        assert!(padded[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn unpad_truncated_buffer_errors() {
        assert!(unpad(&[]).is_err());
        assert!(unpad(&[1u8]).is_err());
        // length prefix = 10, tapi hanya 2 byte tersedia (tidak ada payload)
        assert!(unpad(&[10u8, 0u8]).is_err());
    }

    #[test]
    fn unpad_zero_length_payload() {
        // u16 LE = 0, sisa nol → data kosong valid
        let buf = [0u8; BLOCK];
        assert_eq!(unpad(&buf).unwrap(), &[] as &[u8]);
    }

    #[test]
    fn large_message_roundtrip() {
        let msg: Vec<u8> = (0u8..=255).cycle().take(50_000).collect();
        let padded = pad(&msg);
        assert_eq!(padded.len() % BLOCK, 0);
        assert_eq!(unpad(&padded).unwrap(), &msg[..]);
    }
}
