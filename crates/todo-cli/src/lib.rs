//! Library face of the `todo` CLI.
//!
//! Only the pieces that need to be reachable from integration tests and other
//! crates live here. The command surface itself stays in `main.rs`, because the
//! binary is the product and exposing it as an API would invite a second caller
//! whose expectations diverge from the CLI's.

pub mod serve;
