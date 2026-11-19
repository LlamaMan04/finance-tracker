//! Validation helpers for the future profile setup and unlock command adapters.

use crate::application::ApplicationError;
use crate::application::contracts::SessionStatusDto;
use crate::application::use_cases::{CreateProfileRequest, UnlockProfileRequest};
use crate::domain::ProfileId;
use serde::Deserialize;
use zeroize::Zeroizing;

/// Accepts the untrusted fields supplied to the future create-profile command.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileCommandInput {
    /// Carries the user-selected identifier before domain validation.
    pub profile_id: String,
    /// Carries the passphrase only for immediate validation and service handoff.
    pub passphrase: String,
    /// Carries the confirmation used to reject accidental passphrase mismatches.
    pub passphrase_confirmation: String,
}

impl CreateProfileCommandInput {
    /// Validates command-boundary input without returning or logging sensitive values.
    pub fn into_request(self) -> Result<CreateProfileRequest, ApplicationError> {
        // Own both secret inputs in wiping buffers before any validation can return an error.
        let passphrase = Zeroizing::new(self.passphrase);
        let confirmation = Zeroizing::new(self.passphrase_confirmation);
        validate_profile_id(&self.profile_id)?;
        validate_passphrase(&passphrase, &confirmation)?;

        Ok(CreateProfileRequest {
            profile_id: self.profile_id,
            passphrase: Zeroizing::new(passphrase.as_bytes().to_vec()),
        })
    }
}

/// Accepts the untrusted fields supplied to the future unlock-profile command.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockProfileCommandInput {
    /// Carries the requested identifier before domain validation.
    pub profile_id: String,
    /// Carries the passphrase only for immediate validation and service handoff.
    pub passphrase: String,
}

impl UnlockProfileCommandInput {
    /// Validates command-boundary input without returning or logging sensitive values.
    pub fn into_request(self) -> Result<UnlockProfileRequest, ApplicationError> {
        // Own the secret in a wiping buffer before any validation can return an error.
        let passphrase = Zeroizing::new(self.passphrase);
        validate_profile_id(&self.profile_id)?;
        validate_non_empty_passphrase(&passphrase)?;

        Ok(UnlockProfileRequest {
            profile_id: self.profile_id,
            passphrase: Zeroizing::new(passphrase.as_bytes().to_vec()),
        })
    }
}

/// Validates an identifier with the same domain rule that repositories will later enforce.
fn validate_profile_id(value: &str) -> Result<(), ApplicationError> {
    ProfileId::parse(value).map(|_| ())
}

/// Rejects empty setup input and mismatched confirmation without revealing either value.
fn validate_passphrase(value: &str, confirmation: &str) -> Result<(), ApplicationError> {
    validate_non_empty_passphrase(value)?;
    if value != confirmation {
        return Err(ApplicationError::validation());
    }

    Ok(())
}

/// Rejects an empty unlock secret before invoking the future key-derivation service.
fn validate_non_empty_passphrase(value: &str) -> Result<(), ApplicationError> {
    if value.is_empty() {
        return Err(ApplicationError::validation());
    }

    Ok(())
}

/// Supplies the safe locked state used by the future status command before any profile is active.
pub fn locked_session_status() -> SessionStatusDto {
    SessionStatusDto { profile_id: None, is_locked: true }
}

#[cfg(test)]
mod tests {
    use super::{CreateProfileCommandInput, UnlockProfileCommandInput, locked_session_status};
    use crate::application::ErrorCode;

    #[test]
    fn validates_profile_setup_without_preserving_the_confirmation() {
        let request = CreateProfileCommandInput {
            profile_id: "primary-profile".to_owned(),
            passphrase: "passphrase".to_owned(),
            passphrase_confirmation: "passphrase".to_owned(),
        }
        .into_request()
        .expect("valid setup input becomes a service request");

        assert_eq!(request.profile_id, "primary-profile");
        assert_eq!(request.passphrase.as_slice(), b"passphrase");
    }

    #[test]
    fn rejects_invalid_identifiers_empty_passphrases_and_mismatched_confirmation() {
        let invalid_id = UnlockProfileCommandInput {
            profile_id: "invalid profile".to_owned(),
            passphrase: "passphrase".to_owned(),
        }
        .into_request()
        .err()
        .expect("unsafe identifiers must fail");
        let empty_passphrase = UnlockProfileCommandInput {
            profile_id: "primary-profile".to_owned(),
            passphrase: String::new(),
        }
        .into_request()
        .err()
        .expect("empty passphrases must fail");
        let mismatch = CreateProfileCommandInput {
            profile_id: "primary-profile".to_owned(),
            passphrase: "first".to_owned(),
            passphrase_confirmation: "second".to_owned(),
        }
        .into_request()
        .err()
        .expect("mismatched confirmation must fail");

        assert_eq!(invalid_id.code, ErrorCode::Validation);
        assert_eq!(empty_passphrase.code, ErrorCode::Validation);
        assert_eq!(mismatch.code, ErrorCode::Validation);
    }

    #[test]
    fn describes_the_pre_authentication_state_as_locked_without_a_profile_identifier() {
        assert_eq!(locked_session_status().profile_id, None);
        assert!(locked_session_status().is_locked);
    }
}
