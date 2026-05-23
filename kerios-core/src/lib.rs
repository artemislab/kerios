//! Kerios core library.
//!
//! Shared building blocks used by the daemon and CLI: config merge,
//! policy engine, and provider adapters for AI coding assistants.

pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod github_app;
pub mod merge;
pub mod providers;
pub mod secrets;
pub mod sources;
pub mod state;
pub mod sync;
