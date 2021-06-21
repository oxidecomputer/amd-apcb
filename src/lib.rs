#![feature(min_const_generics)]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(elided_lifetimes_in_paths)]

mod types;
mod apcb;
mod tokens_entry;
mod entry;
mod group;
mod ondisk;
mod tests;
pub use apcb::Apcb;
pub use types::Result;
pub use types::Error;
pub use entry::EntryItemBody;
