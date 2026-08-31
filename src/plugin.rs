use super::*;

#[derive(Clone, Debug)]
pub struct Plugin {
  component: Component,
  pub(crate) runtime: Runtime,
}

impl Plugin {
  /// Returns the compiled Wasmtime component.
  ///
  /// Instantiating this component directly bypasses Wexel's managed limits,
  /// deadlines, and structured error mapping.
  #[must_use]
  pub fn component(&self) -> &Component {
    &self.component
  }

  /// Creates a managed instance builder using Wexel's default host state.
  pub fn instantiate(&self) -> InstanceBuilder<WasiState> {
    self.instantiate_with(std::convert::identity)
  }

  /// Creates a managed instance builder using application host state.
  ///
  /// The factory receives the permission-configured [`WasiState`] that the
  /// returned host state must retain and expose through [`WasiStateView`].
  pub fn instantiate_with<T: WasiStateView + 'static>(
    &self,
    state_factory: impl FnOnce(WasiState) -> T + Send + 'static,
  ) -> InstanceBuilder<T> {
    Instance::<T>::builder(self.clone(), Box::new(state_factory))
  }

  pub(crate) fn new(runtime: Runtime, component: Component) -> Self {
    Self { component, runtime }
  }
}
