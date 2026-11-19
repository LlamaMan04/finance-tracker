//! Versioned Argon2id derivation for raw SQLCipher database keys.

use crate::application::ApplicationError;
use crate::ports::security::{
    Argon2Parameters, DATABASE_KEY_LENGTH, DatabaseKey, EncryptionMetadata, KDF_SALT_LENGTH,
    KeyDerivationPort, SecurityResult,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::{OsRng, RngCore};

/// Current serialized metadata format understood by this application.
const FORMAT_VERSION: u32 = 1;

/// Current Argon2id parameter contract, independently versioned for future calibration upgrades.
const KDF_VERSION: u32 = 1;

/// Current SQLCipher key and compatibility contract.
const ENCRYPTION_VERSION: u32 = 1;

/// Calibrated starting point: 64 MiB, three passes, and one lane for baseline desktop hardware.
const ARGON2_PARAMETERS: Argon2Parameters =
    Argon2Parameters { memory_kib: 65_536, time_cost: 3, parallelism: 1 };

/// Implements deterministic Argon2id derivation using metadata persisted outside the database.
#[derive(Clone, Copy, Debug, Default)]
pub struct Argon2idKeyDeriver;

impl KeyDerivationPort for Argon2idKeyDeriver {
    /// Generates a fresh salt while retaining reviewed, versioned work factors.
    fn create_metadata(&self) -> SecurityResult<EncryptionMetadata> {
        let mut salt = [0_u8; KDF_SALT_LENGTH];
        OsRng.fill_bytes(&mut salt);

        Ok(EncryptionMetadata {
            format_version: FORMAT_VERSION,
            kdf_version: KDF_VERSION,
            encryption_version: ENCRYPTION_VERSION,
            salt,
            argon2: ARGON2_PARAMETERS,
        })
    }

    /// Produces a raw 256-bit key so SQLCipher does not apply a second password KDF.
    fn derive_database_key(
        &self,
        passphrase: &[u8],
        metadata: &EncryptionMetadata,
    ) -> SecurityResult<DatabaseKey> {
        if passphrase.is_empty() {
            return Err(ApplicationError::validation());
        }
        validate_metadata(metadata)?;

        let parameters = Params::new(
            metadata.argon2.memory_kib,
            metadata.argon2.time_cost,
            metadata.argon2.parallelism,
            Some(DATABASE_KEY_LENGTH),
        )
        .map_err(|_| ApplicationError::conflict())?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters);
        let mut output = [0_u8; DATABASE_KEY_LENGTH];
        argon2
            .hash_password_into(passphrase, &metadata.salt, &mut output)
            .map_err(|_| ApplicationError::internal())?;

        Ok(DatabaseKey::new(output))
    }
}

/// Rejects metadata that would silently change the security or compatibility contract.
fn validate_metadata(metadata: &EncryptionMetadata) -> SecurityResult<()> {
    if metadata.format_version != FORMAT_VERSION
        || metadata.kdf_version != KDF_VERSION
        || metadata.encryption_version != ENCRYPTION_VERSION
        || metadata.argon2 != ARGON2_PARAMETERS
    {
        return Err(ApplicationError::conflict());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Argon2idKeyDeriver;
    use crate::application::ErrorCode;
    use crate::ports::security::KeyDerivationPort;

    /// Proves profile salts are unique and therefore separate identical passphrases.
    #[test]
    fn creates_unique_profile_metadata() {
        let deriver = Argon2idKeyDeriver;
        let first = deriver.create_metadata().expect("first metadata is generated");
        let second = deriver.create_metadata().expect("second metadata is generated");

        assert_ne!(first.salt, second.salt);
    }

    /// Proves persisted metadata reproduces the same opaque SQLCipher key.
    #[test]
    fn derives_deterministic_keys_for_the_same_profile() {
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("metadata is generated");
        let first = deriver
            .derive_database_key(b"correct horse battery staple", &metadata)
            .expect("first key derives");
        let second = deriver
            .derive_database_key(b"correct horse battery staple", &metadata)
            .expect("second key derives");

        assert_eq!(first.expose(), second.expose());
    }

    /// Proves unsupported metadata is rejected instead of deriving an incompatible key.
    #[test]
    fn rejects_unknown_kdf_versions() {
        let deriver = Argon2idKeyDeriver;
        let mut metadata = deriver.create_metadata().expect("metadata is generated");
        metadata.kdf_version += 1;

        let error = deriver
            .derive_database_key(b"correct horse battery staple", &metadata)
            .err()
            .expect("unknown version fails");

        assert_eq!(error.code, ErrorCode::Conflict);
    }

    /// Proves metadata is serializable without ever serializing derived key material.
    #[test]
    fn round_trips_non_secret_metadata() {
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("metadata is generated");
        let serialized = serde_json::to_string(&metadata).expect("metadata serializes");
        let restored = serde_json::from_str(&serialized).expect("metadata deserializes");

        assert_eq!(metadata, restored);
        assert!(!serialized.contains("passphrase"));
        assert!(!serialized.contains("database_key"));
    }
}
