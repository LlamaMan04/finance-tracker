//! Authenticated, versioned backup packages stored inside the active profile directory.

use crate::application::ApplicationError;
use crate::application::contracts::BackupDto;
use crate::domain::ProfileScope;
use crate::infrastructure::database::Database;
use crate::ports::security::{Argon2Parameters, DatabaseKey, EncryptionMetadata, KDF_SALT_LENGTH};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use rand_core::{OsRng, RngCore};
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// Identifies the on-disk package so malformed files fail before decryption.
const MAGIC: &[u8; 4] = b"FTBK";
/// Identifies the portable authenticated backup envelope format.
pub const FORMAT_VERSION: u32 = 2;
/// Identifies the ledger compatibility contract embedded in this package.
const COMPATIBILITY_VERSION: u32 = 1;
/// AES-GCM uses a twelve-byte nonce for each package.
const NONCE_LENGTH: usize = 12;

/// Creates one encrypted snapshot without returning its bytes or path to the caller.
pub fn export(
    database: &Database,
    key: &DatabaseKey,
    scope: &ProfileScope,
    encryption_metadata: &EncryptionMetadata,
    backup_directory: &Path,
) -> Result<BackupDto, ApplicationError> {
    database.checkpoint_wal().map_err(|_| ApplicationError::backup())?;
    let database_bytes = database.encrypted_file_bytes().map_err(|_| ApplicationError::backup())?;
    let backup_id = Uuid::new_v4().to_string();
    let created_at = database.current_timestamp().map_err(|_| ApplicationError::backup())?;
    let header =
        encode_header(scope.profile_id().as_str(), &backup_id, &created_at, encryption_metadata);
    let mut nonce_bytes = [0_u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| ApplicationError::backup())?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), Payload { msg: &database_bytes, aad: &header })
        .map_err(|_| ApplicationError::backup())?;

    let mut package = header;
    package.extend_from_slice(&nonce_bytes);
    package.extend_from_slice(&ciphertext);
    fs::create_dir_all(backup_directory).map_err(|_| ApplicationError::backup())?;
    let target = backup_directory.join(format!("{backup_id}.ftbackup"));
    let temporary = backup_directory.join(format!(".{backup_id}.tmp"));
    fs::write(&temporary, package).map_err(|_| ApplicationError::backup())?;
    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
            ApplicationError::conflict()
        } else {
            ApplicationError::backup()
        });
    }

    Ok(BackupDto { id: backup_id, format_version: FORMAT_VERSION, created_at })
}

/// Decrypts one application-owned backup only after authenticating its complete envelope.
///
/// The caller owns the returned encrypted-database bytes and must validate them in a disposable
/// database file before replacing any live profile data.
pub fn decrypt_for_restore(
    backup_directory: &Path,
    backup_id: &str,
    scope: &ProfileScope,
    key: &DatabaseKey,
    expected_encryption_metadata: &EncryptionMetadata,
) -> Result<Vec<u8>, ApplicationError> {
    let package_path = backup_directory.join(format!("{backup_id}.ftbackup"));
    let package = fs::read(package_path).map_err(|_| ApplicationError::backup())?;
    let (header, header_length) = decode_header(&package)?;
    if header.profile_id != scope.profile_id().as_str()
        || header.encryption_metadata != *expected_encryption_metadata
    {
        return Err(ApplicationError::backup());
    }

    let nonce_end = header_length.checked_add(NONCE_LENGTH).ok_or_else(ApplicationError::backup)?;
    if package.len() < nonce_end + 16 {
        return Err(ApplicationError::backup());
    }
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| ApplicationError::backup())?;
    cipher
        .decrypt(
            Nonce::from_slice(&package[header_length..nonce_end]),
            Payload { msg: &package[nonce_end..], aad: &package[..header_length] },
        )
        .map_err(|_| ApplicationError::backup())
}

/// Holds authenticated envelope data needed to reject incompatible restore attempts.
struct BackupHeader {
    profile_id: String,
    encryption_metadata: EncryptionMetadata,
}

/// Decodes only bounded, non-secret envelope metadata before AES-GCM authenticates it.
fn decode_header(package: &[u8]) -> Result<(BackupHeader, usize), ApplicationError> {
    let mut cursor = 0;
    if read_exact(package, &mut cursor, MAGIC.len())? != MAGIC {
        return Err(ApplicationError::backup());
    }
    if read_u32(package, &mut cursor)? != FORMAT_VERSION
        || read_u32(package, &mut cursor)? != COMPATIBILITY_VERSION
    {
        return Err(ApplicationError::backup());
    }
    let profile_id = read_string(package, &mut cursor)?;
    // The backup identifier and creation time are authenticated metadata, but are not needed to
    // restore an application-owned package selected by its validated filename.
    let _backup_id = read_string(package, &mut cursor)?;
    let _created_at = read_string(package, &mut cursor)?;
    let encryption_metadata = EncryptionMetadata {
        format_version: read_u32(package, &mut cursor)?,
        kdf_version: read_u32(package, &mut cursor)?,
        encryption_version: read_u32(package, &mut cursor)?,
        argon2: Argon2Parameters {
            memory_kib: read_u32(package, &mut cursor)?,
            time_cost: read_u32(package, &mut cursor)?,
            parallelism: read_u32(package, &mut cursor)?,
        },
        salt: read_array(package, &mut cursor)?,
    };

    Ok((BackupHeader { profile_id, encryption_metadata }, cursor))
}

/// Reads an exact bounded slice without allowing malformed lengths to overflow indexing.
fn read_exact<'a>(
    package: &'a [u8],
    cursor: &mut usize,
    length: usize,
) -> Result<&'a [u8], ApplicationError> {
    let end = cursor.checked_add(length).ok_or_else(ApplicationError::backup)?;
    let value = package.get(*cursor..end).ok_or_else(ApplicationError::backup)?;
    *cursor = end;
    Ok(value)
}

/// Reads one little-endian envelope integer.
fn read_u32(package: &[u8], cursor: &mut usize) -> Result<u32, ApplicationError> {
    let bytes: [u8; 4] =
        read_exact(package, cursor, 4)?.try_into().map_err(|_| ApplicationError::backup())?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads a short UTF-8 header string with an explicit application limit.
fn read_string(package: &[u8], cursor: &mut usize) -> Result<String, ApplicationError> {
    const MAX_HEADER_STRING_LENGTH: usize = 512;
    let length =
        usize::try_from(read_u32(package, cursor)?).map_err(|_| ApplicationError::backup())?;
    if length > MAX_HEADER_STRING_LENGTH {
        return Err(ApplicationError::backup());
    }
    let bytes = read_exact(package, cursor, length)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| ApplicationError::backup())
}

/// Reads the fixed-width KDF salt without accepting a truncated envelope.
fn read_array(
    package: &[u8],
    cursor: &mut usize,
) -> Result<[u8; KDF_SALT_LENGTH], ApplicationError> {
    read_exact(package, cursor, KDF_SALT_LENGTH)?.try_into().map_err(|_| ApplicationError::backup())
}

/// Encodes the portable, authenticated metadata before the nonce and ciphertext.
///
/// The KDF metadata is not secret, but it is necessary to derive the source key from a passphrase
/// on another computer. AES-GCM authenticates these fields as AAD, so an importer can trust them
/// only after successful decryption and must never persist a key or passphrase in the package.
fn encode_header(
    profile_id: &str,
    backup_id: &str,
    created_at: &str,
    encryption_metadata: &EncryptionMetadata,
) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&COMPATIBILITY_VERSION.to_le_bytes());
    append_string(&mut header, profile_id);
    append_string(&mut header, backup_id);
    append_string(&mut header, created_at);
    append_encryption_metadata(&mut header, encryption_metadata);
    header
}

/// Appends a bounded-length string without exposing a serialization dependency at the file edge.
fn append_string(buffer: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buffer.extend_from_slice(bytes);
}

/// Appends only the non-secret, versioned KDF contract needed by a future portable importer.
fn append_encryption_metadata(buffer: &mut Vec<u8>, metadata: &EncryptionMetadata) {
    buffer.extend_from_slice(&metadata.format_version.to_le_bytes());
    buffer.extend_from_slice(&metadata.kdf_version.to_le_bytes());
    buffer.extend_from_slice(&metadata.encryption_version.to_le_bytes());
    buffer.extend_from_slice(&metadata.argon2.memory_kib.to_le_bytes());
    buffer.extend_from_slice(&metadata.argon2.time_cost.to_le_bytes());
    buffer.extend_from_slice(&metadata.argon2.parallelism.to_le_bytes());
    buffer.extend_from_slice(&metadata.salt);
}

#[cfg(test)]
mod tests {
    use super::{COMPATIBILITY_VERSION, FORMAT_VERSION, MAGIC, decrypt_for_restore, export};
    use crate::application::{ApplicationError, ErrorCode};
    use crate::domain::{ProfileId, ProfileScope};
    use crate::infrastructure::database::Database;
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::security::{EncryptionMetadata, KeyDerivationPort};
    use crate::security::Argon2idKeyDeriver;
    use std::fs;

    /// Creates the same protected test database used by the rest of the infrastructure tests.
    fn fixture() -> (
        TemporaryDatabase,
        Database,
        crate::ports::security::DatabaseKey,
        crate::ports::security::EncryptionMetadata,
    ) {
        let database_fixture = TemporaryDatabase::new();
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("metadata is generated");
        let key = deriver
            .derive_database_key(b"backup test passphrase", &metadata)
            .expect("test key derives");
        let database = Database::create(database_fixture.path(), &key).expect("database creates");
        (database_fixture, database, key, metadata)
    }

    #[test]
    fn exports_an_authenticated_versioned_package_without_plaintext_database_bytes() {
        let (database_fixture, database, key, metadata) = fixture();
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO application_metadata (key, value) VALUES (?1, ?2)",
                        ("backup-secret", "sensitive-backup-value"),
                    )
                    .map_err(|_| ApplicationError::internal())?;
                Ok(())
            })
            .expect("fixture data writes");
        let backup_directory = tempfile::tempdir().expect("backup directory creates");
        let result = export(
            &database,
            &key,
            &ProfileScope::new(ProfileId::parse("profile-a").unwrap()),
            &metadata,
            backup_directory.path(),
        )
        .expect("backup exports");
        let path = backup_directory.path().join(format!("{}.ftbackup", result.id));
        let package = fs::read(path).expect("backup package is written");

        assert_eq!(&package[..4], MAGIC);
        assert_eq!(u32::from_le_bytes(package[4..8].try_into().unwrap()), FORMAT_VERSION);
        assert_eq!(u32::from_le_bytes(package[8..12].try_into().unwrap()), COMPATIBILITY_VERSION);
        let metadata_start = 12
            + encoded_string_length("profile-a")
            + encoded_string_length(&result.id)
            + encoded_string_length(&result.created_at);
        assert_eq!(
            u32::from_le_bytes(package[metadata_start..metadata_start + 4].try_into().unwrap()),
            metadata.format_version
        );
        assert_eq!(
            u32::from_le_bytes(package[metadata_start + 4..metadata_start + 8].try_into().unwrap()),
            metadata.kdf_version
        );
        assert_eq!(
            u32::from_le_bytes(
                package[metadata_start + 8..metadata_start + 12].try_into().unwrap()
            ),
            metadata.encryption_version
        );
        assert_eq!(
            u32::from_le_bytes(
                package[metadata_start + 12..metadata_start + 16].try_into().unwrap()
            ),
            metadata.argon2.memory_kib
        );
        assert_eq!(
            u32::from_le_bytes(
                package[metadata_start + 16..metadata_start + 20].try_into().unwrap()
            ),
            metadata.argon2.time_cost
        );
        assert_eq!(
            u32::from_le_bytes(
                package[metadata_start + 20..metadata_start + 24].try_into().unwrap()
            ),
            metadata.argon2.parallelism
        );
        assert_eq!(&package[metadata_start + 24..metadata_start + 40], &metadata.salt);
        assert!(!package.windows(key.expose().len()).any(|window| window == key.expose()));
        assert!(
            !package
                .windows(b"sensitive-backup-value".len())
                .any(|window| { window == b"sensitive-backup-value" })
        );
        assert!(!package.is_empty());
        assert!(database_fixture.path().is_file());
    }

    /// Calculates a length-prefixed header string without reproducing production serialization.
    fn encoded_string_length(value: &str) -> usize {
        std::mem::size_of::<u32>() + value.len()
    }

    #[test]
    fn creates_unique_backup_files() {
        let (_database_fixture, database, key, metadata) = fixture();
        let backup_directory = tempfile::tempdir().expect("backup directory creates");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").unwrap());
        let first = export(&database, &key, &scope, &metadata, backup_directory.path())
            .expect("first export");
        let second = export(&database, &key, &scope, &metadata, backup_directory.path())
            .expect("second export");

        assert_ne!(first.id, second.id);
        assert_eq!(fs::read_dir(backup_directory.path()).unwrap().count(), 2);
    }

    /// Proves restore decryption returns exactly the protected database snapshot after validation.
    #[test]
    fn decrypts_only_an_authenticated_profile_bound_package() {
        let (database_fixture, database, key, metadata) = fixture();
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO application_metadata (key, value) VALUES (?1, ?2)",
                        ("restore-check", "expected value"),
                    )
                    .map_err(|_| ApplicationError::internal())?;
                Ok(())
            })
            .expect("fixture data writes");
        let backup_directory = tempfile::tempdir().expect("backup directory creates");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").unwrap());
        let exported = export(&database, &key, &scope, &metadata, backup_directory.path())
            .expect("backup exports");

        let restored =
            decrypt_for_restore(backup_directory.path(), &exported.id, &scope, &key, &metadata)
                .expect("authenticated package decrypts");
        assert_eq!(restored, fs::read(database_fixture.path()).expect("database reads"));

        let path = backup_directory.path().join(format!("{}.ftbackup", exported.id));
        let mut tampered = fs::read(&path).expect("package reads");
        let last_byte = tampered.len() - 1;
        tampered[last_byte] ^= 1;
        fs::write(&path, tampered).expect("tampered package writes");
        assert_eq!(
            decrypt_for_restore(backup_directory.path(), &exported.id, &scope, &key, &metadata)
                .expect_err("tampered package is rejected")
                .code,
            ErrorCode::Backup
        );
    }

    /// Proves the restore boundary rejects every envelope failure before releasing database bytes.
    #[test]
    fn rejects_wrong_key_wrong_profile_malformed_and_incompatible_packages() {
        let (_database_fixture, database, key, metadata) = fixture();
        let backup_directory = tempfile::tempdir().expect("backup directory creates");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").unwrap());
        let exported = export(&database, &key, &scope, &metadata, backup_directory.path())
            .expect("backup exports");
        let path = backup_directory.path().join(format!("{}.ftbackup", exported.id));
        let original = fs::read(&path).expect("package reads");
        let wrong_key = Argon2idKeyDeriver
            .derive_database_key(b"a different passphrase", &metadata)
            .expect("wrong test key derives");

        assert_eq!(
            decrypt_for_restore(
                backup_directory.path(),
                &exported.id,
                &scope,
                &wrong_key,
                &metadata
            )
            .expect_err("wrong key is rejected")
            .code,
            ErrorCode::Backup
        );
        assert_eq!(
            decrypt_for_restore(
                backup_directory.path(),
                &exported.id,
                &ProfileScope::new(ProfileId::parse("profile-b").unwrap()),
                &key,
                &metadata,
            )
            .expect_err("wrong profile is rejected")
            .code,
            ErrorCode::Backup
        );

        fs::write(&path, &original[..original.len() - 1]).expect("truncated package writes");
        assert_eq!(
            decrypt_for_restore(backup_directory.path(), &exported.id, &scope, &key, &metadata)
                .expect_err("truncated package is rejected")
                .code,
            ErrorCode::Backup
        );

        let mut incompatible = original;
        incompatible[4..8].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        fs::write(&path, incompatible).expect("incompatible package writes");
        assert_eq!(
            decrypt_for_restore(backup_directory.path(), &exported.id, &scope, &key, &metadata)
                .expect_err("incompatible package is rejected")
                .code,
            ErrorCode::Backup
        );

        // Keep this import explicit in the fixture contract: the package metadata is the exact
        // authenticated value, not a caller-controlled replacement.
        let _: EncryptionMetadata = metadata;
    }
}
