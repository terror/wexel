use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("failed to compile WebAssembly component: {source}")]
  Component {
    #[source]
    source: wasmtime::Error,
  },
  #[error("failed to initialize WebAssembly runtime: {source}")]
  Runtime {
    #[source]
    source: wasmtime::Error,
  },
}
