//! FT-01: buffer penerimaan chunk dan verifikasi integritas SHA-256.
#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Ukuran chunk: 60 KB per frame.
/// Batas diturunkan dari 64 KB agar ciphertext (padded + 16 byte AEAD tag) tetap
/// di bawah MAX_FRAME_LEN=65535. Dengan BLOCK=256: pad(7+61440)=61696, +16=61712. ✓
pub(crate) const CHUNK_SIZE: usize = 60 * 1024;

// Frame type bytes untuk FT-01 (dipakai di session/mod.rs).
pub(crate) const TYPE_FH:     u8 = 0x01;
pub(crate) const TYPE_CHUNK:  u8 = 0x02;
pub(crate) const TYPE_END:    u8 = 0x03;
pub(crate) const TYPE_CANCEL: u8 = 0x04;

/// Metadata file yang dikirim/diterima via FileHeader frame (0x01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHeader {
    pub name:        String,
    pub mime:        String,
    pub total_bytes: u64,
    pub chunk_count: u32,
    pub sha256:      String,
}

/// State buffer sisi penerima selama transfer berlangsung.
pub struct FileTransferState {
    pub header: FileHeader,
    chunks:     HashMap<u32, Vec<u8>>,
}

impl FileTransferState {
    pub fn new(header: FileHeader) -> Self {
        Self { header, chunks: HashMap::new() }
    }

    pub fn receive_chunk(&mut self, index: u32, data: Vec<u8>) {
        self.chunks.insert(index, data);
    }

    pub fn is_complete(&self) -> bool {
        self.chunks.len() as u32 == self.header.chunk_count
    }

    /// Reassemble semua chunk berurutan lalu verifikasi SHA-256.
    /// Mengembalikan data lengkap jika cocok, atau error jika tidak.
    pub fn verify_and_finalize(mut self) -> Result<Vec<u8>, String> {
        let mut data = Vec::with_capacity(self.header.total_bytes as usize);
        for i in 0..self.header.chunk_count {
            match self.chunks.remove(&i) {
                Some(chunk) => data.extend_from_slice(&chunk),
                None => {
                    data.zeroize();
                    return Err(format!("Chunk {i} hilang — transfer tidak lengkap."));
                }
            }
        }

        let computed = hex::encode(Sha256::digest(&data));
        if computed != self.header.sha256 {
            data.zeroize();
            return Err("Verifikasi SHA-256 gagal — data korup atau dimanipulasi.".into());
        }

        Ok(data)
    }

    pub fn cancel(&mut self) {
        for chunk in self.chunks.values_mut() {
            chunk.zeroize();
        }
        self.chunks.clear();
    }
}

impl Drop for FileTransferState {
    fn drop(&mut self) {
        self.cancel();
    }
}
