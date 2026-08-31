## wexel

A capability-based WebAssembly plugin runtime for Rust applications, built on
the WebAssembly Component Model and Wasmtime.

## Status

Early development. The API is not yet stable.

## Capabilities

Everything is denied by default; grants are explicit per plugin instance.

- `read_dir` mounts one host directory read-only at a guest path
- `read_write_dir` mounts one host directory read-write at a guest path
- `env` exposes one host-provided environment variable
- `tcp` permits outbound connections to one exact IP address and port

TCP grants never enable DNS, UDP, listening sockets, or inbound connections.

## Security notes

- Plugins are assumed malicious. Enforcement happens at the host boundary, and
  plugin-declared metadata is advisory.
- Filesystem access is structurally sandboxed through preopened directories;
  traversal and symlink escapes are rejected by the capability layer.
- With TCP enabled, a guest can allocate sockets up to host file-descriptor
  limits. Guest resource use is bounded by fuel, memory, and timeout limits,
  not by live-resource caps. Hosts should apply OS-level limits for defense in
  depth.
