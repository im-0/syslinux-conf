#[macro_use] extern crate custom_derive;
#[macro_use] extern crate enum_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate nom;

mod parser;
mod reader;
mod types;

pub use reader::{LocalConfType, Reader, ReaderError};
pub use types::*;

// TODO: Tests.
// TODO: Documentation.
