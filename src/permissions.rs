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
#[derive(Clone, Default, bon::Builder)]
#[builder(
  derive(Clone),
  builder_type(doc {
    /// Builds effective plugin permissions.
  }),
  finish_fn(doc {
    /// Builds the configured permissions.
  }),
  start_fn(doc {
    /// Creates a permissions builder with no capabilities granted.
  })
)]
pub struct Permissions {
  #[builder(field)]
  directories: Vec<DirectoryGrant>,
  #[builder(field)]
  environment: Vec<(String, String)>,
  #[builder(field)]
  tcp_addresses: Vec<SocketAddr>,
}

impl Permissions {
  /// Creates an empty permission set.
  #[must_use]
  pub fn none() -> Self {
    Self::default()
  }

  /// Returns the exact socket addresses granted for outbound TCP connections.
  #[must_use]
  pub fn tcp_addresses(&self) -> &[SocketAddr] {
    &self.tcp_addresses
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

    for address in &self.tcp_addresses {
      builder = builder.tcp(*address);
    }

    Ok(builder.build())
  }
}

impl Default for PermissionsBuilder {
  fn default() -> Self {
    Permissions::builder()
  }
}

impl<S: permissions_builder::State> PermissionsBuilder<S> {
  /// Exposes one explicitly provided environment variable to the guest.
  pub fn env(
    mut self,
    key: impl Into<String>,
    value: impl Into<String>,
  ) -> Self {
    self.environment.push((key.into(), value.into()));
    self
  }

  /// Grants read-only access to a host directory at `guest_path`.
  pub fn read_dir(
    mut self,
    host_path: impl Into<PathBuf>,
    guest_path: impl Into<String>,
  ) -> Self {
    self.directories.push(DirectoryGrant {
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
    self.directories.push(DirectoryGrant {
      access: DirectoryAccess::ReadWrite,
      guest_path: guest_path.into(),
      host_path: host_path.into(),
    });

    self
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
  use super::*;

  #[test]
  fn tcp_addresses_are_inspectable() {
    let address = "192.0.2.1:443".parse().unwrap();

    let permissions = Permissions::builder().tcp(address).build();

    assert_eq!(permissions.tcp_addresses(), &[address]);
  }
}
