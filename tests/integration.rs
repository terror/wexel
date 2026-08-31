use {
  std::{
    future::{Future, ready},
    pin::Pin,
    sync::{
      Arc,
      atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
  },
  wasmtime::{Trap, component::HasSelf},
  wexel::{
    Error, Permissions, Runtime, RuntimeLimits, WasiCtxView, WasiState,
    WasiStateView, WasiView,
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

#[tokio::test]
async fn environment_exposes_only_configured_values() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/environment.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin
    .instantiate()
    .permissions(
      Permissions::builder()
        .env("WEXEL_TEST", "configured")
        .build(),
    )
    .build()
    .await
    .unwrap();

  let environment = instance
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
    environment,
    (vec![("WEXEL_TEST".into(), "configured".into())],)
  );
}

#[tokio::test]
async fn fuel_exhaustion_interrupts_component() {
  let runtime = Runtime::builder().fuel(100_000).build().unwrap();

  let bytes = wat::parse_file("tests/fixtures/fuel.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let error = instance
    .invoke(async |store, instance| {
      let function = instance.get_typed_func::<(), ()>(&mut *store, "run")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap_err();

  assert!(matches!(error, Error::FuelExhausted { .. }));
}

#[tokio::test]
async fn instances_have_independent_permissions() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/environment.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

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
async fn instantiation_fuel_exhaustion_is_structured() {
  let runtime = Runtime::builder().fuel(100_000).build().unwrap();

  let bytes =
    wat::parse_file("tests/fixtures/instantiation-timeout.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let error = plugin.instantiate().build().await.err().unwrap();

  assert!(matches!(error, Error::FuelExhausted { .. }));
}

#[tokio::test]
async fn instantiation_timeout_interrupts_component() {
  let timeout = Duration::from_millis(50);

  let runtime = Runtime::builder()
    .fuel(u64::MAX)
    .timeout(timeout)
    .build()
    .unwrap();

  let bytes =
    wat::parse_file("tests/fixtures/instantiation-timeout.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

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

  let bytes = wat::parse_file("tests/fixtures/host.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

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

  let bytes = wat::parse_file("tests/fixtures/fuel.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

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
async fn invokes_typed_component() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/answer.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let answer = instance
    .invoke(async |store, instance| {
      let bindings = bindings::Plugin::new(&mut *store, instance).unwrap();

      bindings.call_answer(store).await
    })
    .await
    .unwrap();

  assert_eq!(answer, 42);
}

#[tokio::test]
async fn memory_count_rejects_component() {
  let runtime = Runtime::builder().memories(1).build().unwrap();

  let bytes = wat::parse_file("tests/fixtures/memory-count.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let error = plugin.instantiate().build().await.err().unwrap();

  assert!(matches!(
    error,
    Error::Instantiation { source }
      if source.to_string()
        == "resource limit exceeded: memory count too high at 2"
  ));
}

#[tokio::test]
async fn memory_growth_respects_limit() {
  let runtime = Runtime::builder().memory_size(64 * 1024).build().unwrap();

  let bytes = wat::parse_file("tests/fixtures/memory.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let result = instance
    .invoke(async |store, instance| {
      let function =
        instance.get_typed_func::<(), (i32,)>(&mut *store, "grow")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  assert_eq!(result, (-1,));
}

#[tokio::test]
async fn per_instance_limits_can_tighten_runtime_ceiling() {
  let runtime = Runtime::builder()
    .memory_size(2 * 64 * 1024)
    .build()
    .unwrap();

  let bytes = wat::parse_file("tests/fixtures/memory.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

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
async fn table_growth_respects_limit() {
  let runtime = Runtime::builder().table_elements(1).build().unwrap();

  let bytes = wat::parse_file("tests/fixtures/table.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let result = instance
    .invoke(async |store, instance| {
      let function =
        instance.get_typed_func::<(), (i32,)>(&mut *store, "grow")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap();

  assert_eq!(result, (-1,));
}

#[tokio::test]
async fn trap_is_structured() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/trap.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let error = instance
    .invoke(async |store, instance| {
      let function = instance.get_typed_func::<(), ()>(&mut *store, "run")?;

      function.call_async(store, ()).await
    })
    .await
    .unwrap_err();

  assert!(matches!(
    error,
    Error::Trap {
      trap: Trap::UnreachableCodeReached,
      ..
    }
  ));
}

#[tokio::test]
async fn typed_component_calls_host_function() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/host.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin
    .instantiate_with(|wasi| HostState { answer: 42, wasi })
    .configure_linker(|linker| {
      host_bindings::Plugin::add_to_linker::<_, HasSelf<_>>(linker, |state| {
        state
      })
    })
    .build()
    .await
    .unwrap();

  let answer = instance
    .invoke(async |store, instance| {
      let bindings = host_bindings::Plugin::new(&mut *store, instance).unwrap();

      bindings.call_answer(store).await
    })
    .await
    .unwrap();

  assert_eq!(answer, 42);
}

#[tokio::test]
async fn wasi_component_imports_are_linked() {
  let runtime = Runtime::new().unwrap();

  let bytes = wat::parse_file("tests/fixtures/wasi.wat").unwrap();

  let plugin = runtime.load_bytes(bytes).unwrap();

  let mut instance = plugin.instantiate().build().await.unwrap();

  let answer = instance
    .invoke(async |store, instance| {
      let bindings = bindings::Plugin::new(&mut *store, instance).unwrap();

      bindings.call_answer(store).await
    })
    .await
    .unwrap();

  assert_eq!(answer, 42);
}
