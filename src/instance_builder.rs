use super::*;

type LinkerConfiguration<T> =
  Box<dyn FnOnce(&mut Linker<T>) -> wasmtime::Result<()> + Send>;
type StateFactory<T> = Box<dyn FnOnce(WasiState) -> T + Send>;

/// Configures and creates one isolated plugin instance.
///
/// Instantiation deadlines can preempt guest WebAssembly and yielding async
/// host calls. Application host functions are trusted and must not block their
/// executor thread.
#[must_use]
pub struct InstanceBuilder<T: 'static> {
  configurations: Vec<LinkerConfiguration<T>>,
  limits: Option<RuntimeLimits>,
  permissions: Permissions,
  plugin: Plugin,
  state_factory: StateFactory<T>,
  timeout: Option<Duration>,
}

impl<T: WasiStateView + 'static> InstanceBuilder<T> {
  /// Builds and instantiates the plugin.
  ///
  /// # Errors
  ///
  /// Returns an error if capabilities cannot be prepared, host interfaces
  /// cannot be linked, store configuration fails, instantiation fails, or the
  /// instantiation timeout expires.
  ///
  /// # Panics
  ///
  /// Panics unless called within a Tokio runtime with its time driver enabled.
  pub async fn build(self) -> Result<Instance<T>> {
    let Self {
      configurations,
      limits,
      permissions,
      plugin,
      state_factory,
      timeout,
    } = self;

    let mut limits = limits.map_or(plugin.runtime.limits, |requested| {
      plugin.runtime.limits.restrict(requested)
    });

    if let Some(requested) = timeout {
      limits.timeout = limits.timeout.min(requested);
    }

    let timeout = limits.timeout;
    let started = tokio::time::Instant::now();

    let state = state_factory(permissions.wasi_state()?);
    let mut linker = plugin.runtime.linker::<T>()?;

    for configure in configurations {
      configure(&mut linker)
        .map_err(|source| Error::LinkerConfiguration { source })?;
    }

    let mut store = plugin.runtime.store_with_limits(state, limits)?;

    store.epoch_deadline_async_yield_and_update(1);
    store.set_epoch_deadline(1);

    let elapsed = started.elapsed();

    let Some(remaining) = timeout
      .checked_sub(elapsed)
      .filter(|remaining| !remaining.is_zero())
    else {
      return Err(Error::InstantiationTimeout { timeout });
    };

    let result = tokio::time::timeout(
      remaining,
      linker.instantiate_async(&mut store, plugin.component()),
    )
    .await;

    let Ok(result) = result else {
      return Err(Error::InstantiationTimeout { timeout });
    };

    if started.elapsed() >= timeout {
      return Err(Error::InstantiationTimeout { timeout });
    }

    let instance = result.map_err(Error::instantiation)?;

    Ok(Instance::new(store, instance, limits))
  }

  /// Adds application-defined WIT host interfaces to the instance linker.
  pub fn configure_linker(
    mut self,
    configure: impl FnOnce(&mut Linker<T>) -> wasmtime::Result<()> + Send + 'static,
  ) -> Self {
    self.configurations.push(Box::new(configure));
    self
  }

  /// Requests instance limits. Runtime limits remain hard ceilings.
  pub fn limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = Some(limits);
    self
  }

  pub(crate) fn new(
    plugin: Plugin,
    state_factory: impl FnOnce(WasiState) -> T + Send + 'static,
  ) -> Self {
    Self {
      configurations: Vec::new(),
      limits: None,
      permissions: Permissions::none(),
      plugin,
      state_factory: Box::new(state_factory),
      timeout: None,
    }
  }

  /// Grants effective capabilities to this plugin instance.
  pub fn permissions(mut self, permissions: Permissions) -> Self {
    self.permissions = permissions;
    self
  }

  /// Requests a default operation timeout. The runtime timeout remains a hard
  /// ceiling.
  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.timeout = Some(timeout);
    self
  }
}
