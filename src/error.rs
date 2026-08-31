use super::*;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("failed to compile WebAssembly component: {source}")]
  Component {
    #[source]
    source: wasmtime::Error,
  },
  #[error(
    "failed to mount directory `{}` at `{guest_path}`: {source}",
    host_path.display()
  )]
  Directory {
    guest_path: String,
    host_path: PathBuf,
    #[source]
    source: wasmtime::Error,
  },
  #[error("failed to start the WebAssembly epoch ticker: {source}")]
  EpochTicker {
    #[source]
    source: io::Error,
  },
  #[error("plugin exhausted its fuel budget")]
  FuelExhausted {
    #[source]
    source: wasmtime::Error,
  },
  #[error("plugin instance is unavailable after timing out")]
  InstanceUnavailable,
  #[error("failed to instantiate plugin: {source}")]
  Instantiation {
    #[source]
    source: wasmtime::Error,
  },
  #[error("plugin instantiation exceeded its {timeout:?} timeout")]
  InstantiationTimeout { timeout: Duration },
  #[error("plugin invocation failed: {source}")]
  Invocation {
    #[source]
    source: wasmtime::Error,
  },
  #[error("plugin invocation exceeded its {timeout:?} timeout")]
  InvocationTimeout { timeout: Duration },
  #[error("failed to read plugin `{}`: {source}", path.display())]
  Io {
    path: PathBuf,
    #[source]
    source: io::Error,
  },
  #[error("failed to configure application host interfaces: {source}")]
  LinkerConfiguration {
    #[source]
    source: wasmtime::Error,
  },
  #[error("failed to initialize WebAssembly runtime: {source}")]
  Runtime {
    #[source]
    source: wasmtime::Error,
  },
  #[error("failed to configure plugin store: {source}")]
  Store {
    #[source]
    source: wasmtime::Error,
  },
  #[error("plugin trapped: {trap}")]
  Trap {
    trap: Trap,
    #[source]
    source: wasmtime::Error,
  },
  #[error("failed to configure WASI linker: {source}")]
  WasiLinker {
    #[source]
    source: wasmtime::Error,
  },
}

impl Error {
  pub(crate) fn instantiation(source: wasmtime::Error) -> Self {
    match source.downcast_ref::<Trap>().copied() {
      Some(Trap::OutOfFuel) => Self::FuelExhausted { source },
      Some(trap) => Self::Trap { trap, source },
      None => Self::Instantiation { source },
    }
  }

  pub(crate) fn invocation(source: wasmtime::Error) -> Self {
    match source.downcast_ref::<Trap>().copied() {
      Some(Trap::OutOfFuel) => Self::FuelExhausted { source },
      Some(trap) => Self::Trap { trap, source },
      None => Self::Invocation { source },
    }
  }
}
