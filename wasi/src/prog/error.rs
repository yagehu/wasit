#[derive(thiserror::Error, Debug)]
pub enum GrowError {
    #[error("failed to arbitrarily pick")]
    Arbitrary(#[from] arbitrary::Error),

    #[error("no `{name}` resource in context")]
    NoResource { name: String },
}
