//! Sandboxed WebAssembly plugins for Rust.

use {
  std::{
    fs, io,
    path::{Path, PathBuf},
  },
  wasmtime::{
    Config, Engine,
    component::{Component, ResourceTable},
  },
  wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView},
};

pub use {
  error::Error, plugin::Plugin, runtime::Runtime, wasi_state::WasiState,
};

mod error;
mod plugin;
mod runtime;
mod wasi_state;

pub type Result<T> = std::result::Result<T, Error>;
