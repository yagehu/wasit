mod stateless;

pub use stateless::StatelessStrategy;

use crate::{
    spec::{Function, Spec},
    ValueMeta,
};

pub trait CallStrategy {
    fn select_function<'spec>(&mut self, spec: &'spec Spec)
        -> Result<&'spec Function, eyre::Error>;

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<ValueMeta>, eyre::Error>;
}
