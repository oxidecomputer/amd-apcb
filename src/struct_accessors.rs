#![macro_use]

use byteorder::LittleEndian;
use num_traits::{FromPrimitive, ToPrimitive};
use zerocopy::{FromBytes, AsBytes, U16, U32, U64};

pub(crate) trait Getter<T> {
    fn get1(self, destination: &mut T);
}
impl Getter<u64> for U64<LittleEndian> {
    fn get1(self, destination: &mut u64) {
        *destination = self.get()
    }
}
impl Getter<u32> for U32<LittleEndian> {
    fn get1(self, destination: &mut u32) {
        *destination = self.get()
    }
}
impl Getter<u16> for U16<LittleEndian> {
    fn get1(self, destination: &mut u16) {
        *destination = self.get()
    }
}
impl Getter<u8> for u8 {
    fn get1(self, destination: &mut u8) {
        *destination = self
    }
}
impl<'a> Getter<&'a [u8]> for &'a [u8] {
    fn get1(self, destination: &mut &'a [u8]) {
        *destination = self
    }
}
impl<T: FromPrimitive> Getter<Option<T>> for u8 {
    fn get1(self, destination: &mut Option<T>) {
        *destination = T::from_u8(self)
    }
}
impl<T: FromPrimitive> Getter<Option<T>> for U32<LittleEndian> {
    fn get1(self, destination: &mut Option<T>) {
        *destination = T::from_u32(self.get())
    }
}
#[derive(Debug, PartialEq, FromBytes, AsBytes, Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct BU8(pub(crate) u8);
impl Getter<Option<bool>> for BU8 {
    fn get1(self, destination: &mut Option<bool>) {
        *destination = match self.0 {
            0 => Some(false),
            1 => Some(true),
            _ => None,
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
                let mut result: $field_user_ty = <$field_user_ty>::default();
                self.$field_name.get1(&mut result); //.into()
                result
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty)
                  {
                      self.$field_name.set1(value) // (value as u8).into())
                  }
              }
            )?
        )?)*
    }
)}


pub(crate) use make_accessors;
