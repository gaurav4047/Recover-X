//! RecoverX Core
//!
//! Foundational types, traits, and error definitions shared across all
//! RecoverX crates. Nothing in this crate performs I/O.

pub mod error;
pub mod events;
pub mod identity;
pub mod security;
pub mod types;

pub use error::{RecoverXError, Result};
