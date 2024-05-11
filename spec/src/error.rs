use crate::package::Typeidx;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("duplicate name: {0}")]
    DuplicateName(String),

    #[error("invalid typeidx: {0:?}")]
    InvalidTypeidx(Typeidx),

    #[error("unexpected token: {token}")]
    UnexpectedToken { token: String },
}
