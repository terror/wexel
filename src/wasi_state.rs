use super::*;

/// Per-instance WASI state using restricted defaults.
///
/// No arguments, environment variables, stdio streams, filesystem preopens,
/// or network destinations are inherited from the host.
pub struct WasiState {
  context: WasiCtx,
  table: ResourceTable,
}

impl WasiState {
  /// Creates state using WASI's restricted defaults.
  #[must_use]
  pub fn new() -> Self {
    Self {
      context: WasiCtx::builder().build(),
      table: ResourceTable::new(),
    }
  }
}

impl Default for WasiState {
  fn default() -> Self {
    Self::new()
  }
}

impl WasiView for WasiState {
  fn ctx(&mut self) -> WasiCtxView<'_> {
    WasiCtxView {
      ctx: &mut self.context,
      table: &mut self.table,
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    wasmtime_wasi::{cli::WasiCliView, p2::bindings::cli::environment::Host},
  };

  #[test]
  fn default_has_no_process_context() {
    let mut state = WasiState::default();
    let mut cli = state.cli();

    assert_eq!(Host::get_arguments(&mut cli).unwrap(), Vec::<String>::new());
    assert_eq!(
      Host::get_environment(&mut cli).unwrap(),
      Vec::<(String, String)>::new()
    );
    assert_eq!(Host::initial_cwd(&mut cli).unwrap(), None);
  }
}
