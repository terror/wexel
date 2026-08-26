use super::*;

#[derive(Debug, thiserror::Error)]
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
  #[error("failed to read plugin `{}`: {source}", path.display())]
  Io {
    path: PathBuf,
    #[source]
    source: io::Error,
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
  #[error("failed to configure WASI linker: {source}")]
  WasiLinker {
    #[source]
    source: wasmtime::Error,
  },
}
