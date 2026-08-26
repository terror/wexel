//! Sandboxed WebAssembly plugins for Rust.

use {
  std::{
    fs, io,
    path::{Path, PathBuf},
  },
  wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker, ResourceTable},
  },
  wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, p2},
};

pub use {
  error::Error,
  plugin::Plugin,
  runtime::Runtime,
  runtime_builder::RuntimeBuilder,
  wasi_state::{WasiState, WasiStateBuilder, WasiStateView},
  wasmtime_wasi::{WasiCtxView, WasiView},
};

mod error;
mod plugin;
mod runtime;
mod runtime_builder;
mod wasi_state;

pub type Result<T> = std::result::Result<T, Error>;
