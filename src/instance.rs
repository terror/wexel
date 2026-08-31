use super::*;

type LinkerConfiguration<T> =
  Box<dyn FnOnce(&mut Linker<T>) -> wasmtime::Result<()> + Send>;

type StateFactory<T> = Box<dyn FnOnce(WasiState) -> T + Send>;

/// A running plugin with isolated store state.
///
/// Managed deadlines can preempt guest WebAssembly and yielding async host
/// calls. Application host functions are trusted and must not block their
/// executor thread. Timing out cancels the in-flight call but cannot roll back
/// guest memory, filesystem changes, network requests, or other host effects.
pub struct Instance<T: 'static> {
  limits: RuntimeLimits,
  raw: WasmtimeInstance,
  store: Store<T>,
  timed_out: bool,
}

#[bon::bon]
impl<T: WasiStateView + 'static> Instance<T> {
  #[allow(clippy::arbitrary_source_item_ordering)]
  #[builder(
    builder_type(
      name = InstanceBuilder,
      vis = "pub",
      doc {
        /// Configures and creates one isolated plugin instance.
        ///
        /// Instantiation deadlines can preempt guest WebAssembly and yielding
        /// async host calls. Application host functions are trusted and must
        /// not block their executor thread.
      }
    ),
    finish_fn(
      name = build,
      doc {
        /// Builds and instantiates the plugin.
        ///
        /// # Errors
        ///
        /// Returns an error if capabilities cannot be prepared, host
        /// interfaces cannot be linked, store configuration fails,
        /// instantiation fails, or the instantiation timeout expires.
        ///
        /// # Panics
        ///
        /// Panics unless called within a Tokio runtime with its time driver
        /// enabled.
      }
    ),
    start_fn(name = builder, vis = "pub(crate)")
  )]
  async fn assemble(
    #[builder(start_fn)] plugin: Plugin,
    #[builder(start_fn)] state_factory: StateFactory<T>,
    #[builder(field)] configurations: Vec<LinkerConfiguration<T>>,
    /// Requests instance limits. Runtime limits remain hard ceilings.
    limits: Option<RuntimeLimits>,
    /// Grants effective capabilities to this plugin instance.
    #[builder(default = Permissions::none())]
    permissions: Permissions,
    /// Requests a default operation timeout. The runtime timeout remains a hard
    /// ceiling.
    timeout: Option<Duration>,
  ) -> Result<Self> {
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

    Ok(Self::new(store, instance, limits))
  }
}

impl<T: 'static> Instance<T> {
  /// Consumes this managed instance and returns its raw Wasmtime parts.
  ///
  /// Calls made through the returned store bypass Wexel invocation deadlines
  /// and structured error mapping.
  #[must_use]
  pub fn into_parts(self) -> (Store<T>, WasmtimeInstance) {
    (self.store, self.raw)
  }

  /// Invokes a typed operation using this instance's default timeout.
  ///
  /// # Errors
  ///
  /// Returns an error if the plugin traps, exhausts its fuel, exceeds its
  /// timeout, or the operation otherwise fails. An instance that times out is
  /// unavailable for subsequent managed invocations.
  ///
  /// # Panics
  ///
  /// Panics unless called within a Tokio runtime with its time driver enabled.
  pub async fn invoke<F, R>(&mut self, call: F) -> Result<R>
  where
    F: for<'a> AsyncFnOnce(
      &'a mut Store<T>,
      &'a WasmtimeInstance,
    ) -> wasmtime::Result<R>,
  {
    self.invoke_with_timeout(self.limits.timeout, call).await
  }

  /// Invokes a typed operation with a timeout no greater than the instance
  /// default.
  ///
  /// # Errors
  ///
  /// Returns an error if the plugin traps, exhausts its fuel, exceeds its
  /// timeout, or the operation otherwise fails. An instance that times out is
  /// unavailable for subsequent managed invocations.
  ///
  /// # Panics
  ///
  /// Panics unless called within a Tokio runtime with its time driver enabled.
  pub async fn invoke_with_timeout<F, R>(
    &mut self,
    timeout: Duration,
    call: F,
  ) -> Result<R>
  where
    F: for<'a> AsyncFnOnce(
      &'a mut Store<T>,
      &'a WasmtimeInstance,
    ) -> wasmtime::Result<R>,
  {
    if self.timed_out {
      return Err(Error::InstanceUnavailable);
    }

    let timeout = self.limits.timeout.min(timeout);

    self.store.epoch_deadline_async_yield_and_update(1);
    self.store.set_epoch_deadline(1);

    let started = tokio::time::Instant::now();

    if let Ok(result) =
      tokio::time::timeout(timeout, call(&mut self.store, &self.raw)).await
      && started.elapsed() < timeout
    {
      result.map_err(Error::invocation)
    } else {
      self.timed_out = true;
      Err(Error::InvocationTimeout { timeout })
    }
  }

  /// Returns the effective limits applied to this instance.
  #[must_use]
  pub fn limits(&self) -> RuntimeLimits {
    self.limits
  }

  pub(crate) fn new(
    store: Store<T>,
    instance: WasmtimeInstance,
    limits: RuntimeLimits,
  ) -> Self {
    Self {
      limits,
      raw: instance,
      store,
      timed_out: false,
    }
  }

  /// Returns the raw store and component instance.
  ///
  /// Calls made through these values bypass Wexel invocation deadlines and
  /// structured error mapping.
  pub fn parts_mut(&mut self) -> (&mut Store<T>, &WasmtimeInstance) {
    (&mut self.store, &self.raw)
  }

  /// Returns mutable access to the raw Wasmtime store.
  ///
  /// Calls made through this store bypass Wexel invocation deadlines and
  /// structured error mapping.
  pub fn store_mut(&mut self) -> &mut Store<T> {
    &mut self.store
  }

  /// Returns the default wall-clock timeout for managed invocations.
  #[must_use]
  pub fn timeout(&self) -> Duration {
    self.limits.timeout
  }

  /// Returns the raw Wasmtime component instance.
  #[must_use]
  pub fn wasmtime_instance(&self) -> &WasmtimeInstance {
    &self.raw
  }
}

impl<T: WasiStateView + 'static, S: instance_builder::State>
  InstanceBuilder<T, S>
{
  /// Adds application-defined WIT host interfaces to the instance linker.
  pub fn configure_linker(
    mut self,
    configure: impl FnOnce(&mut Linker<T>) -> wasmtime::Result<()> + Send + 'static,
  ) -> Self {
    self.configurations.push(Box::new(configure));
    self
  }
}
