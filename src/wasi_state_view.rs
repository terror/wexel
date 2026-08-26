use super::*;

pub trait WasiStateView: WasiView {
  fn wasi_state(&mut self) -> &mut WasiState;
}
