use super::*;

const DEFAULT_FUEL: u64 = 10_000_000;

#[derive(Clone)]
pub struct Runtime {
  engine: Engine,
  fuel: u64,
}

pub struct RuntimeBuilder {
  fuel: u64,
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
    Self::builder().build()
  }

  /// Creates a store with this runtime's resource limits.
  ///
  /// # Errors
  ///
  /// Returns an error if the store's fuel budget cannot be configured.
  pub fn store<T>(&self, data: T) -> Result<Store<T>> {
    let mut store = Store::new(&self.engine, data);
    store
      .set_fuel(self.fuel)
      .map_err(|source| Error::Store { source })?;

    Ok(store)
  }
}

impl RuntimeBuilder {
  /// Builds a WebAssembly runtime.
  ///
  /// # Errors
  ///
  /// Returns an error if the Wasmtime engine cannot be initialized.
  pub fn build(self) -> Result<Runtime> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);

    let engine =
      Engine::new(&config).map_err(|source| Error::Runtime { source })?;

    Ok(Runtime {
      engine,
      fuel: self.fuel,
    })
  }

  /// Sets the fuel available to each plugin store.
  #[must_use]
  pub fn fuel(mut self, fuel: u64) -> Self {
    self.fuel = fuel;
    self
  }
}

impl Default for RuntimeBuilder {
  fn default() -> Self {
    Self { fuel: DEFAULT_FUEL }
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

  #[test]
  fn store_uses_default_fuel_limit() {
    let runtime = Runtime::new().unwrap();
    let store = runtime.store(()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), DEFAULT_FUEL);
  }

  #[test]
  fn store_uses_overridden_fuel_limit() {
    let runtime = Runtime::builder().fuel(42).build().unwrap();
    let store = runtime.store(()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), 42);
  }
}
