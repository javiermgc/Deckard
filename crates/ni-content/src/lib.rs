//! Read-only discovery of installed Native Access content. No host, GUI, or database dependency.
mod catalog;
pub mod macos;
pub mod paths;
pub mod windows;
pub mod wine;
pub use catalog::*;
pub use paths::wine_to_host;

pub mod metadata;
#[doc(hidden)]
pub mod msgpack;

pub mod resources;
