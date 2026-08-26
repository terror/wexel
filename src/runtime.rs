use wasmtime::{Config, Engine};

use crate::{Error, Result};

#[derive(Clone)]
pub struct Runtime {
  engine: Engine,
}

impl Runtime {
  #[must_use]
  pub fn engine(&self) -> &Engine {
    &self.engine
  }

  /// Creates a WebAssembly runtime.
  ///
  /// # Errors
  ///
  /// Returns an error if the Wasmtime engine cannot be initialized.
  pub fn new() -> Result<Self> {
    let mut config = Config::new();
    config.wasm_component_model(true);

    let engine =
      Engine::new(&config).map_err(|source| Error::Runtime { source })?;

    Ok(Self { engine })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn clone_shares_engine() {
    let runtime = Runtime::new().unwrap();
    let clone = runtime.clone();

    assert!(Engine::same(runtime.engine(), clone.engine()));
  }
}
