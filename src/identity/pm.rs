/// Password Manager data model — M6 (SEC-15).
///
/// `PmStore` di-serialisasi ke JSON, di-enkripsi ke slot A vault v2.
/// Format plaintext slot A (3900 bytes fixed):
///   [4 bytes LE u32: json_len] [json_data] [CSPRNG padding]
///
/// Maksimum JSON: 3896 bytes (~40–50 entri rata-rata).
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::Error;

/// Ukuran plaintext slot A vault v2 — selalu 3900 bytes.
pub const PM_PLAINTEXT_SIZE: usize = 3900;
/// 4 bytes dipakai untuk menyimpan panjang JSON.
const PM_JSON_LEN_PREFIX: usize = 4;
/// Maksimum bytes yang tersedia untuk JSON entries.
pub const PM_DATA_MAX: usize = PM_PLAINTEXT_SIZE - PM_JSON_LEN_PREFIX;

/// Maksimum backup codes per entry — standar industri (GitHub, Google).
pub const PM_CODES_MAX: usize = 10;

/// Satu backup / recovery code.
///
/// "c" = code, "u" = used flag. Field pendek untuk hemat vault space.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize)]
pub struct BackupCode {
    #[serde(rename = "c")]
    pub code: String,
    #[serde(rename = "u", default)]
    pub used: bool,
}

/// Satu entri password manager.
///
/// Nama field pendek di JSON (serde rename) untuk menghemat ruang di vault:
/// "s" = service, "u" = username, "p" = password, "k" = backup codes.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize)]
pub struct PmEntry {
    pub id: u64,
    #[serde(rename = "s")]
    pub service: String,
    #[serde(rename = "u")]
    pub username: String,
    #[serde(rename = "p")]
    pub password: String,
    /// Backup / recovery codes — None jika entry tidak punya codes.
    #[serde(rename = "k", skip_serializing_if = "Option::is_none", default)]
    pub codes: Option<Vec<BackupCode>>,
}

/// Koleksi semua PM entries — disimpan di slot A vault v2.
#[derive(Debug, Default, Serialize, Deserialize, Zeroize)]
pub struct PmStore {
    pub entries: Vec<PmEntry>,
}

impl PmStore {
    /// Serialisasi ke plaintext 3900 bytes untuk enkripsi.
    ///
    /// Format:
    ///   [4B LE u32: json_len]
    ///   [json_data: json_len bytes]
    ///   [CSPRNG padding: sisa bytes sampai 3900]
    pub fn to_plaintext(&self) -> Result<Vec<u8>, Error> {
        let json = serde_json::to_vec(self).map_err(|_| Error::PmFull)?;
        if json.len() > PM_DATA_MAX {
            return Err(Error::PmFull);
        }
        let mut out = Vec::with_capacity(PM_PLAINTEXT_SIZE);
        let json_len = json.len() as u32;
        out.extend_from_slice(&json_len.to_le_bytes());
        out.extend_from_slice(&json);
        // CSPRNG padding — ciphertext tidak akan punya pola zero
        let pad_len = PM_PLAINTEXT_SIZE - PM_JSON_LEN_PREFIX - json.len();
        let mut pad = vec![0u8; pad_len];
        OsRng.fill_bytes(&mut pad);
        out.extend_from_slice(&pad);
        debug_assert_eq!(out.len(), PM_PLAINTEXT_SIZE);
        Ok(out)
    }

    /// Deseralisasi dari plaintext slot A.
    ///
    /// Jika json_len tidak valid atau JSON corrupt: kembalikan store kosong (never-fail).
    pub fn from_plaintext(data: &[u8]) -> Self {
        if data.len() < PM_JSON_LEN_PREFIX {
            return Self::default();
        }
        let json_len = u32::from_le_bytes(
            data[..PM_JSON_LEN_PREFIX].try_into().unwrap(),
        ) as usize;
        if json_len > PM_DATA_MAX || PM_JSON_LEN_PREFIX + json_len > data.len() {
            return Self::default();
        }
        serde_json::from_slice(&data[PM_JSON_LEN_PREFIX..PM_JSON_LEN_PREFIX + json_len])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_empty() {
        let store = PmStore::default();
        let plain = store.to_plaintext().unwrap();
        assert_eq!(plain.len(), PM_PLAINTEXT_SIZE);
        let restored = PmStore::from_plaintext(&plain);
        assert!(restored.entries.is_empty());
    }

    #[test]
    fn roundtrip_entries() {
        let store = PmStore {
            entries: vec![
                PmEntry { id: 1, service: "github.com".into(), username: "alice".into(), password: "s3cr3t".into(), codes: None },
                PmEntry { id: 2, service: "protonmail.com".into(), username: "alice@pm.me".into(), password: "another".into(), codes: None },
            ],
        };
        let plain = store.to_plaintext().unwrap();
        assert_eq!(plain.len(), PM_PLAINTEXT_SIZE);
        let restored = PmStore::from_plaintext(&plain);
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].service, "github.com");
        assert_eq!(restored.entries[1].password, "another");
    }

    #[test]
    fn from_corrupt_plaintext_returns_default() {
        let corrupt = vec![0xff, 0xff, 0xff, 0xff]; // json_len too large
        let store = PmStore::from_plaintext(&corrupt);
        assert!(store.entries.is_empty());

        let empty_slice: &[u8] = &[];
        let store2 = PmStore::from_plaintext(empty_slice);
        assert!(store2.entries.is_empty());
    }

    #[test]
    fn plaintext_size_is_always_fixed() {
        // Berbeda jumlah entries → plaintext size tetap sama
        let s1 = PmStore::default();
        let s2 = PmStore { entries: vec![PmEntry { id: 1, service: "x".into(), username: "y".into(), password: "z".into(), codes: None }] };
        assert_eq!(s1.to_plaintext().unwrap().len(), PM_PLAINTEXT_SIZE);
        assert_eq!(s2.to_plaintext().unwrap().len(), PM_PLAINTEXT_SIZE);
    }
}
