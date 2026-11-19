use crate::application::ApplicationError;
use serde::Serialize;

/// Implements an opaque, validated stable identifier newtype.
macro_rules! define_identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates a non-empty identifier safe for use as an opaque application key.
            pub fn parse(value: &str) -> Result<Self, ApplicationError> {
                let is_valid = !value.is_empty()
                    && value.len() <= 128
                    && value.bytes().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_' | b':')
                    });

                if !is_valid {
                    return Err(ApplicationError::validation());
                }

                Ok(Self(value.to_owned()))
            }

            /// Returns the opaque identifier for a persistence adapter or approved DTO.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

define_identifier!(ProfileId);
define_identifier!(BudgetId);
define_identifier!(CategoryId);
define_identifier!(AccountId);
define_identifier!(TransactionId);
define_identifier!(AllocationId);
define_identifier!(BackupId);

/// Limits repository access and future records to one validated profile.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
pub struct ProfileScope {
    profile_id: ProfileId,
}

impl ProfileScope {
    /// Creates the profile boundary that every protected repository operation must receive.
    pub fn new(profile_id: ProfileId) -> Self {
        Self { profile_id }
    }

    /// Returns the profile identifier used to enforce ownership in an adapter.
    pub fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }
}

#[cfg(test)]
mod tests {
    use super::{BudgetId, ProfileId, ProfileScope};
    use crate::application::ErrorCode;

    #[test]
    fn accepts_opaque_stable_identifier_characters() {
        let id = BudgetId::parse("budget_2026:primary-01").expect("fixture identifier is valid");

        assert_eq!(id.as_str(), "budget_2026:primary-01");
    }

    #[test]
    fn rejects_empty_or_unsafe_identifiers() {
        let empty = ProfileId::parse("").expect_err("identifiers cannot be empty");
        let unsafe_id = ProfileId::parse("profile id").expect_err("spaces are not allowed");

        assert_eq!(empty.code, ErrorCode::Validation);
        assert_eq!(unsafe_id.code, ErrorCode::Validation);
    }

    #[test]
    fn preserves_profile_scope_identity() {
        let profile = ProfileId::parse("profile-01").expect("fixture identifier is valid");
        let scope = ProfileScope::new(profile);

        assert_eq!(scope.profile_id().as_str(), "profile-01");
    }
}
