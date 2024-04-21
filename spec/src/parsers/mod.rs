pub mod wazzi_preview1;
pub mod wazzi_preview1_old;

use nom_locate::LocatedSpan;

pub type Span<'a> = LocatedSpan<&'a str>;
