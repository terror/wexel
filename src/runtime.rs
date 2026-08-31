use super::*;

#[derive(Clone, Debug)]
pub struct Runtime {
  pub(crate) engine: Engine,
  pub(crate) limits: RuntimeLimits,
}

impl Runtime {
  const EPOCH_INTERVAL: Duration = Duration::from_millis(10);

  /// Creates a runtime builder with secure defaults.
  #[must_use]
  pub fn builder() -> RuntimeBuilder {
    RuntimeBuilder::default()
  }

  /// Returns the underlying Wasmtime engine.
  ///
  /// Stores created directly with this engine bypass Wexel's managed limits
  /// and invocation deadlines.
  #[must_use]
  pub fn engine(&self) -> &Engine {
    &self.engine
  }

  /// Returns the default limits and hard ceilings for plugin instances.
  #[must_use]
  pub fn limits(&self) -> RuntimeLimits {
    self.limits
  }

  /// Creates a raw asynchronous component linker with WASI host interfaces.
  ///
  /// Prefer [`Plugin::instantiate`] for managed plugin execution.
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
      self.clone(),
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

  pub(crate) fn start_epoch_ticker(engine: &Engine) -> Result<()> {
    let engine = engine.weak();

    thread::Builder::new()
      .name("wexel-epoch".into())
      .spawn(move || {
        loop {
          thread::sleep(Self::EPOCH_INTERVAL);

          let Some(engine) = engine.upgrade() else {
            break;
          };

          engine.increment_epoch();
        }
      })
      .map_err(|source| Error::EpochTicker { source })?;

    Ok(())
  }

  /// Creates a raw store with this runtime's resource limits.
  ///
  /// Calls made directly through this store bypass managed invocation
  /// deadlines and structured error mapping. Prefer [`Plugin::instantiate`].
  ///
  /// # Errors
  ///
  /// Returns an error if the store's fuel budget cannot be configured.
  pub fn store<T: WasiStateView>(&self, data: T) -> Result<Store<T>> {
    self.store_with_limits(data, self.limits)
  }

  pub(crate) fn store_with_limits<T: WasiStateView>(
    &self,
    data: T,
    limits: RuntimeLimits,
  ) -> Result<Store<T>> {
    let mut store = Store::new(&self.engine, data);

    store
      .data_mut()
      .wasi_state()
      .set_limits(limits.store_limits());

    store.limiter(|data| data.wasi_state().limits());

    store
      .set_fuel(limits.fuel)
      .map_err(|source| Error::Store { source })?;

    store.set_epoch_deadline(u64::MAX / 2);

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
