use super::*;

#[must_use]
pub struct WasiStateBuilder {
  context: WasiCtxBuilder,
}

impl WasiStateBuilder {
  /// Builds per-instance WASI state.
  #[must_use]
  pub fn build(mut self) -> WasiState {
    WasiState {
      context: self.context.build(),
      limits: StoreLimits::default(),
      table: ResourceTable::new(),
    }
  }

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
}

impl Default for WasiStateBuilder {
  fn default() -> Self {
    Self {
      context: WasiCtx::builder(),
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
        cli::environment::Host as EnvironmentHost,
        filesystem::{
          preopens::Host as PreopensHost,
          types::{
            DescriptorFlags, ErrorCode as FilesystemErrorCode, HostDescriptor,
            OpenFlags, PathFlags,
          },
        },
      },
    },
  };

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
}
