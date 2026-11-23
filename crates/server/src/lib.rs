//! Dandelion server: HTTP API and the embedded web application.
//!
//! `main.rs` wires these modules into the binary. Keeping them in a library
//! lets the router be tested without binding a socket.

pub mod agents;
pub mod app;
pub mod audit;
pub mod auth;
pub mod claude;
pub mod config;
pub mod containers;
pub mod health;
pub mod metrics;
pub mod openapi;
pub mod processes;
pub mod services;
pub mod shutdown;
pub mod state;
pub mod static_files;
pub mod storage;
pub mod terminal;
pub mod ws;
