use super::*;

/// Resource and execution limits applied to one plugin instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLimits {
  pub(crate) fuel: u64,
  pub(crate) instances: usize,
  pub(crate) memories: usize,
  pub(crate) memory_size: usize,
  pub(crate) table_elements: usize,
  pub(crate) tables: usize,
  pub(crate) timeout: Duration,
}

impl RuntimeLimits {
  const DEFAULT_FUEL: u64 = 10_000_000;
  const DEFAULT_INSTANCES: usize = 100;
  const DEFAULT_MEMORIES: usize = 1;
  const DEFAULT_MEMORY_SIZE: usize = 64 * 1024 * 1024;
  const DEFAULT_TABLES: usize = 1;
  const DEFAULT_TABLE_ELEMENTS: usize = 10_000;
  const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

  /// Creates a runtime limits builder with secure defaults.
  pub fn builder() -> RuntimeLimitsBuilder {
    RuntimeLimitsBuilder::default()
  }

  /// Returns the fuel budget for one plugin store.
  #[must_use]
  pub fn fuel(&self) -> u64 {
    self.fuel
  }

  /// Returns the maximum number of core instances in one plugin store.
  #[must_use]
  pub fn instances(&self) -> usize {
    self.instances
  }

  /// Returns the maximum number of linear memories in one plugin store.
  #[must_use]
  pub fn memories(&self) -> usize {
    self.memories
  }

  /// Returns the maximum size in bytes of each guest linear memory.
  #[must_use]
  pub fn memory_size(&self) -> usize {
    self.memory_size
  }

  pub(crate) fn restrict(self, requested: Self) -> Self {
    Self {
      fuel: self.fuel.min(requested.fuel),
      instances: self.instances.min(requested.instances),
      memories: self.memories.min(requested.memories),
      memory_size: self.memory_size.min(requested.memory_size),
      table_elements: self.table_elements.min(requested.table_elements),
      tables: self.tables.min(requested.tables),
      timeout: self.timeout.min(requested.timeout),
    }
  }

  pub(crate) fn store_limits(self) -> StoreLimits {
    StoreLimitsBuilder::new()
      .instances(self.instances)
      .memories(self.memories)
      .memory_size(self.memory_size)
      .table_elements(self.table_elements)
      .tables(self.tables)
      .build()
  }

  /// Returns the maximum number of elements in each guest table.
  #[must_use]
  pub fn table_elements(&self) -> usize {
    self.table_elements
  }

  /// Returns the maximum number of tables in one plugin store.
  #[must_use]
  pub fn tables(&self) -> usize {
    self.tables
  }

  /// Returns the wall-clock timeout for plugin instantiation and invocation.
  ///
  /// CPU-bound guest interruption uses a 10 millisecond epoch cadence, so
  /// enforcement is intentionally coarse rather than real-time.
  #[must_use]
  pub fn timeout(&self) -> Duration {
    self.timeout
  }
}

impl Default for RuntimeLimits {
  fn default() -> Self {
    Self {
      fuel: Self::DEFAULT_FUEL,
      instances: Self::DEFAULT_INSTANCES,
      memories: Self::DEFAULT_MEMORIES,
      memory_size: Self::DEFAULT_MEMORY_SIZE,
      table_elements: Self::DEFAULT_TABLE_ELEMENTS,
      tables: Self::DEFAULT_TABLES,
      timeout: Self::DEFAULT_TIMEOUT,
    }
  }
}

/// Builds [`RuntimeLimits`].
#[derive(Clone, Copy, Debug, Default)]
#[must_use]
pub struct RuntimeLimitsBuilder {
  limits: RuntimeLimits,
}

impl RuntimeLimitsBuilder {
  /// Builds the configured limits.
  #[must_use]
  pub fn build(self) -> RuntimeLimits {
    self.limits
  }

  /// Sets the fuel available to one plugin store.
  pub fn fuel(mut self, fuel: u64) -> Self {
    self.limits.fuel = fuel;
    self
  }

  /// Sets the maximum number of core instances in one plugin store.
  pub fn instances(mut self, instances: usize) -> Self {
    self.limits.instances = instances;
    self
  }

  /// Sets the maximum number of linear memories in one plugin store.
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

  /// Sets the maximum number of tables in one plugin store.
  pub fn tables(mut self, tables: usize) -> Self {
    self.limits.tables = tables;
    self
  }

  /// Sets the wall-clock timeout for plugin instantiation and invocation.
  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.limits.timeout = timeout;
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn restriction_uses_tighter_limits() {
    let ceiling = RuntimeLimits::builder()
      .fuel(100)
      .memory_size(100)
      .timeout(Duration::from_secs(10))
      .build();

    let requested = RuntimeLimits::builder()
      .fuel(50)
      .memory_size(200)
      .timeout(Duration::from_secs(5))
      .build();

    let effective = ceiling.restrict(requested);

    assert_eq!(effective.fuel(), 50);
    assert_eq!(effective.memory_size(), 100);
    assert_eq!(effective.timeout(), Duration::from_secs(5));
  }
}
