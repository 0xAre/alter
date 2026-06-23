//! Framing protocol untuk pesan di atas TCP.
//!
//! Setiap frame: `[2-byte big-endian length][payload]`.
//! Batas 2 byte = 65535 bytes per frame — kebetulan persis sama dengan batas
//! maksimum satu Noise message (spec Noise = 65535). Jadi satu frame = satu
//! Noise message, tidak perlu fragmentasi di layer ini.
//!
//! Fungsi-fungsi ini generic di atas `AsyncRead`/`AsyncWrite` supaya bisa diuji
//! dengan `tokio::io::duplex` tanpa socket nyata.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::Error;

/// Ukuran maksimum payload satu frame (batas Noise message).
pub const MAX_FRAME_LEN: usize = 65535;

/// Tulis satu frame ke `w`. Payload yang melebihi `MAX_FRAME_LEN` ditolak.
pub async fn write_frame<W>(w: &mut W, payload: &[u8]) -> Result<(), Error>
where
    W: AsyncWriteExt + Unpin,
{
    if payload.len() > MAX_FRAME_LEN {
        return Err(Error::FrameTooLarge);
    }
    let len = (payload.len() as u16).to_be_bytes();
    w.write_all(&len).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}

/// Baca satu frame dari `r` ke dalam `buf`.
///
/// Mengembalikan jumlah byte payload, atau `0` jika koneksi ditutup bersih
/// (EOF) — pemanggil menafsirkan 0 sebagai "peer keluar dari room".
pub async fn read_frame<R>(r: &mut R, buf: &mut [u8]) -> Result<usize, Error>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_bytes = [0u8; 2];
    match r.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        // EOF saat membaca length = koneksi ditutup bersih
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(0),
        Err(e) => return Err(Error::Io(e)),
    }

    let len = u16::from_be_bytes(len_bytes) as usize;
    if len > buf.len() {
        return Err(Error::FrameTooLarge);
    }

    r.read_exact(&mut buf[..len]).await.map_err(Error::Io)?;
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(4096);

        let payload = b"halo dunia terenkripsi";
        write_frame(&mut a, payload).await.unwrap();

        let mut buf = vec![0u8; MAX_FRAME_LEN];
        let n = read_frame(&mut b, &mut buf).await.unwrap();
        assert_eq!(&buf[..n], payload);
    }

    #[tokio::test]
    async fn multiple_frames_in_order() {
        let (mut a, mut b) = tokio::io::duplex(4096);

        write_frame(&mut a, b"satu").await.unwrap();
        write_frame(&mut a, b"dua").await.unwrap();

        let mut buf = vec![0u8; 64];
        let n1 = read_frame(&mut b, &mut buf).await.unwrap();
        assert_eq!(&buf[..n1], b"satu");
        let n2 = read_frame(&mut b, &mut buf).await.unwrap();
        assert_eq!(&buf[..n2], b"dua");
    }

    #[tokio::test]
    async fn eof_returns_zero() {
        let (a, mut b) = tokio::io::duplex(4096);
        drop(a); // tutup sisi penulis
        let mut buf = vec![0u8; 64];
        let n = read_frame(&mut b, &mut buf).await.unwrap();
        assert_eq!(n, 0, "EOF harus menghasilkan 0, bukan error");
    }

    #[tokio::test]
    async fn oversized_payload_rejected() {
        let (mut a, _b) = tokio::io::duplex(4096);
        let too_big = vec![0u8; MAX_FRAME_LEN + 1];
        assert!(matches!(
            write_frame(&mut a, &too_big).await,
            Err(Error::FrameTooLarge)
        ));
    }
}
