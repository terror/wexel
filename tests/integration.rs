use {
  std::{
    fmt::Debug,
    future::{Future, ready},
    ops::AsyncFnOnce,
    pin::Pin,
    sync::{
      Arc,
      atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
  },
  wasmtime::{
    Store, Trap,
    component::{
      ComponentNamedList, HasSelf, Instance as WasmtimeInstance, Lift, Linker,
    },
  },
  wexel::{
    Error, Instance, Permissions, Runtime, RuntimeBuilder, RuntimeLimits,
    WasiCtxView, WasiState, WasiStateView, WasiView,
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

struct PendingAnswer {
  cancelled: Arc<AtomicBool>,
}

struct PendingHostState {
  cancelled: Arc<AtomicBool>,
  wasi: WasiState,
}

impl Drop for PendingAnswer {
  fn drop(&mut self) {
    self.cancelled.store(true, Ordering::SeqCst);
  }
}

impl Future for PendingAnswer {
  type Output = u32;

  fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
    Poll::Pending
  }
}

impl host_bindings::PluginImports for HostState {
  fn host_answer(&mut self) -> impl Future<Output = u32> + Send {
    ready(self.answer)
  }
}

impl host_bindings::PluginImports for PendingHostState {
  fn host_answer(&mut self) -> impl Future<Output = u32> + Send {
    PendingAnswer {
      cancelled: Arc::clone(&self.cancelled),
    }
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

impl WasiView for PendingHostState {
  fn ctx(&mut self) -> WasiCtxView<'_> {
    self.wasi.ctx()
  }
}

impl WasiStateView for PendingHostState {
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
  async fn call<R>(self, export: &str) -> wexel::Result<R>
  where
    R: ComponentNamedList + Lift + 'static,
  {
    let mut instance = self.instantiate(|_| {}).await?;

    instance
      .invoke(async |store, instance| {
        let function = instance.get_typed_func::<(), R>(&mut *store, export)?;

        function.call_async(store, ()).await
      })
      .await
  }

  async fn expect<R>(self, export: &str, expected: R)
  where
    R: ComponentNamedList + Debug + Lift + PartialEq + 'static,
  {
    assert_eq!(self.call::<R>(export).await.unwrap(), expected);
  }

  async fn expect_instantiation_error(self, expected: &str) {
    let error = self.instantiate(|_| {}).await.err().unwrap();

    assert!(matches!(
      error,
      Error::Instantiation { source } if source.to_string() == expected
    ));
  }

  async fn expect_trap(self, export: &str, expected: Trap) {
    let error = self.call::<()>(export).await.unwrap_err();

    match (error, expected) {
      (Error::FuelExhausted { .. }, Trap::OutOfFuel) => {}
      (Error::Trap { trap, .. }, expected) => assert_eq!(trap, expected),
      (error, expected) => {
        panic!("expected trap {expected:?}, got {error:?}")
      }
    }
  }

  async fn instantiate(
    self,
    configure: impl FnOnce(&mut Linker<T>) + Send + 'static,
  ) -> wexel::Result<Instance<T>> {
    let runtime = self.runtime.build().unwrap();

    let bytes =
      wat::parse_file(format!("tests/fixtures/{}.wat", self.fixture)).unwrap();

    let plugin = runtime.load_bytes(bytes).unwrap();

    plugin
      .instantiate_with(move |_| self.state)
      .configure_linker(move |linker| {
        configure(linker);
        Ok(())
      })
      .build()
      .await
  }

  async fn run<F>(self, check: F)
  where
    F: for<'a> AsyncFnOnce(&'a mut Store<T>, &'a WasmtimeInstance),
  {
    self.run_with(|_| {}, check).await;
  }

  async fn run_with<C, F>(self, configure: C, check: F)
  where
    C: FnOnce(&mut Linker<T>) + Send + 'static,
    F: for<'a> AsyncFnOnce(&'a mut Store<T>, &'a WasmtimeInstance),
  {
    let mut instance = self.instantiate(configure).await.unwrap();

    instance
      .invoke(async |store, instance| {
        check(store, instance).await;
        Ok(())
      })
      .await
      .unwrap();
  }
}

fn fixture(runtime: &Runtime, name: &str) -> wexel::Plugin {
  let bytes = wat::parse_file(format!("tests/fixtures/{name}.wat")).unwrap();

  runtime.load_bytes(bytes).unwrap()
}

fn require_send<T: Send>(_: T) {}

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
async fn instance_build_future_is_send() {
  let runtime = Runtime::new().unwrap();
  let plugin = fixture(&runtime, "answer");

  require_send(plugin.instantiate().build());
}

#[tokio::test]
async fn instantiation_fuel_exhaustion_is_structured() {
  let runtime = Runtime::builder().fuel(100_000).build().unwrap();
  let plugin = fixture(&runtime, "instantiation-timeout");

  assert!(matches!(
    plugin.instantiate().build().await.err().unwrap(),
    Error::FuelExhausted { .. }
  ));
}

#[tokio::test]
async fn instantiation_timeout_interrupts_component() {
  let timeout = Duration::from_millis(50);

  let runtime = Runtime::builder()
    .fuel(u64::MAX)
    .timeout(timeout)
    .build()
    .unwrap();

  let plugin = fixture(&runtime, "instantiation-timeout");

  let error = plugin.instantiate().build().await.err().unwrap();

  assert!(matches!(
    error,
    Error::InstantiationTimeout { timeout: actual } if actual == timeout
  ));
}

#[tokio::test]
async fn invocation_timeout_cancels_pending_host_call() {
  let timeout = Duration::from_millis(50);

  let runtime = Runtime::builder().timeout(timeout).build().unwrap();
  let plugin = fixture(&runtime, "host");
  let cancelled = Arc::new(AtomicBool::new(false));

  let mut instance = plugin
    .instantiate_with({
      let cancelled = Arc::clone(&cancelled);

      move |wasi| PendingHostState { cancelled, wasi }
    })
    .configure_linker(|linker| {
      host_bindings::Plugin::add_to_linker::<_, HasSelf<_>>(linker, |state| {
        state
      })
    })
    .build()
    .await
    .unwrap();

  let error = instance
    .invoke(async |store, instance| {
      let bindings = host_bindings::Plugin::new(&mut *store, instance).unwrap();

      bindings.call_answer(store).await
    })
    .await
    .unwrap_err();

  assert!(matches!(
    error,
    Error::InvocationTimeout { timeout: actual } if actual == timeout
  ));

  assert!(cancelled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn invocation_timeout_interrupts_component() {
  let timeout = Duration::from_millis(50);

  let runtime = Runtime::builder()
    .fuel(u64::MAX)
    .timeout(timeout)
    .build()
    .unwrap();

  let plugin = fixture(&runtime, "fuel");

  let mut instance = plugin
    .instantiate()
    .timeout(Duration::from_secs(1))
    .build()
    .await
    .unwrap();

  let started = Instant::now();

  let error = instance
    .invoke(async |store, instance| {
      let function = instance.get_typed_func::<(), ()>(&mut *store, "run")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap_err();

  assert!(matches!(
    error,
    Error::InvocationTimeout { timeout: actual } if actual == timeout
  ));

  assert!(started.elapsed() < Duration::from_secs(2));

  assert!(matches!(
    instance
      .invoke(async |_, _| Ok::<(), wasmtime::Error>(()))
      .await
      .unwrap_err(),
    Error::InstanceUnavailable
  ));
}

#[tokio::test]
async fn instances_have_independent_permissions() {
  let runtime = Runtime::new().unwrap();
  let plugin = fixture(&runtime, "environment");

  let mut first = plugin
    .instantiate()
    .permissions(Permissions::builder().env("WEXEL_TEST", "first").build())
    .build()
    .await
    .unwrap();

  let mut second = plugin
    .instantiate()
    .permissions(Permissions::builder().env("WEXEL_TEST", "second").build())
    .build()
    .await
    .unwrap();

  let first_environment = first
    .invoke(async |store, instance| {
      let function = instance.get_typed_func::<(), (Vec<(String, String)>,)>(
        &mut *store,
        "environment",
      )?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  let second_environment = second
    .invoke(async |store, instance| {
      let function = instance.get_typed_func::<(), (Vec<(String, String)>,)>(
        &mut *store,
        "environment",
      )?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  assert_eq!(
    first_environment,
    (vec![("WEXEL_TEST".into(), "first".into())],)
  );

  assert_eq!(
    second_environment,
    (vec![("WEXEL_TEST".into(), "second".into())],)
  );
}

#[tokio::test]
async fn memory_growth_respects_limit() {
  Test::new("memory")
    .memory_size(64 * 1024)
    .expect("grow", (-1_i32,))
    .await;
}

#[tokio::test]
async fn per_instance_limits_can_tighten_runtime_ceiling() {
  let runtime = Runtime::builder()
    .memory_size(2 * 64 * 1024)
    .build()
    .unwrap();

  let plugin = fixture(&runtime, "memory");

  let mut tight = plugin
    .instantiate()
    .limits(RuntimeLimits::builder().memory_size(64 * 1024).build())
    .build()
    .await
    .unwrap();

  let mut loose = plugin
    .instantiate()
    .limits(RuntimeLimits::builder().memory_size(3 * 64 * 1024).build())
    .build()
    .await
    .unwrap();

  let tight_result = tight
    .invoke(async |store, instance| {
      let function =
        instance.get_typed_func::<(), (i32,)>(&mut *store, "grow")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  let loose_result = loose
    .invoke(async |store, instance| {
      let function =
        instance.get_typed_func::<(), (i32,)>(&mut *store, "grow")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  let loose_ceiling_result = loose
    .invoke(async |store, instance| {
      let function =
        instance.get_typed_func::<(), (i32,)>(&mut *store, "grow")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  assert_eq!(loose.limits().memory_size(), 2 * 64 * 1024);
  assert_eq!(tight_result, (-1,));
  assert_eq!(loose_result, (1,));
  assert_eq!(loose_ceiling_result, (-1,));
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
async fn trap_is_structured() {
  Test::new("trap")
    .expect_trap("run", Trap::UnreachableCodeReached)
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
