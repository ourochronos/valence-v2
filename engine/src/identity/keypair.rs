use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

/// Wrapper around an Ed25519 keypair for signing and verification.
pub struct Keypair {
    signing_key: SigningKey,
}

impl Keypair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Reconstruct from a 32-byte secret key.
    pub fn from_secret(secret: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(secret);
        Self { signing_key }
    }

    /// Get the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Derive a DID string from this keypair's public key.
    pub fn did_string(&self) -> String {
        let encoded = bs58::encode(self.public_key_bytes()).into_string();
        format!("did:valence:key:{}", encoded)
    }

    /// Sign a message, returning the 64-byte signature.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let sig = self.signing_key.sign(message);
        sig.to_bytes()
    }

    /// Verify a signature against a public key.
    pub fn verify(pubkey_bytes: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
        let Ok(verifying_key) = VerifyingKey::from_bytes(pubkey_bytes) else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(signature);
        verifying_key.verify(message, &sig).is_ok()
    }

    /// Get the raw secret key bytes (handle with care).
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_sign_verify() {
        let kp = Keypair::generate();
        let message = b"hello valence";
        let sig = kp.sign(message);
        assert!(Keypair::verify(&kp.public_key_bytes(), message, &sig));
    }

    #[test]
    fn test_wrong_message_fails() {
        let kp = Keypair::generate();
        let sig = kp.sign(b"correct message");
        assert!(!Keypair::verify(&kp.public_key_bytes(), b"wrong message", &sig));
    }

    #[test]
    fn test_roundtrip_from_secret() {
        let kp1 = Keypair::generate();
        let secret = kp1.secret_bytes();
        let kp2 = Keypair::from_secret(&secret);
        assert_eq!(kp1.public_key_bytes(), kp2.public_key_bytes());
        assert_eq!(kp1.did_string(), kp2.did_string());
    }

    #[test]
    fn test_did_derivation() {
        let kp = Keypair::generate();
        let did = kp.did_string();
        assert!(did.starts_with("did:valence:key:"));
    }
}
