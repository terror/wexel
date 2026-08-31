use super::*;

#[derive(Clone, Debug)]
enum DirectoryAccess {
  ReadOnly,
  ReadWrite,
}

#[derive(Clone, Debug)]
struct DirectoryGrant {
  access: DirectoryAccess,
  guest_path: String,
  host_path: PathBuf,
}

/// Effective capabilities granted by the host to one plugin instance.
#[derive(Clone, Default)]
pub struct Permissions {
  directories: Vec<DirectoryGrant>,
  environment: Vec<(String, String)>,
}

impl Permissions {
  /// Creates a permissions builder with no capabilities granted.
  pub fn builder() -> PermissionsBuilder {
    PermissionsBuilder::default()
  }

  /// Creates an empty permission set.
  #[must_use]
  pub fn none() -> Self {
    Self::default()
  }

  pub(crate) fn wasi_state(&self) -> Result<WasiState> {
    let mut builder = WasiState::builder();

    for (key, value) in &self.environment {
      builder = builder.env(key, value);
    }

    for directory in &self.directories {
      builder = match directory.access {
        DirectoryAccess::ReadOnly => {
          builder.read_dir(&directory.host_path, &directory.guest_path)?
        }
        DirectoryAccess::ReadWrite => {
          builder.read_write_dir(&directory.host_path, &directory.guest_path)?
        }
      };
    }

    Ok(builder.build())
  }
}

/// Builds effective plugin permissions.
#[derive(Clone, Default)]
#[must_use]
pub struct PermissionsBuilder {
  permissions: Permissions,
}

impl PermissionsBuilder {
  /// Builds the configured permissions.
  #[must_use]
  pub fn build(self) -> Permissions {
    self.permissions
  }

  /// Exposes one explicitly provided environment variable to the guest.
  pub fn env(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self
      .permissions
      .environment
      .push((key.into(), value.into()));
    self
  }

  /// Grants read-only access to a host directory at `guest_path`.
  pub fn read_dir(
    mut self,
    host_path: impl Into<PathBuf>,
    guest_path: impl Into<String>,
  ) -> Self {
    self.permissions.directories.push(DirectoryGrant {
      access: DirectoryAccess::ReadOnly,
      guest_path: guest_path.into(),
      host_path: host_path.into(),
    });
    self
  }

  /// Grants read-write access to a host directory at `guest_path`.
  pub fn read_write_dir(
    mut self,
    host_path: impl Into<PathBuf>,
    guest_path: impl Into<String>,
  ) -> Self {
    self.permissions.directories.push(DirectoryGrant {
      access: DirectoryAccess::ReadWrite,
      guest_path: guest_path.into(),
      host_path: host_path.into(),
    });
    self
  }
}
