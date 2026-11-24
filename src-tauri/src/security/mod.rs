//! Key derivation and lifecycle policy kept outside finance domain and frontend code.

mod credential_store;
mod key_derivation;
mod profile_service;
mod session_timeout;
mod unlock_throttle;

pub use credential_store::PlatformCredentialStore;
pub use key_derivation::Argon2idKeyDeriver;
pub use profile_service::LocalProfileService;
