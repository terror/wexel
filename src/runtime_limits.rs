use super::*;

/// Resource and execution limits applied to one plugin instance.
#[allow(clippy::arbitrary_source_item_ordering)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, bon::Builder)]
#[builder(
  derive(Clone, Debug),
  builder_type(doc {
    /// Builds [`RuntimeLimits`].
  }),
  finish_fn(doc {
    /// Builds the configured limits.
  }),
  start_fn(doc {
    /// Creates a runtime limits builder with secure defaults.
  })
)]
pub struct RuntimeLimits {
  /// Sets the fuel available to one plugin store.
  #[builder(default = RuntimeLimits::DEFAULT_FUEL)]
  pub(crate) fuel: u64,
  /// Sets the maximum number of core instances in one plugin store.
  #[builder(default = RuntimeLimits::DEFAULT_INSTANCES)]
  pub(crate) instances: usize,
  /// Sets the maximum number of linear memories in one plugin store.
  #[builder(default = RuntimeLimits::DEFAULT_MEMORIES)]
  pub(crate) memories: usize,
  /// Sets the maximum size in bytes of each guest linear memory.
  #[builder(default = RuntimeLimits::DEFAULT_MEMORY_SIZE)]
  pub(crate) memory_size: usize,
  /// Sets the maximum number of elements in each guest table.
  #[builder(default = RuntimeLimits::DEFAULT_TABLE_ELEMENTS)]
  pub(crate) table_elements: usize,
  /// Sets the maximum number of tables in one plugin store.
  #[builder(default = RuntimeLimits::DEFAULT_TABLES)]
  pub(crate) tables: usize,
  /// Sets the wall-clock timeout for plugin instantiation and invocation.
  #[builder(default = RuntimeLimits::DEFAULT_TIMEOUT)]
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

impl Default for RuntimeLimitsBuilder {
  fn default() -> Self {
    RuntimeLimits::builder()
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
