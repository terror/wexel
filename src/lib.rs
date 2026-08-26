//! Sandboxed WebAssembly plugins for Rust.

use {
  std::{
    fs, io,
    path::{Path, PathBuf},
  },
  wasmtime::{Config, Engine, component::Component},
};

pub use {error::Error, plugin::Plugin, runtime::Runtime};

mod error;
mod plugin;
mod runtime;

pub type Result<T> = std::result::Result<T, Error>;
