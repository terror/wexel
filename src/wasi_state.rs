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
    wasmtime_wasi::{
      cli::WasiCliView,
      filesystem::WasiFilesystemView,
      p2::bindings::{
        cli::{
          environment::Host as EnvironmentHost,
          terminal_stderr::Host as TerminalStderrHost,
          terminal_stdin::Host as TerminalStdinHost,
          terminal_stdout::Host as TerminalStdoutHost,
        },
        filesystem::preopens::Host as PreopensHost,
        sockets::{
          network::{ErrorCode, IpAddressFamily},
          tcp_create_socket::Host as TcpCreateSocketHost,
        },
      },
      sockets::WasiSocketsView,
    },
  };

  #[test]
  fn default_has_no_filesystem_preopens() {
    let mut state = WasiState::default();
    let directories =
      PreopensHost::get_directories(&mut state.filesystem()).unwrap();

    assert!(directories.is_empty());
  }

  #[test]
  fn default_has_no_network_access() {
    let mut state = WasiState::default();
    let error = TcpCreateSocketHost::create_tcp_socket(
      &mut state.sockets(),
      IpAddressFamily::Ipv4,
    )
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, ErrorCode::AccessDenied);
  }

  #[test]
  fn default_has_no_process_context() {
    let mut state = WasiState::default();
    let mut cli = state.cli();

    assert_eq!(
      EnvironmentHost::get_arguments(&mut cli).unwrap(),
      Vec::<String>::new()
    );
    assert_eq!(
      EnvironmentHost::get_environment(&mut cli).unwrap(),
      Vec::<(String, String)>::new()
    );
    assert_eq!(EnvironmentHost::initial_cwd(&mut cli).unwrap(), None);
  }

  #[test]
  fn default_has_no_terminal_stdio() {
    let mut state = WasiState::default();
    let mut cli = state.cli();

    assert!(
      TerminalStderrHost::get_terminal_stderr(&mut cli)
        .unwrap()
        .is_none()
    );
    assert!(
      TerminalStdinHost::get_terminal_stdin(&mut cli)
        .unwrap()
        .is_none()
    );
    assert!(
      TerminalStdoutHost::get_terminal_stdout(&mut cli)
        .unwrap()
        .is_none()
    );
  }
}
