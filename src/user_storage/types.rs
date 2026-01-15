//! User storage types.
//!
//! This module re-exports the core user types from the login module
//! to maintain backward compatibility while providing a clean API
//! for the user storage abstraction.

// Re-export from login module for backward compatibility
// The types remain in login/ because they have dependencies on webauthn_rs
// and other login-specific code.
pub use crate::login::types::{User, UserWithUsername, UserWithUsernameMut};
pub use crate::login::webauthn::types::UserPasskey;
