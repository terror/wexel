use {
  wasmtime::Store,
  wexel::{Runtime, WasiState},
};

mod bindings {
  wasmtime::component::bindgen!({
    path: "tests/fixtures/answer.wit",
    world: "plugin",
    exports: { default: async },
  });
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
