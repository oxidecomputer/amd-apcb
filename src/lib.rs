#![cfg_attr(not(feature = "std"), no_std)]
#![warn(elided_lifetimes_in_paths)]
#![allow(clippy::collapsible_if)]

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
pub use types::PriorityLevel;
pub use entry::EntryItemBody;
pub use ondisk::*;
