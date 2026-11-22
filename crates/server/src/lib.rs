//! Dandelion server: HTTP API and the embedded web application.
//!
//! `main.rs` wires these modules into the binary. Keeping them in a library
//! lets the router be tested without binding a socket.

pub mod app;
pub mod config;
pub mod health;
pub mod openapi;
pub mod shutdown;
pub mod static_files;
