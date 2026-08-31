use super::*;

pub struct WasiState {
  pub(crate) context: WasiCtx,
  pub(crate) limits: StoreLimits,
  pub(crate) table: ResourceTable,
}

#[bon::bon]
impl WasiState {
  #[builder(
    builder_type(
      name = WasiStateBuilder,
      vis = "pub",
      doc {
        /// Configures guest capabilities and builds per-instance WASI state.
      }
    ),
    finish_fn(
      name = build,
      doc {
        /// Builds per-instance WASI state.
      }
    ),
    start_fn(name = builder, vis = "pub")
  )]
  fn assemble(
    #[builder(field = WasiCtx::builder())] context: WasiCtxBuilder,
    #[builder(field)] tcp_addresses: Vec<SocketAddr>,
  ) -> Self {
    let mut context = context;

    if !tcp_addresses.is_empty() {
      context
        .allow_tcp(true)
        .socket_addr_check(move |address, use_| {
          let allowed = match use_ {
            SocketAddrUse::TcpBind => {
              address.ip().is_unspecified() && address.port() == 0
            }
            SocketAddrUse::TcpConnect => tcp_addresses.contains(&address),
            _ => false,
          };

          Box::pin(std::future::ready(allowed))
        });
    }

    Self {
      context: context.build(),
      limits: StoreLimits::default(),
      table: ResourceTable::new(),
    }
  }
}

impl WasiState {
  pub(crate) fn limits(&mut self) -> &mut StoreLimits {
    &mut self.limits
  }

  /// Creates state using WASI's restricted defaults.
  #[must_use]
  pub fn new() -> Self {
    Self::builder().build()
  }

  pub(crate) fn set_limits(&mut self, limits: StoreLimits) {
    self.limits = limits;
  }
}

impl Default for WasiStateBuilder {
  fn default() -> Self {
    WasiState::builder()
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

impl WasiStateView for WasiState {
  fn wasi_state(&mut self) -> &mut WasiState {
    self
  }
}

impl<S: wasi_state_builder::State> WasiStateBuilder<S> {
  /// Exposes one environment variable to the guest.
  pub fn env(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
    self.context.env(key, value);
    self
  }

  fn mount_dir(
    mut self,
    host_path: impl AsRef<Path>,
    guest_path: impl AsRef<str>,
    permissions: FsPerms,
  ) -> Result<Self> {
    let guest_path = guest_path.as_ref();

    let host_path = host_path.as_ref();

    self
      .context
      .preopened_dir(host_path, guest_path, permissions)
      .map_err(|source| Error::Directory {
        guest_path: guest_path.to_owned(),
        host_path: host_path.to_owned(),
        source,
      })?;

    Ok(self)
  }

  /// Exposes a host directory read-only at `guest_path`.
  ///
  /// # Errors
  ///
  /// Returns an error if the host directory cannot be opened.
  pub fn read_dir(
    self,
    host_path: impl AsRef<Path>,
    guest_path: impl AsRef<str>,
  ) -> Result<Self> {
    self.mount_dir(host_path, guest_path, FsPerms::ReadOnly)
  }

  /// Exposes a host directory read-write at `guest_path`.
  ///
  /// # Errors
  ///
  /// Returns an error if the host directory cannot be opened.
  pub fn read_write_dir(
    self,
    host_path: impl AsRef<Path>,
    guest_path: impl AsRef<str>,
  ) -> Result<Self> {
    self.mount_dir(host_path, guest_path, FsPerms::ReadWrite)
  }

  /// Grants outbound TCP access to one exact IP address and port.
  ///
  /// This does not enable DNS, UDP, inbound connections, or listening sockets.
  pub fn tcp(mut self, address: impl Into<SocketAddr>) -> Self {
    self.tcp_addresses.push(address.into());
    self
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    std::net::TcpListener,
    wasmtime::component::Resource,
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
        filesystem::{
          preopens::Host as PreopensHost,
          types::{
            DescriptorFlags, ErrorCode as FilesystemErrorCode, HostDescriptor,
            OpenFlags, PathFlags,
          },
        },
        sockets::{
          instance_network::Host as InstanceNetworkHost,
          ip_name_lookup::Host as IpNameLookupHost,
          network::{ErrorCode as NetworkErrorCode, IpAddressFamily},
          tcp::HostTcpSocket as TcpSocketHost,
          tcp_create_socket::Host as TcpCreateSocketHost,
          udp_create_socket::Host as UdpCreateSocketHost,
        },
      },
      sockets::WasiSocketsView,
    },
  };

  async fn finish_tcp_connect(
    state: &mut WasiState,
    socket: u32,
  ) -> std::result::Result<(), NetworkErrorCode> {
    tokio::time::timeout(Duration::from_secs(1), async {
      loop {
        match TcpSocketHost::finish_connect(
          &mut state.sockets(),
          Resource::new_borrow(socket),
        ) {
          Ok(_) => return Ok(()),
          Err(source) => {
            let error: NetworkErrorCode = source.downcast().unwrap();

            if error != NetworkErrorCode::WouldBlock {
              return Err(error);
            }

            tokio::task::yield_now().await;
          }
        }
      }
    })
    .await
    .expect("TCP connection did not finish")
  }

  fn tcp_resources(
    state: &mut WasiState,
    family: IpAddressFamily,
  ) -> (u32, u32) {
    let network =
      InstanceNetworkHost::instance_network(&mut state.sockets()).unwrap();

    let socket =
      TcpCreateSocketHost::create_tcp_socket(&mut state.sockets(), family)
        .unwrap();

    (network.rep(), socket.rep())
  }

  async fn listening_denied_for(family: IpAddressFamily, wildcard: &str) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();

    let mut state = WasiState::builder()
      .tcp(listener.local_addr().unwrap())
      .build();

    let (network, socket) = tcp_resources(&mut state, family);

    let wildcard: SocketAddr = wildcard.parse().unwrap();

    TcpSocketHost::start_bind(
      &mut state.sockets(),
      Resource::new_borrow(socket),
      Resource::new_borrow(network),
      wildcard.into(),
    )
    .await
    .unwrap();

    TcpSocketHost::finish_bind(
      &mut state.sockets(),
      Resource::new_borrow(socket),
    )
    .unwrap();

    let error = TcpSocketHost::start_listen(
      &mut state.sockets(),
      Resource::new_borrow(socket),
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, NetworkErrorCode::AccessDenied);
  }

  #[test]
  fn environment_is_explicitly_configured() {
    let mut state =
      WasiState::builder().env("WEXEL_TEST", "configured").build();

    let environment =
      EnvironmentHost::get_environment(&mut state.cli()).unwrap();

    assert_eq!(
      environment,
      vec![("WEXEL_TEST".into(), "configured".into())]
    );
  }

  #[tokio::test]
  async fn read_directory_allows_reading() {
    let directory = tempfile::tempdir().unwrap();

    fs::write(directory.path().join("file"), "contents").unwrap();

    let mut state = WasiState::builder()
      .read_dir(directory.path(), "/workspace")
      .unwrap()
      .build();

    let descriptor = PreopensHost::get_directories(&mut state.filesystem())
      .unwrap()
      .pop()
      .unwrap()
      .0;

    let mut filesystem = state.filesystem();

    HostDescriptor::open_at(
      &mut filesystem,
      descriptor,
      PathFlags::empty(),
      "file".into(),
      OpenFlags::empty(),
      DescriptorFlags::READ,
    )
    .await
    .unwrap();
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn read_directory_denies_symlink_escape() {
    let directory = tempfile::tempdir().unwrap();

    let workspace = directory.path().join("workspace");
    let outside = directory.path().join("outside");

    fs::create_dir(&workspace).unwrap();
    fs::write(&outside, "contents").unwrap();
    std::os::unix::fs::symlink(&outside, workspace.join("link")).unwrap();

    let mut state = WasiState::builder()
      .read_dir(&workspace, "/workspace")
      .unwrap()
      .build();

    let descriptor = PreopensHost::get_directories(&mut state.filesystem())
      .unwrap()
      .pop()
      .unwrap()
      .0;

    let error = HostDescriptor::open_at(
      &mut state.filesystem(),
      descriptor,
      PathFlags::SYMLINK_FOLLOW,
      "link".into(),
      OpenFlags::empty(),
      DescriptorFlags::READ,
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, FilesystemErrorCode::NotPermitted);
  }

  #[tokio::test]
  async fn read_directory_denies_traversal() {
    let directory = tempfile::tempdir().unwrap();

    let workspace = directory.path().join("workspace");

    fs::create_dir(&workspace).unwrap();
    fs::write(directory.path().join("outside"), "contents").unwrap();

    let mut state = WasiState::builder()
      .read_dir(&workspace, "/workspace")
      .unwrap()
      .build();

    let descriptor = PreopensHost::get_directories(&mut state.filesystem())
      .unwrap()
      .pop()
      .unwrap()
      .0;

    let error = HostDescriptor::open_at(
      &mut state.filesystem(),
      descriptor,
      PathFlags::empty(),
      "../outside".into(),
      OpenFlags::empty(),
      DescriptorFlags::READ,
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, FilesystemErrorCode::NotPermitted);
  }

  #[tokio::test]
  async fn read_directory_denies_writing() {
    let directory = tempfile::tempdir().unwrap();

    fs::write(directory.path().join("file"), "contents").unwrap();

    let mut state = WasiState::builder()
      .read_dir(directory.path(), "/workspace")
      .unwrap()
      .build();

    let descriptor = PreopensHost::get_directories(&mut state.filesystem())
      .unwrap()
      .pop()
      .unwrap()
      .0;

    let mut filesystem = state.filesystem();

    let error = HostDescriptor::open_at(
      &mut filesystem,
      descriptor,
      PathFlags::empty(),
      "file".into(),
      OpenFlags::empty(),
      DescriptorFlags::WRITE,
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, FilesystemErrorCode::NotPermitted);
  }

  #[test]
  fn read_directory_is_explicitly_configured() {
    let directory = tempfile::tempdir().unwrap();

    let mut state = WasiState::builder()
      .read_dir(directory.path(), "/workspace")
      .unwrap()
      .build();

    let directories =
      PreopensHost::get_directories(&mut state.filesystem()).unwrap();

    assert_eq!(
      directories
        .iter()
        .map(|(descriptor, path)| {
          (descriptor.rep(), descriptor.owned(), path.as_str())
        })
        .collect::<Vec<_>>(),
      vec![(0, true, "/workspace")]
    );
  }

  #[test]
  fn read_directory_reports_missing_host_path() {
    let directory = tempfile::tempdir().unwrap();

    let host_path = directory.path().join("missing");

    let error = WasiState::builder()
      .read_dir(&host_path, "/workspace")
      .err()
      .unwrap();

    assert_matches!(
      error,
      Error::Directory {
        guest_path,
        host_path: error_path,
        ..
      } if guest_path == "/workspace" && error_path == host_path
    );
  }

  #[tokio::test]
  async fn read_write_directory_allows_writing() {
    let directory = tempfile::tempdir().unwrap();

    let path = directory.path().join("file");

    fs::write(&path, "contents").unwrap();

    let mut state = WasiState::builder()
      .read_write_dir(directory.path(), "/workspace")
      .unwrap()
      .build();

    let descriptor = PreopensHost::get_directories(&mut state.filesystem())
      .unwrap()
      .pop()
      .unwrap()
      .0;

    {
      let mut filesystem = state.filesystem();

      let file = HostDescriptor::open_at(
        &mut filesystem,
        descriptor,
        PathFlags::empty(),
        "file".into(),
        OpenFlags::empty(),
        DescriptorFlags::WRITE,
      )
      .await
      .unwrap();

      HostDescriptor::write(&mut filesystem, file, b"updated!".to_vec(), 0)
        .await
        .unwrap();
    }

    assert_eq!(fs::read_to_string(path).unwrap(), "updated!");
  }

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

    assert_eq!(error, NetworkErrorCode::AccessDenied);
  }

  #[tokio::test]
  async fn tcp_address_allows_exact_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    let permissions = Permissions::builder().tcp(address).build();
    let mut state = permissions.wasi_state().unwrap();

    let (network, socket) = tcp_resources(&mut state, IpAddressFamily::Ipv4);

    TcpSocketHost::start_connect(
      &mut state.sockets(),
      Resource::new_borrow(socket),
      Resource::new_borrow(network),
      address.into(),
    )
    .unwrap();

    finish_tcp_connect(&mut state, socket).await.unwrap();
  }

  #[tokio::test]
  async fn tcp_address_denies_explicit_bind() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();

    let mut state = WasiState::builder()
      .tcp(listener.local_addr().unwrap())
      .build();

    let (network, socket) = tcp_resources(&mut state, IpAddressFamily::Ipv4);

    let error = TcpSocketHost::start_bind(
      &mut state.sockets(),
      Resource::new_borrow(socket),
      Resource::new_borrow(network),
      "127.0.0.1:0".parse::<SocketAddr>().unwrap().into(),
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, NetworkErrorCode::AccessDenied);
  }

  #[tokio::test]
  async fn tcp_address_denies_listening_after_wildcard_bind() {
    listening_denied_for(IpAddressFamily::Ipv4, "0.0.0.0:0").await;

    listening_denied_for(IpAddressFamily::Ipv6, "[::]:0").await;
  }

  #[tokio::test]
  async fn tcp_address_denies_other_connection() {
    let allowed = TcpListener::bind("127.0.0.1:0").unwrap();
    let denied = TcpListener::bind("127.0.0.1:0").unwrap();

    let mut state = WasiState::builder()
      .tcp(allowed.local_addr().unwrap())
      .build();

    let (network, socket) = tcp_resources(&mut state, IpAddressFamily::Ipv4);

    TcpSocketHost::start_connect(
      &mut state.sockets(),
      Resource::new_borrow(socket),
      Resource::new_borrow(network),
      denied.local_addr().unwrap().into(),
    )
    .unwrap();

    let error = finish_tcp_connect(&mut state, socket).await.unwrap_err();

    assert_eq!(error, NetworkErrorCode::AccessDenied);
  }

  #[tokio::test]
  async fn tcp_address_does_not_enable_udp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();

    let mut state = WasiState::builder()
      .tcp(listener.local_addr().unwrap())
      .build();

    let error = UdpCreateSocketHost::create_udp_socket(
      &mut state.sockets(),
      IpAddressFamily::Ipv4,
    )
    .await
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, NetworkErrorCode::AccessDenied);
  }

  #[test]
  fn tcp_address_does_not_enable_name_lookup() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();

    let mut state = WasiState::builder()
      .tcp(listener.local_addr().unwrap())
      .build();

    let network =
      InstanceNetworkHost::instance_network(&mut state.sockets()).unwrap();

    let error = IpNameLookupHost::resolve_addresses(
      &mut state.sockets(),
      Resource::new_borrow(network.rep()),
      "example.com".into(),
    )
    .unwrap_err()
    .downcast()
    .unwrap();

    assert_eq!(error, NetworkErrorCode::PermanentResolverFailure);
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
