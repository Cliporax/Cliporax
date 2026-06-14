/// Crypto - encryption/decryption for sync data
///
/// Uses XSalsa20Poly1305 (via crypto_secretbox) for authenticated symmetric encryption.
/// Each encryption operation generates a random 24-byte nonce, which is prepended
/// to the ciphertext output. The format is: [nonce (24 bytes) || ciphertext || auth_tag (16 bytes)].
/// Decryption extracts the nonce first, then verifies the auth tag before returning plaintext.
use crate::sync::error::SyncError;
use crate::sync::models::*;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use crypto_secretbox::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    XSalsa20Poly1305,
};
use secrecy::ExposeSecret;

/// Nonce size for XSalsa20Poly1305 (24 bytes / 192 bits)
const NONCE_SIZE: usize = 24;

/// Derive encryption key from password using Argon2id
pub fn derive_key(
    password: &str,
    context: &SyncCryptoContext,
) -> Result<secrecy::SecretVec<u8>, SyncError> {
    let salt = SaltString::encode_b64(&context.salt)
        .map_err(|e| SyncError::Encryption(format!("Invalid salt: {}", e)))?;

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            context.memory_kb,
            context.iterations,
            context.parallelism,
            None,
        )
        .map_err(|e| SyncError::Encryption(format!("Invalid Argon2 params: {}", e)))?,
    );

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| SyncError::Encryption(format!("Argon2 hashing failed: {}", e)))?;

    let key_bytes = hash
        .hash
        .ok_or_else(|| SyncError::Encryption("Argon2 produced no hash".to_string()))?;

    // Use first 32 bytes as key
    let mut key = vec![0u8; 32];
    key.copy_from_slice(&key_bytes.as_bytes()[..32]);

    Ok(secrecy::SecretVec::new(key))
}

/// Encrypt data using XSalsa20Poly1305 (xchacha20poly1305).
///
/// Generates a cryptographically random nonce, encrypts the data, and prepends
/// the nonce to the ciphertext. Output format: [nonce (24 bytes) || ciphertext + auth_tag].
///
/// # Security
/// - Each call generates a fresh random nonce via OS CSPRNG
/// - Auth tag is automatically generated and verified (16 bytes Poly1305 MAC)
/// - Fails closed: any encryption error returns Err, never partial/plaintext output
pub fn encrypt(data: &[u8], key: &secrecy::SecretVec<u8>) -> Result<Vec<u8>, SyncError> {
    let key_bytes = key.expose_secret();

    if key_bytes.len() != 32 {
        return Err(SyncError::Encryption(format!(
            "Invalid key length: expected 32 bytes, got {}",
            key_bytes.len()
        )));
    }

    // Initialize cipher with the 32-byte key
    let cipher = XSalsa20Poly1305::new_from_slice(key_bytes)
        .map_err(|e| SyncError::Encryption(format!("Failed to initialize cipher: {}", e)))?;

    // Generate a random nonce using OS CSPRNG
    let nonce = XSalsa20Poly1305::generate_nonce(&mut OsRng);

    // Encrypt the data (auth tag is appended automatically by the AEAD implementation)
    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| SyncError::Encryption(format!("Encryption failed: {}", e)))?;

    // Prepend nonce to ciphertext: [nonce || ciphertext + auth_tag]
    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(nonce.as_slice());
    output.extend_from_slice(&ciphertext);

    log::debug!(
        "[Sync::Crypto] Encrypted {} bytes -> {} bytes (nonce={} nonce+ciphertext)",
        data.len(),
        output.len(),
        NONCE_SIZE
    );

    Ok(output)
}

/// Decrypt data using XSalsa20Poly1305 (xchacha20poly1305).
///
/// Extracts the nonce from the first 24 bytes of the input, then decrypts
/// and verifies the authentication tag. Returns error if auth tag verification fails.
///
/// # Security
/// - Auth tag is verified before any plaintext is returned
/// - Fails closed: any decryption or auth failure returns Err, never partial output
/// - Input must be at least NONCE_SIZE + 1 bytes (nonce + minimum ciphertext)
pub fn decrypt(data: &[u8], key: &secrecy::SecretVec<u8>) -> Result<Vec<u8>, SyncError> {
    let key_bytes = key.expose_secret();

    if key_bytes.len() != 32 {
        return Err(SyncError::Encryption(format!(
            "Invalid key length: expected 32 bytes, got {}",
            key_bytes.len()
        )));
    }

    // Input must contain at least nonce + 1 byte of ciphertext
    if data.len() < NONCE_SIZE + 1 {
        return Err(SyncError::Encryption(format!(
            "Ciphertext too short: need at least {} bytes (nonce + ciphertext), got {}",
            NONCE_SIZE + 1,
            data.len()
        )));
    }

    // Extract nonce from the first 24 bytes
    let nonce_bytes = &data[..NONCE_SIZE];
    let ciphertext = &data[NONCE_SIZE..];

    // Initialize cipher with the 32-byte key
    let cipher = XSalsa20Poly1305::new_from_slice(key_bytes)
        .map_err(|e| SyncError::Encryption(format!("Failed to initialize cipher: {}", e)))?;

    // Create nonce array from bytes
    let nonce = crypto_secretbox::Nonce::from_slice(nonce_bytes);

    // Decrypt and verify auth tag
    // This will fail if the auth tag doesn't match (tamper detection)
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| {
        SyncError::Encryption(format!(
            "Decryption failed (auth tag verification failed): {}",
            e
        ))
    })?;

    log::debug!(
        "[Sync::Crypto] Decrypted {} bytes -> {} bytes",
        data.len(),
        plaintext.len()
    );

    Ok(plaintext)
}

/// Generate default crypto context
pub fn default_crypto_context() -> SyncCryptoContext {
    SyncCryptoContext {
        algorithm: "xchacha20poly1305".to_string(),
        kdf: "argon2id".to_string(),
        salt: rand::random::<[u8; 16]>().to_vec(),
        memory_kb: 65536, // 64 MB
        iterations: 3,
        parallelism: 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> secrecy::SecretVec<u8> {
        // Use a fixed 32-byte key for testing
        let key_bytes: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        secrecy::SecretVec::new(key_bytes.to_vec())
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"Hello, Cliporax sync encryption!";

        let ciphertext = encrypt(plaintext, &key).expect("encrypt should succeed");
        let decrypted = decrypt(&ciphertext, &key).expect("decrypt should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext_each_time() {
        let key = test_key();
        let plaintext = b"Same plaintext";

        let ct1 = encrypt(plaintext, &key).expect("encrypt should succeed");
        let ct2 = encrypt(plaintext, &key).expect("encrypt should succeed");

        // Different nonces should produce different ciphertexts
        assert_ne!(ct1, ct2);

        // But both should decrypt to the same plaintext
        assert_eq!(decrypt(&ct1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&ct2, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_tampered_data_fails() {
        let key = test_key();
        let plaintext = b"Secret data";

        let mut ciphertext = encrypt(plaintext, &key).expect("encrypt should succeed");

        // Tamper with the ciphertext (modify a byte after the nonce)
        if ciphertext.len() > NONCE_SIZE {
            ciphertext[NONCE_SIZE] ^= 0xFF;
        }

        let result = decrypt(&ciphertext, &key);
        assert!(result.is_err(), "Decryption of tampered data must fail");
    }

    #[test]
    fn test_decrypt_too_short_fails() {
        let key = test_key();
        let short_data = [0u8; 10]; // Less than nonce size (24)

        let result = decrypt(&short_data, &key);
        assert!(result.is_err(), "Decryption of too-short data must fail");
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = test_key();
        let key2 = secrecy::SecretVec::new(vec![0xAB; 32]); // Different key
        let plaintext = b"Secret data";

        let ciphertext = encrypt(plaintext, &key1).expect("encrypt should succeed");
        let result = decrypt(&ciphertext, &key2);
        assert!(result.is_err(), "Decryption with wrong key must fail");
    }

    #[test]
    fn test_encrypt_empty_data() {
        let key = test_key();
        let plaintext = b"";

        let ciphertext = encrypt(plaintext, &key).expect("encrypt should succeed");
        let decrypted = decrypt(&ciphertext, &key).expect("decrypt should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_large_data() {
        let key = test_key();
        let plaintext = vec![0x42u8; 1024 * 100]; // 100KB

        let ciphertext = encrypt(&plaintext, &key).expect("encrypt should succeed");
        let decrypted = decrypt(&ciphertext, &key).expect("decrypt should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_invalid_key_length() {
        let short_key = secrecy::SecretVec::new(vec![0x00; 16]); // Too short
        let result = encrypt(b"test", &short_key);
        assert!(
            result.is_err(),
            "Encryption with invalid key length must fail"
        );
    }

    #[test]
    fn test_derive_key_produces_valid_key() {
        let context = default_crypto_context();
        let key = derive_key("test_password", &context).expect("derive_key should succeed");

        assert_eq!(
            key.expose_secret().len(),
            32,
            "Derived key should be 32 bytes"
        );

        // The key should work for encrypt/decrypt
        let plaintext = b"Test data";
        let ciphertext = encrypt(plaintext, &key).expect("encrypt should succeed");
        let decrypted = decrypt(&ciphertext, &key).expect("decrypt should succeed");
        assert_eq!(decrypted, plaintext);
    }
}
