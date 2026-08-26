use super::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("failed to compile WebAssembly component: {source}")]
  Component {
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
}
