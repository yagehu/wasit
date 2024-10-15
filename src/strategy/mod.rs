mod stateful;
mod stateless;

pub use stateful::StatefulStrategy;
pub use stateless::StatelessStrategy;

use crate::{
    spec::{Function, Spec, WasiValue},
    ResourceIdx,
};

pub trait CallStrategy {
    fn select_function<'spec>(&mut self, spec: &'spec Spec)
        -> Result<&'spec Function, eyre::Error>;

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<(WasiValue, Option<ResourceIdx>)>, eyre::Error>;

    fn handle_results(
        &mut self,
        spec: &Spec,
        function: &Function,
        params: Vec<Option<ResourceIdx>>,
        results: Vec<Option<ResourceIdx>>,
    ) -> Result<(), eyre::Error>;
}
