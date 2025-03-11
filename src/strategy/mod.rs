mod stateful;
mod stateless;

pub use stateful::StatefulStrategy;
pub use stateless::StatelessStrategy;

use crate::{
    resource::HighLevelValue,
    spec::{Function, Spec, WasiValue},
    Environment,
    ResourceIdx,
};

pub trait CallStrategy {
    fn select_function<'spec>(&mut self, spec: &'spec Spec, env: &Environment) -> Result<&'spec Function, eyre::Error>;

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &Environment,
    ) -> Result<Vec<HighLevelValue>, eyre::Error>;

    fn handle_results(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &mut Environment,
        params: Vec<HighLevelValue>,
        results: Vec<Option<ResourceIdx>>,
        result_values: Option<&[WasiValue]>,
    ) -> Result<(), eyre::Error>;
}
