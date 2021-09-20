#![feature(min_const_generics)]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(elided_lifetimes_in_paths)]

#[cfg(test)]
#[macro_use]
extern crate memoffset;

mod types;
mod apcb;
mod tokens_entry;
mod entry;
mod group;
mod ondisk;
mod tests;
mod struct_accessors;
mod struct_variants_enum;
mod token_accessors;
pub use apcb::Apcb;
pub use apcb::ApcbIoOptions;
pub use types::Result;
pub use types::Error;
pub use types::FileSystemError;
pub use entry::EntryItemBody;
pub use ondisk::*;
