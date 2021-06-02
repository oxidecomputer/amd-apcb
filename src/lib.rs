#![feature(min_const_generics)]
#![cfg_attr(not(feature = "std"), no_std)]

//#![no_std]
//#[cfg(feature="std")]
//extern crate std;

#[macro_use]
extern crate serde_derive;

mod ondisk;
mod image;
pub use image::APCB;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
