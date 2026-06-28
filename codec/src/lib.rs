pub mod decode;
pub mod encode;
mod error;
mod value;

pub use decode::decode;
pub use encode::encode;
pub use error::CodecError;
pub use value::Value;
