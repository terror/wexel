use super::*;

#[derive(Default)]
pub struct RuntimeBuilder {
  limits: RuntimeLimits,
}

impl RuntimeBuilder {
  /// Builds a WebAssembly runtime.
  ///
  /// # Errors
  ///
  /// Returns an error if the Wasmtime engine cannot be initialized.
  pub fn build(self) -> Result<Runtime> {
    let mut config = Config::new();

    config
      .consume_fuel(true)
      .epoch_interruption(true)
      .wasm_component_model(true);

    let engine =
      Engine::new(&config).map_err(|source| Error::Runtime { source })?;

    Runtime::start_epoch_ticker(&engine)?;

    Ok(Runtime {
      engine,
      limits: self.limits,
    })
  }

  /// Sets the fuel available to each plugin store.
  #[must_use]
  pub fn fuel(mut self, fuel: u64) -> Self {
    self.limits.fuel = fuel;
    self
  }

  /// Sets the maximum number of core instances in each plugin store.
  #[must_use]
  pub fn instances(mut self, instances: usize) -> Self {
    self.limits.instances = instances;
    self
  }

  /// Sets the default limits and hard ceilings for plugin instances.
  #[must_use]
  pub fn limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = limits;
    self
  }

  /// Sets the maximum number of linear memories in each plugin store.
  #[must_use]
  pub fn memories(mut self, memories: usize) -> Self {
    self.limits.memories = memories;
    self
  }

  /// Sets the maximum size in bytes of each guest linear memory.
  #[must_use]
  pub fn memory_size(mut self, memory_size: usize) -> Self {
    self.limits.memory_size = memory_size;
    self
  }

  /// Sets the maximum number of elements in each guest table.
  #[must_use]
  pub fn table_elements(mut self, table_elements: usize) -> Self {
    self.limits.table_elements = table_elements;
    self
  }

  /// Sets the maximum number of tables in each plugin store.
  #[must_use]
  pub fn tables(mut self, tables: usize) -> Self {
    self.limits.tables = tables;
    self
  }

  /// Sets the default wall-clock timeout and ceiling for plugin operations.
  #[must_use]
  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.limits.timeout = timeout;
    self
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
