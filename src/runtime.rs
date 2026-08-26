use super::*;

const DEFAULT_FUEL: u64 = 10_000_000;
const DEFAULT_INSTANCES: usize = 100;
const DEFAULT_MEMORIES: usize = 1;
const DEFAULT_MEMORY_SIZE: usize = 64 * 1024 * 1024;
const DEFAULT_TABLE_ELEMENTS: usize = 10_000;
const DEFAULT_TABLES: usize = 1;

#[derive(Clone)]
pub struct Runtime {
  engine: Engine,
  fuel: u64,
  limits: StoreLimits,
}

pub struct RuntimeBuilder {
  fuel: u64,
  instances: usize,
  memories: usize,
  memory_size: usize,
  table_elements: usize,
  tables: usize,
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
      limits: StoreLimitsBuilder::new()
        .instances(self.instances)
        .memories(self.memories)
        .memory_size(self.memory_size)
        .table_elements(self.table_elements)
        .tables(self.tables)
        .build(),
    })
  }

  /// Sets the fuel available to each plugin store.
  #[must_use]
  pub fn fuel(mut self, fuel: u64) -> Self {
    self.fuel = fuel;
    self
  }

  /// Sets the maximum number of core instances in each plugin store.
  #[must_use]
  pub fn instances(mut self, instances: usize) -> Self {
    self.instances = instances;
    self
  }

  /// Sets the maximum number of linear memories in each plugin store.
  #[must_use]
  pub fn memories(mut self, memories: usize) -> Self {
    self.memories = memories;
    self
  }

  /// Sets the maximum size in bytes of each guest linear memory.
  #[must_use]
  pub fn memory_size(mut self, memory_size: usize) -> Self {
    self.memory_size = memory_size;
    self
  }

  /// Sets the maximum number of elements in each guest table.
  #[must_use]
  pub fn table_elements(mut self, table_elements: usize) -> Self {
    self.table_elements = table_elements;
    self
  }

  /// Sets the maximum number of tables in each plugin store.
  #[must_use]
  pub fn tables(mut self, tables: usize) -> Self {
    self.tables = tables;
    self
  }
}

impl Default for RuntimeBuilder {
  fn default() -> Self {
    Self {
      fuel: DEFAULT_FUEL,
      instances: DEFAULT_INSTANCES,
      memories: DEFAULT_MEMORIES,
      memory_size: DEFAULT_MEMORY_SIZE,
      table_elements: DEFAULT_TABLE_ELEMENTS,
      tables: DEFAULT_TABLES,
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    wasmtime::{
      Instance, Memory, MemoryType, Module, Ref, RefType, Table, TableType,
    },
  };

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
    let store = runtime.store(WasiState::new()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), DEFAULT_FUEL);
  }

  #[test]
  fn store_uses_overridden_instance_limit() {
    let runtime = Runtime::builder().instances(1).build().unwrap();
    let bytes = wat::parse_str("(module)").unwrap();
    let module = Module::new(runtime.engine(), bytes).unwrap();
    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert!(Instance::new(&mut store, &module, &[]).is_err());
  }

  #[test]
  fn store_uses_overridden_memory_limit() {
    let runtime = Runtime::builder().memories(1).build().unwrap();
    let bytes = wat::parse_str("(module (memory 1))").unwrap();
    let module = Module::new(runtime.engine(), bytes).unwrap();
    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert!(Instance::new(&mut store, &module, &[]).is_err());
  }

  #[test]
  fn store_uses_overridden_fuel_limit() {
    let runtime = Runtime::builder().fuel(42).build().unwrap();
    let store = runtime.store(WasiState::new()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), 42);
  }

  #[test]
  fn store_uses_overridden_memory_size_limit() {
    let runtime = Runtime::builder().memory_size(64 * 1024).build().unwrap();
    let mut store = runtime.store(WasiState::new()).unwrap();

    Memory::new(&mut store, MemoryType::new(1, None)).unwrap();
    assert!(Memory::new(&mut store, MemoryType::new(2, None)).is_err());
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
    assert!(
      Table::new(
        &mut store,
        TableType::new(RefType::FUNCREF, 2, None),
        Ref::Func(None),
      )
      .is_err()
    );
  }

  #[test]
  fn store_uses_overridden_table_limit() {
    let runtime = Runtime::builder().tables(1).build().unwrap();
    let bytes = wat::parse_str("(module (table 1 funcref))").unwrap();
    let module = Module::new(runtime.engine(), bytes).unwrap();
    let mut store = runtime.store(WasiState::new()).unwrap();

    let _instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert!(Instance::new(&mut store, &module, &[]).is_err());
  }
}
