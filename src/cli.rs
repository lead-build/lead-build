//! Command line arguments of the binaries. They live in the library so that
//! tools other than the binaries themselves, such as the man page generator
//! (`examples/mangen.rs`), can reach them.

pub mod pb;
pub mod pbfmt;
pub mod pbls;
