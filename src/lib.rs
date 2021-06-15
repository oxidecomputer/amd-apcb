#![feature(min_const_generics)]
#![cfg_attr(not(feature = "std"), no_std)]

mod types;
mod apcb;
mod entry;
mod group;
mod ondisk;
mod tests;
pub use apcb::APCB;
pub use types::Result;
pub use types::Error;
