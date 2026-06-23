//! Contact store dan invite code.
//!
//! Kontak = identitas peer yang sudah dikenal. Untuk M1 (LAN) kontak hanya
//! butuh dua public key:
//!   - `ed25519_pub`  → identitas / fingerprint (yang dipublikasikan via mDNS TXT)
//!   - `noise_pub`    → X25519 static key untuk Noise_IK handshake
//!
//! Invite code menggabungkan keduanya menjadi satu string yang dibagikan
//! out-of-band (bukan lewat aplikasi). Onion address akan ditambahkan di M2.
//!
//! Catatan M1: contact store hidup di RAM saja, tidak dipersist ke disk.
//! Ini menjaga zero-trace (tidak ada social graph di disk), dengan trade-off
//! user perlu menukar invite code tiap sesi. Persistensi terenkripsi (memperluas
//! vault) adalah follow-up M1.x/M2.

use std::path::Path;

use base64::Engine;
use blake2::{Blake2s256, Digest};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::rngs::OsRng;

use crate::error::Error;
use crate::identity::keypair::KeyBundle;

/// Satu kontak yang dikenal.
#[derive(Clone)]
pub struct Contact {
    pub nickname: String,
    pub ed25519_pub: [u8; 32],
    pub noise_pub: [u8; 32],
    /// Onion address peer (mis. `xxxx.onion`). None = kontak LAN-only.
    pub onion: Option<String>,
}

/// Fingerprint identitas = hex dari Ed25519 public key.
/// Dipakai untuk matching saat discovery dan untuk menentukan role handshake.
pub fn fingerprint(ed25519_pub: &[u8; 32]) -> String {
    hex::encode(ed25519_pub)
}

/// Encode invite code: `base64(ed25519_pub || noise_pub)` dengan onion address
/// opsional di-append sebagai `...@xxxx.onion`.
/// URL-safe, tanpa padding, tanpa magic prefix (tidak self-identifying).
pub fn encode_invite(ed25519_pub: &[u8; 32], noise_pub: &[u8; 32], onion: Option<&str>) -> String {
    let mut raw = [0u8; 64];
    raw[..32].copy_from_slice(ed25519_pub);
    raw[32..].copy_from_slice(noise_pub);
    let keys = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    match onion {
        Some(o) => format!("{keys}@{o}"),
        None => keys,
    }
}

/// Decode invite code menjadi (ed25519_pub, noise_pub, onion_opsional).
pub fn decode_invite(code: &str) -> Result<([u8; 32], [u8; 32], Option<String>), Error> {
    let code = code.trim();
    let (keys_part, onion) = match code.split_once('@') {
        Some((k, o)) if !o.is_empty() => (k, Some(o.to_string())),
        _ => (code, None),
    };

    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(keys_part)
        .map_err(|_| Error::InvalidInvite)?;
    if raw.len() != 64 {
        return Err(Error::InvalidInvite);
    }
    let mut ed = [0u8; 32];
    let mut noise = [0u8; 32];
    ed.copy_from_slice(&raw[..32]);
    noise.copy_from_slice(&raw[32..]);
    Ok((ed, noise, onion))
}

// ───────────────────────── Persistensi terenkripsi ────────────────────────
//
// Kontak disimpan terenkripsi (ChaCha20Poly1305) dengan key diturunkan dari
// secret identity via BLAKE2s — social graph tidak tersimpan plaintext di disk.
// Format file: [12 nonce][ciphertext+tag], tanpa magic bytes.

const C_NONCE_LEN: usize = 12;

/// Turunkan key enkripsi kontak dari secret identity (cepat, bukan Argon2id —
/// ini bukan dari passphrase, melainkan dari key yang sudah ter-unlock).
pub fn derive_contacts_key(bundle: &KeyBundle) -> [u8; 32] {
    let mut h = Blake2s256::new();
    h.update(bundle.identity.secret_bytes());
    h.update(b"alter-contacts-key-v1");
    let digest = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn serialize_contacts(contacts: &[Contact]) -> String {
    let mut s = String::new();
    for c in contacts {
        let onion = c.onion.as_deref().unwrap_or("-");
        let nick = c.nickname.replace(['\n', '\r'], " ");
        s.push_str(&format!(
            "{} {} {} {}\n",
            hex::encode(c.ed25519_pub),
            hex::encode(c.noise_pub),
            onion,
            nick
        ));
    }
    s
}

fn parse_key32(hexstr: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(hexstr).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn deserialize_contacts(s: &str) -> Vec<Contact> {
    let mut out = Vec::new();
    for line in s.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut p = line.splitn(4, ' ');
        let (Some(ed), Some(noise), Some(onion), Some(nick)) =
            (p.next(), p.next(), p.next(), p.next())
        else {
            continue;
        };
        let (Some(ed25519_pub), Some(noise_pub)) = (parse_key32(ed), parse_key32(noise)) else {
            continue;
        };
        out.push(Contact {
            nickname: nick.to_string(),
            ed25519_pub,
            noise_pub,
            onion: if onion == "-" {
                None
            } else {
                Some(onion.to_string())
            },
        });
    }
    out
}

/// Simpan daftar kontak terenkripsi ke `path`.
pub fn save_contacts(path: &Path, contacts: &[Contact], key: &[u8; 32]) -> Result<(), Error> {
    let plaintext = serialize_contacts(contacts);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| Error::Encryption)?;
    let mut out = Vec::with_capacity(C_NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    std::fs::write(path, out).map_err(Error::Io)
}

/// Muat daftar kontak dari `path`. File tidak ada → daftar kosong.
pub fn load_contacts(path: &Path, key: &[u8; 32]) -> Result<Vec<Contact>, Error> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };
    if data.len() < C_NONCE_LEN + 16 {
        return Ok(Vec::new());
    }
    let (nonce_bytes, ct) = data.split_at(C_NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| Error::Decryption)?;
    Ok(deserialize_contacts(&String::from_utf8_lossy(&plaintext)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_roundtrip_lan_only() {
        let ed = [7u8; 32];
        let noise = [42u8; 32];
        let code = encode_invite(&ed, &noise, None);
        let (ed2, noise2, onion) = decode_invite(&code).unwrap();
        assert_eq!(ed, ed2);
        assert_eq!(noise, noise2);
        assert_eq!(onion, None);
    }

    #[test]
    fn invite_roundtrip_with_onion() {
        let ed = [7u8; 32];
        let noise = [42u8; 32];
        let onion = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuv2y3d.onion";
        let code = encode_invite(&ed, &noise, Some(onion));
        let (ed2, noise2, onion2) = decode_invite(&code).unwrap();
        assert_eq!(ed, ed2);
        assert_eq!(noise, noise2);
        assert_eq!(onion2.as_deref(), Some(onion));
    }

    #[test]
    fn invite_rejects_garbage() {
        assert!(decode_invite("bukan-invite-yang-valid!!!").is_err());
    }

    #[test]
    fn invite_rejects_wrong_length() {
        // base64 valid tapi decode jadi bukan 64 byte
        let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 10]);
        assert!(decode_invite(&short).is_err());
    }

    #[test]
    fn contacts_save_load_roundtrip() {
        let key = [9u8; 32];
        let contacts = vec![
            Contact {
                nickname: "Bob Sang Peretas".to_string(),
                ed25519_pub: [1u8; 32],
                noise_pub: [2u8; 32],
                onion: Some("abc.onion".to_string()),
            },
            Contact {
                nickname: "Alice".to_string(),
                ed25519_pub: [3u8; 32],
                noise_pub: [4u8; 32],
                onion: None,
            },
        ];
        let mut path = std::env::temp_dir();
        path.push(format!("alter-test-contacts-{}", std::process::id()));

        save_contacts(&path, &contacts, &key).unwrap();
        let loaded = load_contacts(&path, &key).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].nickname, "Bob Sang Peretas");
        assert_eq!(loaded[0].ed25519_pub, [1u8; 32]);
        assert_eq!(loaded[0].onion.as_deref(), Some("abc.onion"));
        assert_eq!(loaded[1].nickname, "Alice");
        assert_eq!(loaded[1].onion, None);
    }

    #[test]
    fn contacts_load_missing_file_is_empty() {
        let key = [5u8; 32];
        let path = std::path::Path::new("zzz-tidak-ada-file-kontak-xyz.dat");
        assert!(load_contacts(path, &key).unwrap().is_empty());
    }

    #[test]
    fn contacts_wrong_key_fails() {
        let contacts = vec![Contact {
            nickname: "X".into(),
            ed25519_pub: [1u8; 32],
            noise_pub: [2u8; 32],
            onion: None,
        }];
        let mut path = std::env::temp_dir();
        path.push(format!("alter-test-wrongkey-{}", std::process::id()));
        save_contacts(&path, &contacts, &[1u8; 32]).unwrap();
        let res = load_contacts(&path, &[2u8; 32]);
        std::fs::remove_file(&path).ok();
        assert!(res.is_err());
    }

    #[test]
    fn fingerprint_is_64_hex_chars() {
        let fp = fingerprint(&[0xAB; 32]);
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn invite_has_no_obvious_prefix() {
        // Dua identitas berbeda menghasilkan invite yang berbeda total,
        // tidak ada prefix konstan yang mengidentifikasi format.
        let a = encode_invite(&[1u8; 32], &[2u8; 32], None);
        let b = encode_invite(&[3u8; 32], &[4u8; 32], None);
        assert_ne!(&a[..4], &b[..4]);
    }
}
