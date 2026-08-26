//! Sandboxed WebAssembly plugins for Rust.

mod error;
mod runtime;

pub use {error::Error, runtime::Runtime};

pub type Result<T> = std::result::Result<T, Error>;
