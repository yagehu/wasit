pub mod program;

use program::ConstraintSetError;
pub use program::Program;

use interval::IntervalSet;

use crate::program::ConstraintSet;

pub fn evaluate(prog: &Program) -> Result<ConstraintSet, ConstraintSetError> {
    let cset = prog.expr.eval();

    cset
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Constraint {
    intervals: IntervalSet<usize>,
}

#[cfg(test)]
mod tests {}
