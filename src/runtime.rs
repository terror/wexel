use std::{fs, path::Path};

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

  /// Loads and compiles a WebAssembly component from a local file.
  ///
  /// # Errors
  ///
  /// Returns an error if the file cannot be read, is not a valid WebAssembly
  /// component, or compilation fails.
  pub fn load(&self, path: impl AsRef<Path>) -> Result<Plugin> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| Error::Io {
      path: path.to_owned(),
      source,
    })?;

    self.load_bytes(bytes)
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

  #[test]
  fn load_reads_component_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plugin.wasm");
    let bytes = wat::parse_str("(component)").unwrap();
    fs::write(&path, bytes).unwrap();

    let runtime = Runtime::new().unwrap();
    let plugin = runtime.load(path).unwrap();

    assert!(Engine::same(runtime.engine(), plugin.component().engine()));
  }

  #[test]
  fn load_reports_unreadable_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.wasm");
    let runtime = Runtime::new().unwrap();
    let error = runtime.load(&path).unwrap_err();

    assert!(matches!(
      error,
      Error::Io {
        path: error_path,
        ..
      } if error_path == path
    ));
  }
}
