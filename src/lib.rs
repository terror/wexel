//! Sandboxed WebAssembly plugins for Rust.

mod error;
mod plugin;
mod runtime;

pub use {error::Error, plugin::Plugin, runtime::Runtime};

pub type Result<T> = std::result::Result<T, Error>;
