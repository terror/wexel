use {
  std::future::{Future, ready},
  wasmtime::{Trap, component::HasSelf},
  wexel::{Runtime, WasiCtxView, WasiState, WasiStateView, WasiView},
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

mod fuel_bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/fuel.wit",
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

mod memory_bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/memory.wit",
    world: "plugin",
    exports: { default: async },
  });
}

mod table_bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/table.wit",
    world: "plugin",
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

impl WasiStateView for HostState {
  fn wasi_state(&mut self) -> &mut WasiState {
    &mut self.wasi
  }
}

#[tokio::test]
async fn fuel_exhaustion_interrupts_component() {
  let runtime = Runtime::builder().fuel(100_000).build().unwrap();
  let bytes = wat::parse_file("tests/fixtures/fuel.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = runtime.store(WasiState::new()).unwrap();

  let bindings = fuel_bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();
  let error = bindings.call_run(&mut store).await.unwrap_err();

  assert_eq!(error.downcast::<Trap>().unwrap(), Trap::OutOfFuel);
}

#[tokio::test]
async fn invokes_typed_component() {
  let runtime = Runtime::new().unwrap();
  let bytes = wat::parse_file("tests/fixtures/answer.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = runtime.store(WasiState::new()).unwrap();

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
async fn memory_growth_respects_limit() {
  let runtime = Runtime::builder().memory_size(64 * 1024).build().unwrap();
  let bytes = wat::parse_file("tests/fixtures/memory.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = runtime.store(WasiState::new()).unwrap();

  let bindings = memory_bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_grow(&mut store).await.unwrap(), -1);
}

#[tokio::test]
async fn memory_count_rejects_component() {
  let runtime = Runtime::builder().memories(1).build().unwrap();
  let bytes = wat::parse_file("tests/fixtures/memory-count.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = runtime.store(WasiState::new()).unwrap();

  assert!(
    linker
      .instantiate_async(&mut store, plugin.component())
      .await
      .is_err()
  );
}

#[tokio::test]
async fn table_growth_respects_limit() {
  let runtime = Runtime::builder().table_elements(1).build().unwrap();
  let bytes = wat::parse_file("tests/fixtures/table.wat").unwrap();
  let plugin = runtime.load_bytes(bytes).unwrap();
  let linker = runtime.linker::<WasiState>().unwrap();
  let mut store = runtime.store(WasiState::new()).unwrap();

  let bindings = table_bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_grow(&mut store).await.unwrap(), -1);
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

  let mut store = runtime
    .store(HostState {
      answer: 42,
      wasi: WasiState::new(),
    })
    .unwrap();

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
  let mut store = runtime.store(WasiState::new()).unwrap();

  let bindings = bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_answer(&mut store).await.unwrap(), 42);
}
