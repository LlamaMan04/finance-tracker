//! Security-facing ports and opaque values shared by application and infrastructure layers.

use crate::application::ApplicationError;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Number of bytes in the raw key supplied directly to SQLCipher.
pub const DATABASE_KEY_LENGTH: usize = 32;

/// Number of random bytes stored as the unique salt for one profile.
pub const KDF_SALT_LENGTH: usize = 16;

/// Gives security adapters the reviewed, redacted application error contract.
pub type SecurityResult<T> = Result<T, ApplicationError>;

/// Holds a SQLCipher key in a non-copyable value that wipes its bytes when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DatabaseKey([u8; DATABASE_KEY_LENGTH]);

impl DatabaseKey {
    /// Creates an opaque database key from KDF output without exposing a string form.
    pub fn new(bytes: [u8; DATABASE_KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the key only for a reviewed cryptographic operation.
    pub fn expose(&self) -> &[u8; DATABASE_KEY_LENGTH] {
        &self.0
    }
}

/// Records the versioned Argon2id work factors needed to reproduce a profile key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Argon2Parameters {
    pub memory_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

/// Stores only non-secret information required to derive and interpret a protected profile key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptionMetadata {
    pub format_version: u32,
    pub kdf_version: u32,
    pub encryption_version: u32,
    pub salt: [u8; KDF_SALT_LENGTH],
    pub argon2: Argon2Parameters,
}

/// Derives an opaque database key without prescribing the concrete KDF implementation.
pub trait KeyDerivationPort {
    /// Creates unique, versioned metadata for a newly protected profile.
    fn create_metadata(&self) -> SecurityResult<EncryptionMetadata>;

    /// Derives the SQLCipher key for a supported metadata version and passphrase.
    fn derive_database_key(
        &self,
        passphrase: &[u8],
        metadata: &EncryptionMetadata,
    ) -> SecurityResult<DatabaseKey>;
}
