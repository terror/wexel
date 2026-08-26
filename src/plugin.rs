use super::*;

#[derive(Clone, Debug)]
pub struct Plugin {
  component: Component,
}

impl Plugin {
  #[must_use]
  pub fn component(&self) -> &Component {
    &self.component
  }

  pub(crate) fn new(component: Component) -> Self {
    Self { component }
  }
}
