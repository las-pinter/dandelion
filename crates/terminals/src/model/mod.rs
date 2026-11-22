//! The backend-independent terminal model.
//!
//! Both herdr-core and classic herdr are projected into these types, so the web
//! layer never has to know which protocol produced a terminal.

pub mod filter;
pub mod ids;
pub mod request;
pub mod spec;
pub mod status;
pub mod terminal;

#[cfg(test)]
mod tests;

pub use filter::*;
pub use ids::*;
pub use request::*;
pub use spec::*;
pub use status::*;
pub use terminal::*;
