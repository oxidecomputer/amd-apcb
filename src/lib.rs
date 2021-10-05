#![cfg_attr(not(feature = "std"), no_std)]
#![warn(elided_lifetimes_in_paths)]
#![allow(clippy::collapsible_if)]

#[cfg(test)]
#[macro_use]
extern crate memoffset;

mod apcb;
mod entry;
mod group;
mod ondisk;
mod struct_accessors;
mod struct_variants_enum;
mod tests;
mod token_accessors;
mod tokens_entry;
mod types;
pub use apcb::Apcb;
pub use apcb::ApcbIoOptions;
pub use entry::EntryItemBody;
pub use ondisk::*;
pub use types::Error;
pub use types::FileSystemError;
pub use types::PriorityLevel;
pub use types::Result;
