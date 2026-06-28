//! Contact store dan invite code.
//!
//! M5 (SEC-13): Invite code v2 (96 bytes) — menyertakan `client_auth_pub`
//! untuk Tor restricted discovery. Format v1 (64 bytes) ditolak secara eksplisit
//! (fail-closed): pengguna harus menukar ulang invite code setelah upgrade.
//!
//! Format invite v2: `base64url_nopad(ed[32]||noise[32]||client_auth_pub[32])@xxxx.onion`
//! Backward compat file kontak: field ke-5 opsional (kontak lama → None).

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
    /// Pubkey client auth Tor peer (dari invite v2). None = kontak legacy/LAN-only.
    /// Dipakai untuk mengkonfigurasi restricted discovery di onion service kita.
    pub tor_client_auth_pub: Option<[u8; 32]>,
}

/// Fingerprint identitas = hex dari Ed25519 public key.
pub fn fingerprint(ed25519_pub: &[u8; 32]) -> String {
    hex::encode(ed25519_pub)
}

/// Turunkan x25519 client auth public key dari identity bundle secara deterministik.
///
/// KDF: BLAKE2s(identity_secret || "alter-tor-client-auth-v1") → 32-byte secret seed
/// → x25519 public key. Tidak mengubah format vault — kunci ini selalu bisa
/// diderivasi ulang dari identity key yang sama.
pub fn derive_tor_client_auth_pub(bundle: &KeyBundle) -> [u8; 32] {
    let secret_seed = derive_tor_client_auth_secret_seed(bundle);
    let secret = x25519_dalek::StaticSecret::from(secret_seed);
    x25519_dalek::PublicKey::from(&secret).to_bytes()
}

/// Bagian privat dari client auth keypair (untuk injeksi ke arti keystore).
pub fn derive_tor_client_auth_secret_seed(bundle: &KeyBundle) -> [u8; 32] {
    let mut h = Blake2s256::new();
    h.update(bundle.identity.secret_bytes());
    h.update(b"alter-tor-client-auth-v1");
    let digest = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Encode invite code v2: `base64url_nopad(ed[32]||noise[32]||client_auth_pub[32])[@onion]`
///
/// Total 96 byte → ~128 karakter base64url (tanpa padding).
pub fn encode_invite(
    ed25519_pub: &[u8; 32],
    noise_pub: &[u8; 32],
    client_auth_pub: &[u8; 32],
    onion: Option<&str>,
) -> String {
    let mut raw = [0u8; 96];
    raw[..32].copy_from_slice(ed25519_pub);
    raw[32..64].copy_from_slice(noise_pub);
    raw[64..96].copy_from_slice(client_auth_pub);
    let keys = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    match onion {
        Some(o) => format!("{keys}@{o}"),
        None => keys,
    }
}

/// Decode invite code v2 menjadi (ed25519_pub, noise_pub, client_auth_pub, onion).
///
/// Format v1 (64 byte) DITOLAK secara eksplisit (fail-closed per PRD): user harus
/// menukar ulang invite code agar restricted discovery aktif bagi semua kontak.
pub fn decode_invite(
    code: &str,
) -> Result<([u8; 32], [u8; 32], Option<[u8; 32]>, Option<String>), Error> {
    let code = code.trim();
    let (keys_part, onion) = match code.split_once('@') {
        Some((k, o)) if !o.is_empty() => (k, Some(o.to_string())),
        _ => (code, None),
    };

    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(keys_part)
        .map_err(|_| Error::InvalidInvite)?;

    match raw.len() {
        96 => {
            // v2: ed[32] || noise[32] || client_auth_pub[32]
            let mut ed = [0u8; 32];
            let mut noise = [0u8; 32];
            let mut cap = [0u8; 32];
            ed.copy_from_slice(&raw[..32]);
            noise.copy_from_slice(&raw[32..64]);
            cap.copy_from_slice(&raw[64..96]);
            Ok((ed, noise, Some(cap), onion))
        }
        64 => {
            // v1 legacy — tolak. Pengguna harus menukar invite code baru.
            // Ini adalah keputusan eksplisit (fail-closed): kontak lama tanpa
            // client_auth_pub tidak mendapat restricted discovery protection.
            Err(Error::InvalidInvite)
        }
        _ => Err(Error::InvalidInvite),
    }
}

// ───────────────────────── Persistensi terenkripsi ────────────────────────
//
// Format file: [12 nonce][ciphertext+tag]
// Format baris kontak: "{ed_hex} {noise_hex} {onion_or_dash} {cap_hex_or_dash} {nickname}\n"
// Backward compat: baris 4-field (tanpa cap) masih diparse → cap = None.

const C_NONCE_LEN: usize = 12;

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
        let cap = c
            .tor_client_auth_pub
            .as_ref()
            .map(hex::encode)
            .unwrap_or_else(|| "-".to_string());
        let nick = c.nickname.replace(['\n', '\r'], " ");
        s.push_str(&format!(
            "{} {} {} {} {}\n",
            hex::encode(c.ed25519_pub),
            hex::encode(c.noise_pub),
            onion,
            cap,
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
        // Field 5: ed noise onion cap nick (v2 format)
        // Field 4: ed noise onion nick     (v1 backward compat)
        let mut p = line.splitn(5, ' ');
        let (Some(ed), Some(noise), Some(onion_s), Some(fourth)) =
            (p.next(), p.next(), p.next(), p.next())
        else {
            continue;
        };
        let (Some(ed25519_pub), Some(noise_pub)) = (parse_key32(ed), parse_key32(noise)) else {
            continue;
        };

        // Disambiguasi v2 (5 field: ed noise onion cap nick) vs v1 (4 field: ed noise onion nick).
        // Cap field selalu "-" atau tepat 64 hex char (hex::encode [u8;32]).
        // Nickname lama sangat tidak mungkin cocok dengan pola ini.
        let (tor_client_auth_pub, nickname) = if let Some(nick_rest) = p.next() {
            let is_cap = fourth == "-"
                || (fourth.len() == 64 && fourth.bytes().all(|b| b.is_ascii_hexdigit()));
            if is_cap {
                let cap = if fourth == "-" { None } else { parse_key32(fourth) };
                (cap, nick_rest.to_string())
            } else {
                // Format lama: fourth adalah bagian nickname yang mengandung spasi
                (None, format!("{} {}", fourth, nick_rest))
            }
        } else {
            // 4 field tanpa spasi di nickname: fourth = nickname lengkap
            (None, fourth.to_string())
        };

        out.push(Contact {
            nickname,
            ed25519_pub,
            noise_pub,
            onion: if onion_s == "-" {
                None
            } else {
                Some(onion_s.to_string())
            },
            tor_client_auth_pub,
        });
    }
    out
}

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
    use crate::identity::keypair::{IdentityKey, NoiseKey};

    fn dummy_bundle() -> KeyBundle {
        KeyBundle {
            identity: IdentityKey::from_secret_bytes([0x42u8; 32]),
            noise: NoiseKey::from_secret_bytes([0x11u8; 32]),
        }
    }

    #[test]
    fn invite_v2_roundtrip_lan_only() {
        let ed = [7u8; 32];
        let noise = [42u8; 32];
        let cap = [99u8; 32];
        let code = encode_invite(&ed, &noise, &cap, None);
        let (ed2, noise2, cap2, onion) = decode_invite(&code).unwrap();
        assert_eq!(ed, ed2);
        assert_eq!(noise, noise2);
        assert_eq!(cap2, Some(cap));
        assert_eq!(onion, None);
    }

    #[test]
    fn invite_v2_roundtrip_with_onion() {
        let ed = [7u8; 32];
        let noise = [42u8; 32];
        let cap = [55u8; 32];
        let onion = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuv2y3d.onion";
        let code = encode_invite(&ed, &noise, &cap, Some(onion));
        let (ed2, noise2, cap2, onion2) = decode_invite(&code).unwrap();
        assert_eq!(ed, ed2);
        assert_eq!(noise, noise2);
        assert_eq!(cap2, Some(cap));
        assert_eq!(onion2.as_deref(), Some(onion));
    }

    #[test]
    fn invite_v1_rejected_fail_closed() {
        // Format lama (64 byte) harus DITOLAK — fail-closed per PRD M5.
        let ed = [7u8; 32];
        let noise = [42u8; 32];
        let mut raw = [0u8; 64];
        raw[..32].copy_from_slice(&ed);
        raw[32..].copy_from_slice(&noise);
        let v1_code = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
        assert!(decode_invite(&v1_code).is_err(), "v1 invite harus ditolak");
    }

    #[test]
    fn invite_rejects_garbage() {
        assert!(decode_invite("bukan-invite-yang-valid!!!").is_err());
    }

    #[test]
    fn invite_rejects_wrong_length() {
        let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 10]);
        assert!(decode_invite(&short).is_err());
    }

    #[test]
    fn derive_tor_client_auth_pub_deterministic() {
        let bundle = dummy_bundle();
        let pub1 = derive_tor_client_auth_pub(&bundle);
        let pub2 = derive_tor_client_auth_pub(&bundle);
        assert_eq!(pub1, pub2, "derivasi harus deterministik");
        assert_ne!(pub1, [0u8; 32], "harus non-zero");
    }

    #[test]
    fn derive_tor_client_auth_pub_changes_with_identity() {
        let b1 = KeyBundle {
            identity: IdentityKey::from_secret_bytes([0x01u8; 32]),
            noise: NoiseKey::from_secret_bytes([0x11u8; 32]),
        };
        let b2 = KeyBundle {
            identity: IdentityKey::from_secret_bytes([0x02u8; 32]),
            noise: NoiseKey::from_secret_bytes([0x11u8; 32]),
        };
        assert_ne!(
            derive_tor_client_auth_pub(&b1),
            derive_tor_client_auth_pub(&b2),
        );
    }

    #[test]
    fn contacts_save_load_roundtrip_v2() {
        let key = [9u8; 32];
        let contacts = vec![
            Contact {
                nickname: "Bob".to_string(),
                ed25519_pub: [1u8; 32],
                noise_pub: [2u8; 32],
                onion: Some("abc.onion".to_string()),
                tor_client_auth_pub: Some([5u8; 32]),
            },
            Contact {
                nickname: "Alice".to_string(),
                ed25519_pub: [3u8; 32],
                noise_pub: [4u8; 32],
                onion: None,
                tor_client_auth_pub: None,
            },
        ];
        let mut path = std::env::temp_dir();
        path.push(format!("alter-test-contacts-v2-{}", std::process::id()));

        save_contacts(&path, &contacts, &key).unwrap();
        let loaded = load_contacts(&path, &key).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].nickname, "Bob");
        assert_eq!(loaded[0].ed25519_pub, [1u8; 32]);
        assert_eq!(loaded[0].onion.as_deref(), Some("abc.onion"));
        assert_eq!(loaded[0].tor_client_auth_pub, Some([5u8; 32]));
        assert_eq!(loaded[1].nickname, "Alice");
        assert_eq!(loaded[1].tor_client_auth_pub, None);
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
            tor_client_auth_pub: None,
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
        let fp = fingerprint(&[0xABu8; 32]);
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn invite_has_no_obvious_prefix() {
        let cap = [0u8; 32];
        let a = encode_invite(&[1u8; 32], &[2u8; 32], &cap, None);
        let b = encode_invite(&[3u8; 32], &[4u8; 32], &cap, None);
        assert_ne!(&a[..4], &b[..4]);
    }

    #[test]
    fn legacy_nickname_with_spaces_backward_compat() {
        // Format lama (4 field): nickname bisa mengandung spasi.
        // Parser baru harus mengembalikan nickname lengkap, bukan cuma kata pertama.
        let ed = [0xAAu8; 32];
        let noise = [0xBBu8; 32];
        let line = format!("{} {} - Nick with Spaces\n", hex::encode(ed), hex::encode(noise));
        let contacts = deserialize_contacts(&line);
        assert_eq!(contacts.len(), 1, "harus parse satu kontak");
        assert_eq!(contacts[0].nickname, "Nick with Spaces");
        assert!(contacts[0].tor_client_auth_pub.is_none());
    }

    #[test]
    fn v2_format_with_spaces_in_nickname() {
        // Format baru (5 field): nickname dengan spasi harus tetap benar.
        let ed = [0x11u8; 32];
        let noise = [0x22u8; 32];
        let cap = [0x33u8; 32];
        let line = format!(
            "{} {} some.onion {} Alice Blue\n",
            hex::encode(ed),
            hex::encode(noise),
            hex::encode(cap),
        );
        let contacts = deserialize_contacts(&line);
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].nickname, "Alice Blue");
        assert_eq!(contacts[0].tor_client_auth_pub, Some(cap));
    }
}
