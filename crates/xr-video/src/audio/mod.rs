//! Audio decoding, synchronization and output, independent of transport.
pub mod decoder;
pub mod sink;
pub mod sync;
pub use decoder::*;
pub use sink::*;
pub use sync::*;
