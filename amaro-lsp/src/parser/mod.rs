pub mod parser;
pub mod semantics;
pub mod utils;

pub use parser::parse_file;
pub use semantics::check_semantics;
