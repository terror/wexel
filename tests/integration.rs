use {
  wasmtime::{Store, component::Linker},
  wexel::Runtime,
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
  let linker = Linker::new(runtime.engine());
  let mut store = Store::new(runtime.engine(), ());

  let bindings = bindings::Plugin::instantiate_async(
    &mut store,
    plugin.component(),
    &linker,
  )
  .await
  .unwrap();

  assert_eq!(bindings.call_answer(&mut store).await.unwrap(), 42);
}
