use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};
use zeroize::ZeroizeOnDrop;

/// Ed25519 keypair — dipakai sebagai identity jangka panjang.
/// Pubkey ini yang dibagikan ke kontak lewat invite code.
///
/// ZeroizeOnDrop: secret key di-wipe dari memori saat struct ini di-drop.
#[derive(ZeroizeOnDrop)]
pub struct IdentityKey {
    #[zeroize(skip)] // VerifyingKey tidak mengandung secret
    verifying: VerifyingKey,
    signing: SigningKey,
}

impl IdentityKey {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        Self { verifying, signing }
    }

    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&bytes);
        let verifying = signing.verifying_key();
        Self { verifying, signing }
    }

    /// Raw 32-byte secret key — hanya boleh diakses untuk vault serialization.
    pub(crate) fn secret_bytes(&self) -> &[u8; 32] {
        self.signing.as_bytes()
    }

    pub fn public_key(&self) -> &VerifyingKey {
        &self.verifying
    }
}

/// X25519 keypair — dipakai untuk Noise_IK DH handshake.
/// Dipisah dari IdentityKey karena Ed25519 dan X25519 punya scalar space berbeda.
///
/// ZeroizeOnDrop: secret key di-wipe dari memori saat struct ini di-drop.
#[derive(ZeroizeOnDrop)]
pub struct NoiseKey {
    #[zeroize(skip)]
    public: X25519PublicKey,
    secret: X25519Secret,
}

impl NoiseKey {
    pub fn generate() -> Self {
        let secret = X25519Secret::random_from_rng(OsRng);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        let secret = X25519Secret::from(bytes);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Raw 32-byte X25519 secret — hanya untuk vault serialization dan snow builder.
    pub(crate) fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }
}

/// Pasangan kunci lengkap untuk satu user ALTER.
/// Struct ini yang di-unlock dari vault dan hidup di RAM selama sesi.
#[derive(ZeroizeOnDrop)]
pub struct KeyBundle {
    pub identity: IdentityKey,
    pub noise: NoiseKey,
}

impl KeyBundle {
    pub fn generate() -> Self {
        Self {
            identity: IdentityKey::generate(),
            noise: NoiseKey::generate(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_key_roundtrip() {
        let key = IdentityKey::generate();
        let bytes = *key.secret_bytes();
        let restored = IdentityKey::from_secret_bytes(bytes);
        assert_eq!(key.public_key().to_bytes(), restored.public_key().to_bytes());
    }

    #[test]
    fn noise_key_roundtrip() {
        let key = NoiseKey::generate();
        let bytes = key.secret_bytes();
        let restored = NoiseKey::from_secret_bytes(bytes);
        assert_eq!(key.public_bytes(), restored.public_bytes());
    }

    #[test]
    fn keybundle_keys_are_independent() {
        let bundle = KeyBundle::generate();
        // Identity pubkey dan Noise pubkey harus berbeda
        let id_bytes = bundle.identity.public_key().to_bytes();
        let noise_bytes = bundle.noise.public_bytes();
        assert_ne!(id_bytes.as_slice(), noise_bytes.as_slice());
    }
}
