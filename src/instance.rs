use super::*;

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
