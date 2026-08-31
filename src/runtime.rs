use super::*;

#[derive(Clone, Debug)]
pub struct Runtime {
  pub(crate) engine: Engine,
  pub(crate) limits: RuntimeLimits,
}

#[bon::bon]
impl Runtime {
  #[builder(
    builder_type(
      name = RuntimeBuilder,
      vis = "pub",
      doc {
        /// Configures and builds a WebAssembly runtime.
      }
    ),
    finish_fn(
      name = build,
      doc {
        /// Builds a WebAssembly runtime.
        ///
        /// # Errors
        ///
        /// Returns an error if the Wasmtime engine cannot be initialized.
      }
    ),
    start_fn(name = builder, vis = "pub")
  )]
  fn assemble(
    #[builder(field = RuntimeLimits::default())] limits: RuntimeLimits,
  ) -> Result<Self> {
    let mut config = Config::new();

    config
      .consume_fuel(true)
      .epoch_interruption(true)
      .wasm_component_model(true);

    let engine =
      Engine::new(&config).map_err(|source| Error::Runtime { source })?;

    Self::start_epoch_ticker(&engine)?;

    Ok(Self { engine, limits })
  }
}

impl Runtime {
  const EPOCH_INTERVAL: Duration = Duration::from_millis(10);

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

impl Default for RuntimeBuilder {
  fn default() -> Self {
    Runtime::builder()
  }
}

impl<S: runtime_builder::State> RuntimeBuilder<S> {
  /// Sets the fuel available to each plugin store.
  pub fn fuel(mut self, fuel: u64) -> Self {
    self.limits.fuel = fuel;
    self
  }

  /// Sets the maximum number of core instances in each plugin store.
  pub fn instances(mut self, instances: usize) -> Self {
    self.limits.instances = instances;
    self
  }

  /// Sets the default limits and hard ceilings for plugin instances.
  pub fn limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = limits;
    self
  }

  /// Sets the maximum number of linear memories in each plugin store.
  pub fn memories(mut self, memories: usize) -> Self {
    self.limits.memories = memories;
    self
  }

  /// Sets the maximum size in bytes of each guest linear memory.
  pub fn memory_size(mut self, memory_size: usize) -> Self {
    self.limits.memory_size = memory_size;
    self
  }

  /// Sets the maximum number of elements in each guest table.
  pub fn table_elements(mut self, table_elements: usize) -> Self {
    self.limits.table_elements = table_elements;
    self
  }

  /// Sets the maximum number of tables in each plugin store.
  pub fn tables(mut self, tables: usize) -> Self {
    self.limits.tables = tables;
    self
  }

  /// Sets the default wall-clock timeout and ceiling for plugin operations.
  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.limits.timeout = timeout;
    self
  }
}

#[cfg(test)]
mod builder_tests {
  use {
    super::*,
    wasmtime::{
      Instance, Memory, MemoryType, Module, Ref, RefType, Table, TableType,
    },
  };

  #[test]
  fn store_uses_default_fuel_limit() {
    let runtime = Runtime::new().unwrap();

    let store = runtime.store(WasiState::new()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), RuntimeLimits::default().fuel());
  }

  #[test]
  fn store_uses_overridden_fuel_limit() {
    let runtime = Runtime::builder().fuel(42).build().unwrap();

    let store = runtime.store(WasiState::new()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), 42);
  }

  #[test]
  fn store_uses_overridden_instance_limit() {
    let runtime = Runtime::builder().instances(1).build().unwrap();

    let module =
      Module::new(runtime.engine(), wat::parse_str("(module)").unwrap())
        .unwrap();

    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    let error = Instance::new(&mut store, &module, &[]).unwrap_err();

    assert_eq!(
      error.to_string(),
      "resource limit exceeded: instance count too high at 2"
    );
  }

  #[test]
  fn store_uses_overridden_memory_limit() {
    let runtime = Runtime::builder().memories(1).build().unwrap();

    let module = Module::new(
      runtime.engine(),
      wat::parse_str("(module (memory 1))").unwrap(),
    )
    .unwrap();

    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    let error = Instance::new(&mut store, &module, &[]).unwrap_err();

    assert_eq!(
      error.to_string(),
      "resource limit exceeded: memory count too high at 2"
    );
  }

  #[test]
  fn store_uses_overridden_memory_size_limit() {
    let runtime = Runtime::builder().memory_size(64 * 1024).build().unwrap();

    let mut store = runtime.store(WasiState::new()).unwrap();

    Memory::new(&mut store, MemoryType::new(1, None)).unwrap();

    let error = Memory::new(&mut store, MemoryType::new(2, None)).unwrap_err();

    assert_eq!(
      error.to_string(),
      "memory minimum size of 2 pages exceeds memory limits"
    );
  }

  #[test]
  fn store_uses_overridden_table_element_limit() {
    let runtime = Runtime::builder().table_elements(1).build().unwrap();

    let mut store = runtime.store(WasiState::new()).unwrap();

    Table::new(
      &mut store,
      TableType::new(RefType::FUNCREF, 1, None),
      Ref::Func(None),
    )
    .unwrap();

    let error = Table::new(
      &mut store,
      TableType::new(RefType::FUNCREF, 2, None),
      Ref::Func(None),
    )
    .unwrap_err();

    assert_eq!(
      error.to_string(),
      "table minimum size of 2 elements exceeds table limits"
    );
  }

  #[test]
  fn store_uses_overridden_table_limit() {
    let runtime = Runtime::builder().tables(1).build().unwrap();

    let module = Module::new(
      runtime.engine(),
      wat::parse_str("(module (table 1 funcref))").unwrap(),
    )
    .unwrap();

    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    let error = Instance::new(&mut store, &module, &[]).unwrap_err();

    assert_eq!(
      error.to_string(),
      "resource limit exceeded: table count too high at 2"
    );
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
