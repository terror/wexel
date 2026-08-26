use {
  std::{
    fmt::Debug,
    future::{Future, ready},
    ops::AsyncFnOnce,
  },
  wasmtime::{
    Store, Trap,
    component::{ComponentNamedList, HasSelf, Instance, Lift, Linker},
  },
  wexel::{
    Runtime, RuntimeBuilder, WasiCtxView, WasiState, WasiStateView, WasiView,
  },
};

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

struct HostState {
  answer: u32,
  wasi: WasiState,
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

struct Test<T> {
  fixture: &'static str,
  runtime: RuntimeBuilder,
  state: T,
}

impl Test<WasiState> {
  fn new(fixture: &'static str) -> Self {
    Self {
      fixture,
      runtime: Runtime::builder(),
      state: WasiState::new(),
    }
  }
}

impl<T> Test<T> {
  fn fuel(mut self, fuel: u64) -> Self {
    self.runtime = self.runtime.fuel(fuel);
    self
  }

  fn memories(mut self, memories: usize) -> Self {
    self.runtime = self.runtime.memories(memories);
    self
  }

  fn memory_size(mut self, memory_size: usize) -> Self {
    self.runtime = self.runtime.memory_size(memory_size);
    self
  }

  fn state<U>(self, state: U) -> Test<U> {
    Test {
      fixture: self.fixture,
      runtime: self.runtime,
      state,
    }
  }

  fn table_elements(mut self, table_elements: usize) -> Self {
    self.runtime = self.runtime.table_elements(table_elements);
    self
  }
}

impl<T: WasiStateView + 'static> Test<T> {
  async fn call<R>(self, export: &str) -> wasmtime::Result<R>
  where
    R: ComponentNamedList + Lift + 'static,
  {
    let (mut store, instance) = self.instantiate(|_| {}).await?;

    let function = instance.get_typed_func::<(), R>(&mut store, export)?;

    function.call_async(&mut store, ()).await
  }

  async fn expect<R>(self, export: &str, expected: R)
  where
    R: ComponentNamedList + Debug + Lift + PartialEq + 'static,
  {
    assert_eq!(self.call::<R>(export).await.unwrap(), expected);
  }

  async fn expect_instantiation_error(self, expected: &str) {
    let error = self.instantiate(|_| {}).await.map(|_| ()).unwrap_err();

    assert_eq!(error.to_string(), expected);
  }

  async fn expect_trap(self, export: &str, expected: Trap) {
    let error = self.call::<()>(export).await.unwrap_err();

    assert_eq!(error.downcast::<Trap>().unwrap(), expected);
  }

  async fn instantiate(
    self,
    configure: impl FnOnce(&mut Linker<T>),
  ) -> wasmtime::Result<(Store<T>, Instance)> {
    let runtime = self.runtime.build().unwrap();

    let bytes =
      wat::parse_file(format!("tests/fixtures/{}.wat", self.fixture)).unwrap();

    let plugin = runtime.load_bytes(bytes).unwrap();

    let mut linker = runtime.linker::<T>().unwrap();

    configure(&mut linker);

    let mut store = runtime.store(self.state).unwrap();

    let instance = linker
      .instantiate_async(&mut store, plugin.component())
      .await?;

    Ok((store, instance))
  }

  async fn run<F>(self, check: F)
  where
    F: for<'a> AsyncFnOnce(&'a mut Store<T>, &'a Instance),
  {
    self.run_with(|_| {}, check).await;
  }

  async fn run_with<C, F>(self, configure: C, check: F)
  where
    C: FnOnce(&mut Linker<T>),
    F: for<'a> AsyncFnOnce(&'a mut Store<T>, &'a Instance),
  {
    let (mut store, instance) = self.instantiate(configure).await.unwrap();

    check(&mut store, &instance).await;
  }
}

#[tokio::test]
async fn environment_exposes_only_configured_values() {
  Test::new("environment")
    .state(WasiState::builder().env("WEXEL_TEST", "configured").build())
    .expect(
      "environment",
      (vec![("WEXEL_TEST".to_owned(), "configured".to_owned())],),
    )
    .await;
}

#[tokio::test]
async fn fuel_exhaustion_interrupts_component() {
  Test::new("fuel")
    .fuel(100_000)
    .expect_trap("run", Trap::OutOfFuel)
    .await;
}

#[tokio::test]
async fn invokes_typed_component() {
  Test::new("answer")
    .run(async |store, instance| {
      let bindings = bindings::Plugin::new(&mut *store, instance).unwrap();

      assert_eq!(bindings.call_answer(&mut *store).await.unwrap(), 42);
    })
    .await;
}

#[tokio::test]
async fn memory_growth_respects_limit() {
  Test::new("memory")
    .memory_size(64 * 1024)
    .expect("grow", (-1_i32,))
    .await;
}

#[tokio::test]
async fn memory_count_rejects_component() {
  Test::new("memory-count")
    .memories(1)
    .expect_instantiation_error(
      "resource limit exceeded: memory count too high at 2",
    )
    .await;
}

#[tokio::test]
async fn table_growth_respects_limit() {
  Test::new("table")
    .table_elements(1)
    .expect("grow", (-1_i32,))
    .await;
}

#[tokio::test]
async fn typed_component_calls_host_function() {
  Test::new("host")
    .state(HostState {
      answer: 42,
      wasi: WasiState::new(),
    })
    .run_with(
      |linker| {
        host_bindings::Plugin::add_to_linker::<_, HasSelf<_>>(
          linker,
          |state| state,
        )
        .unwrap();
      },
      async |store, instance| {
        let bindings =
          host_bindings::Plugin::new(&mut *store, instance).unwrap();

        assert_eq!(bindings.call_answer(&mut *store).await.unwrap(), 42);
      },
    )
    .await;
}

#[tokio::test]
async fn wasi_component_imports_are_linked() {
  Test::new("wasi")
    .run(async |store, instance| {
      let bindings = bindings::Plugin::new(&mut *store, instance).unwrap();

      assert_eq!(bindings.call_answer(&mut *store).await.unwrap(), 42);
    })
    .await;
}
