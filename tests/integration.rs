use {
  std::future::{Future, ready},
  wasmtime::{Store, component::HasSelf},
  wexel::{Runtime, WasiCtxView, WasiState, WasiView},
};

struct HostState {
  answer: u32,
  wasi: WasiState,
}

mod bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/answer.wit",
    world: "plugin",
    exports: { default: async },
  });
}

mod host_bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/host.wit",
    world: "plugin",
    imports: { default: async },
    exports: { default: async },
  });
}

impl host_bindings::PluginImports for HostState {
  fn host_answer(&mut self) -> impl Future<Output = u32> + Send {
    ready(self.answer)
  }
}

impl WasiView for HostState {
  fn ctx(&mut self) -> WasiCtxView<'_> {
    self.wasi.ctx()
  }
}

#[tokio::test]
async fn invokes_typed_component() {
  let runtime = Runtime::new().unwrap();
  let bytes = wat::parse_file("tests/fixtures/answer.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = Store::new(runtime.engine(), WasiState::new());

  let bindings = bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_answer(&mut store).await.unwrap(), 42);
}

#[tokio::test]
async fn typed_component_calls_host_function() {
  let runtime = Runtime::new().unwrap();
  let bytes = wat::parse_file("tests/fixtures/host.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let mut linker = runtime.linker::<HostState>().unwrap();

  host_bindings::Plugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| {
    state
  })
  .unwrap();

  let mut store = Store::new(
    runtime.engine(),
    HostState {
      answer: 42,
      wasi: WasiState::new(),
    },
  );

  let bindings = host_bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_answer(&mut store).await.unwrap(), 42);
}

#[tokio::test]
async fn wasi_component_imports_are_linked() {
  let runtime = Runtime::new().unwrap();
  let bytes = wat::parse_file("tests/fixtures/wasi.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = Store::new(runtime.engine(), WasiState::new());

  let bindings = bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_answer(&mut store).await.unwrap(), 42);
}
