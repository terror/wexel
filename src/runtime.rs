use wasmtime::{Config, Engine, component::Component};

use crate::{Error, Plugin, Result};

#[derive(Clone)]
pub struct Runtime {
  engine: Engine,
}

impl Runtime {
  #[must_use]
  pub fn engine(&self) -> &Engine {
    &self.engine
  }

  /// Compiles a WebAssembly component from binary data.
  ///
  /// # Errors
  ///
  /// Returns an error if the data is not a valid WebAssembly component or
  /// compilation fails.
  pub fn load_bytes(&self, bytes: impl AsRef<[u8]>) -> Result<Plugin> {
    let component = Component::from_binary(&self.engine, bytes.as_ref())
      .map_err(|source| Error::Component { source })?;

    Ok(Plugin::new(component))
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

  #[test]
  fn load_bytes_accepts_component() {
    let runtime = Runtime::new().unwrap();
    let bytes = wat::parse_str("(component)").unwrap();
    let plugin = runtime.load_bytes(bytes).unwrap();

    assert!(Engine::same(runtime.engine(), plugin.component().engine()));
  }

  #[test]
  fn load_bytes_rejects_core_module() {
    let runtime = Runtime::new().unwrap();
    let bytes = wat::parse_str("(module)").unwrap();
    let error = runtime.load_bytes(bytes).unwrap_err();

    assert!(matches!(error, Error::Component { .. }));
  }
}
