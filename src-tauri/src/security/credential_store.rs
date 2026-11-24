//! Optional operating-system credential storage for passphrase-authorized profile keys.

use crate::ports::security::{CredentialStorePort, DatabaseKey, SecurityResult};

#[cfg(windows)]
use crate::application::ApplicationError;
#[cfg(windows)]
use crate::ports::security::DATABASE_KEY_LENGTH;

/// Provides the platform credential store while allowing unsupported platforms to use passphrase-only unlock.
#[derive(Default)]
pub struct PlatformCredentialStore;

/// Defines an application-owned, non-secret target name for one profile's credential entry.
pub fn credential_reference(profile_id: &str) -> String {
    format!("FinanceTracker/v1/profile/{profile_id}/database-key")
}

#[cfg(not(windows))]
impl CredentialStorePort for PlatformCredentialStore {
    /// Reports that no platform credential entry is available without affecting passphrase unlock.
    fn load_database_key(&self, _reference: &str) -> SecurityResult<Option<DatabaseKey>> {
        Ok(None)
    }

    /// Treats unsupported credential storage as an optional no-op rather than blocking profile creation.
    fn store_database_key(&self, _reference: &str, _key: &DatabaseKey) -> SecurityResult<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl CredentialStorePort for PlatformCredentialStore {
    /// Loads only an exactly-sized application key and treats unavailable or malformed entries as absent.
    fn load_database_key(&self, reference: &str) -> SecurityResult<Option<DatabaseKey>> {
        use std::ptr;
        use windows_sys::Win32::Security::Credentials::{
            CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
        };

        let mut target_name = wide_null(reference);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: target_name is null-terminated, credential is a valid out-pointer, and the API
        // allocates the returned credential for CredFree when it succeeds.
        let read_succeeded =
            unsafe { CredReadW(target_name.as_mut_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) }
                != 0;
        if !read_succeeded || credential.is_null() {
            return Ok(None);
        }

        // SAFETY: CredReadW returned a non-null credential owned by Credential Manager until freed.
        let stored_key = unsafe {
            let blob_size = (*credential).CredentialBlobSize as usize;
            let blob = (*credential).CredentialBlob;
            let key = if blob_size == DATABASE_KEY_LENGTH && !blob.is_null() {
                let mut bytes = [0_u8; DATABASE_KEY_LENGTH];
                bytes.copy_from_slice(std::slice::from_raw_parts(blob, DATABASE_KEY_LENGTH));
                Some(DatabaseKey::new(bytes))
            } else {
                None
            };
            CredFree(credential.cast());
            key
        };

        Ok(stored_key)
    }

    /// Writes the opaque SQLCipher key into the current user's encrypted Credential Manager vault.
    fn store_database_key(&self, reference: &str, key: &DatabaseKey) -> SecurityResult<()> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        };

        let mut target_name = wide_null(reference);
        let mut user_name = wide_null("Finance Tracker");
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target_name.as_mut_ptr(),
            CredentialBlobSize: DATABASE_KEY_LENGTH as u32,
            CredentialBlob: key.expose().as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user_name.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: every pointer in the credential references live buffers for the duration of this call.
        let write_succeeded = unsafe { CredWriteW(&credential, 0) } != 0;
        if write_succeeded { Ok(()) } else { Err(ApplicationError::internal()) }
    }
}

#[cfg(windows)]
/// Converts a Rust string to the null-terminated UTF-16 format required by Credential Manager.
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::credential_reference;

    /// Proves credential targets are application-owned and never embed key material or a passphrase.
    #[test]
    fn creates_a_non_secret_stable_credential_reference() {
        let reference = credential_reference("household-01");

        assert_eq!(reference, "FinanceTracker/v1/profile/household-01/database-key");
        assert!(!reference.contains("passphrase"));
    }
}
