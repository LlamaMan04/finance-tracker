use crate::application::ApplicationError;
use crate::domain::ProfileScope;
use serde::Serialize;

/// Describes the session information commands may safely return to the frontend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionStatus {
    pub profile: ProfileScope,
    pub is_locked: bool,
}

/// Defines the future session use cases without coupling them to Tauri or persistence.
pub trait SessionService {
    /// Returns the current session status for the supplied profile scope.
    fn status(&self, profile: &ProfileScope) -> Result<SessionStatus, ApplicationError>;

    /// Locks the supplied profile scope and clears session state in its implementation.
    fn lock(&self, profile: &ProfileScope) -> Result<(), ApplicationError>;
}
