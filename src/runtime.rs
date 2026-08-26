use super::*;

#[derive(Clone)]
pub struct Runtime {
  pub(crate) engine: Engine,
  pub(crate) fuel: u64,
  pub(crate) limits: StoreLimits,
}

impl Runtime {
  /// Creates a runtime builder with secure defaults.
  #[must_use]
  pub fn builder() -> RuntimeBuilder {
    RuntimeBuilder::default()
  }

  #[must_use]
  pub fn engine(&self) -> &Engine {
    &self.engine
  }

  /// Creates an asynchronous component linker with WASI host interfaces.
  ///
  /// # Errors
  ///
  /// Returns an error if the WASI interfaces cannot be added to the linker.
  pub fn linker<T: WasiView>(&self) -> Result<Linker<T>> {
    let mut linker = Linker::new(&self.engine);

    p2::add_to_linker_async(&mut linker)
      .map_err(|source| Error::WasiLinker { source })?;

    Ok(linker)
  }

  /// Loads and compiles a WebAssembly component from a local file.
  ///
  /// # Errors
  ///
  /// Returns an error if the file cannot be read, is not a valid WebAssembly
  /// component, or compilation fails.
  pub fn load(&self, path: impl AsRef<Path>) -> Result<Plugin> {
    let path = path.as_ref();

    self.load_bytes(fs::read(path).map_err(|source| Error::Io {
      path: path.to_owned(),
      source,
    })?)
  }

  /// Compiles a WebAssembly component from binary data.
  ///
  /// # Errors
  ///
  /// Returns an error if the data is not a valid WebAssembly component or
  /// compilation fails.
  pub fn load_bytes(&self, bytes: impl AsRef<[u8]>) -> Result<Plugin> {
    Ok(Plugin::new(
      Component::from_binary(&self.engine, bytes.as_ref())
        .map_err(|source| Error::Component { source })?,
    ))
  }

  /// Creates a WebAssembly runtime.
  ///
  /// # Errors
  ///
  /// Returns an error if the Wasmtime engine cannot be initialized.
  pub fn new() -> Result<Self> {
    Self::builder().build()
  }

  /// Creates a store with this runtime's resource limits.
  ///
  /// # Errors
  ///
  /// Returns an error if the store's fuel budget cannot be configured.
  pub fn store<T: WasiStateView>(&self, data: T) -> Result<Store<T>> {
    let mut store = Store::new(&self.engine, data);

    store
      .data_mut()
      .wasi_state()
      .set_limits(self.limits.clone());

    store.limiter(|data| data.wasi_state().limits());

    store
      .set_fuel(self.fuel)
      .map_err(|source| Error::Store { source })?;

    Ok(store)
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

    let plugin = runtime
      .load_bytes(wat::parse_str("(component)").unwrap())
      .unwrap();

    assert!(Engine::same(runtime.engine(), plugin.component().engine()));
  }

  #[test]
  fn load_bytes_rejects_core_module() {
    let runtime = Runtime::new().unwrap();

    assert_matches!(
      runtime
        .load_bytes(wat::parse_str("(module)").unwrap())
        .unwrap_err(),
      Error::Component { .. }
    );
  }

  #[test]
  fn load_reads_component_file() {
    let directory = tempfile::tempdir().unwrap();

    let path = directory.path().join("plugin.wasm");

    fs::write(&path, wat::parse_str("(component)").unwrap()).unwrap();

    let runtime = Runtime::new().unwrap();

    let plugin = runtime.load(path).unwrap();

    assert!(Engine::same(runtime.engine(), plugin.component().engine()));
  }

  #[test]
  fn load_reports_unreadable_file() {
    let directory = tempfile::tempdir().unwrap();

    let path = directory.path().join("missing.wasm");

    let runtime = Runtime::new().unwrap();

    assert_matches!(
      runtime.load(&path).unwrap_err(),
      Error::Io {
        path: error_path,
        ..
      } if error_path == path
    );
  }
}
