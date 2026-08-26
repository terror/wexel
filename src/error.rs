use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("failed to initialize WebAssembly runtime: {source}")]
  Runtime {
    #[source]
    source: wasmtime::Error,
  },
}
