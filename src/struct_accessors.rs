#![macro_use]

use byteorder::LittleEndian;
use num_traits::{FromPrimitive, ToPrimitive};
use zerocopy::{FromBytes, AsBytes, U16, U32, U64};
use crate::types::Error;
use crate::types::Result;

pub(crate) trait Getter<T> {
    fn get1(self) -> T;
}
impl Getter<u64> for U64<LittleEndian> {
    fn get1(self) -> u64 {
        self.get()
    }
}
impl Getter<u32> for U32<LittleEndian> {
    fn get1(self) -> u32 {
        self.get()
    }
}
impl Getter<u16> for U16<LittleEndian> {
    fn get1(self) -> u16 {
        self.get()
    }
}
impl Getter<u8> for u8 {
    fn get1(self) -> u8 {
        self
    }
}
impl<'a> Getter<&'a [u8]> for &'a [u8] {
    fn get1(self) -> &'a [u8] {
        self
    }
}
impl<T: FromPrimitive> Getter<Result<T>> for u8 {
    fn get1(self) -> Result<T> {
        T::from_u8(self).ok_or_else(|| Error::EntryTypeMismatch)
    }
}
impl<T: FromPrimitive> Getter<Result<T>> for U32<LittleEndian> {
    fn get1(self) -> Result<T> {
        T::from_u32(self.get()).ok_or_else(|| Error::EntryTypeMismatch)
    }
}
#[derive(Debug, PartialEq, FromBytes, AsBytes, Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct BU8(pub(crate) u8);
impl Getter<Result<bool>> for BU8 {
    fn get1(self) -> Result<bool> {
        match self.0 {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::EntryTypeMismatch),
        }
    }
}

pub(crate) trait Setter<T> {
    fn set1(&mut self, value: T);
}
impl<T: ToPrimitive> Setter<T> for U64<LittleEndian> {
    fn set1(&mut self, value: T) {
        self.set(value.to_u64().unwrap())
    }
}
impl<T: ToPrimitive> Setter<T> for U32<LittleEndian> {
    fn set1(&mut self, value: T) {
        self.set(value.to_u32().unwrap())
    }
}
impl<T: ToPrimitive> Setter<T> for U16<LittleEndian> { // maybe never happens
    fn set1(&mut self, value: T) {
        self.set(value.to_u16().unwrap())
    }
}
impl<T: ToPrimitive> Setter<T> for u8 {
    fn set1(&mut self, value: T) {
        *self = value.to_u8().unwrap()
    }
}
impl Setter<bool> for BU8 {
    fn set1(&mut self, value: bool) {
        *self = BU8(match value {
            false => 0,
            true => 1,
        })
    }
}

macro_rules! make_accessors {(
    $(#[$struct_meta:meta])*
    $struct_vis:vis
    struct $StructName:ident {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident : $field_ty:ty $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
    }
) => (
    $(#[$struct_meta])*
    $struct_vis
    struct $StructName {
        $(
            $(#[$field_meta])*
            $field_vis
            $field_name: $field_ty,
        )*
    }

    impl $StructName {
        $($(
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> $field_user_ty
            {
                self.$field_name.get1()
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty)
                  {
                      self.$field_name.set1(value)
                  }
              }
            )?
        )?)*
    }
)}


pub(crate) use make_accessors;
