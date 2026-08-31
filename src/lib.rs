use {
  std::{
    fs, io,
    net::SocketAddr,
    ops::AsyncFnOnce,
    path::{Path, PathBuf},
    thread,
    time::Duration,
  },
  wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap,
    component::{
      Component, Instance as WasmtimeInstance, Linker, ResourceTable,
    },
  },
  wasmtime_wasi::{
    FsPerms, WasiCtx, WasiCtxBuilder, p2, sockets::SocketAddrUse,
  },
};

pub use {
  error::Error,
  instance::{Instance, InstanceBuilder},
  permissions::{Permissions, PermissionsBuilder},
  plugin::Plugin,
  runtime::{Runtime, RuntimeBuilder},
  runtime_limits::{RuntimeLimits, RuntimeLimitsBuilder},
  wasi_state::{WasiState, WasiStateBuilder},
  wasi_state_view::WasiStateView,
  wasmtime_wasi::{WasiCtxView, WasiView},
};

#[cfg(test)]
macro_rules! assert_matches {
  ($expression:expr, $( $pattern:pat_param )|+ $( if $guard:expr )? $(,)?) => {
    match $expression {
      $( $pattern )|+ $( if $guard )? => {}
      left => panic!(
        "assertion failed: (left ~= right)\n  left: `{:?}`\n right: `{}`",
        left,
        stringify!($($pattern)|+ $(if $guard)?)
      ),
    }
  }
}

mod error;
mod instance;
mod permissions;
mod plugin;
mod runtime;
mod runtime_limits;
mod wasi_state;
mod wasi_state_view;

pub type Result<T> = std::result::Result<T, Error>;
