use super::*;

pub struct RuntimeBuilder {
  fuel: u64,
  instances: usize,
  memories: usize,
  memory_size: usize,
  table_elements: usize,
  tables: usize,
}

impl RuntimeBuilder {
  const DEFAULT_FUEL: u64 = 10_000_000;
  const DEFAULT_INSTANCES: usize = 100;
  const DEFAULT_MEMORIES: usize = 1;
  const DEFAULT_MEMORY_SIZE: usize = 64 * 1024 * 1024;
  const DEFAULT_TABLES: usize = 1;
  const DEFAULT_TABLE_ELEMENTS: usize = 10_000;

  /// Builds a WebAssembly runtime.
  ///
  /// # Errors
  ///
  /// Returns an error if the Wasmtime engine cannot be initialized.
  pub fn build(self) -> Result<Runtime> {
    let mut config = Config::new();
    config.consume_fuel(true).wasm_component_model(true);

    let engine =
      Engine::new(&config).map_err(|source| Error::Runtime { source })?;

    let limits = StoreLimitsBuilder::new()
      .instances(self.instances)
      .memories(self.memories)
      .memory_size(self.memory_size)
      .table_elements(self.table_elements)
      .tables(self.tables)
      .build();

    Ok(Runtime {
      engine,
      fuel: self.fuel,
      limits,
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
      fuel: Self::DEFAULT_FUEL,
      instances: Self::DEFAULT_INSTANCES,
      memories: Self::DEFAULT_MEMORIES,
      memory_size: Self::DEFAULT_MEMORY_SIZE,
      table_elements: Self::DEFAULT_TABLE_ELEMENTS,
      tables: Self::DEFAULT_TABLES,
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
  fn store_uses_default_fuel_limit() {
    let runtime = Runtime::new().unwrap();

    let store = runtime.store(WasiState::new()).unwrap();

    assert_eq!(store.get_fuel().unwrap(), RuntimeBuilder::DEFAULT_FUEL);
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
