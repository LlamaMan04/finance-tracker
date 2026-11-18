//! Key derivation and lifecycle policy kept outside finance domain and frontend code.

mod key_derivation;

pub use key_derivation::Argon2idKeyDeriver;
