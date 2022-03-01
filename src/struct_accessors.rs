#![macro_use]

use crate::types::Error;
use crate::types::Result;
use byteorder::LittleEndian;
use num_traits::{FromPrimitive, ToPrimitive};
use zerocopy::{AsBytes, FromBytes, U16, U32, U64};

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
impl Getter<Result<u16>> for U16<LittleEndian> {
    fn get1(self) -> Result<u16> {
        Ok(self.get())
    }
}
impl Getter<Result<u64>> for U64<LittleEndian> {
    fn get1(self) -> Result<u64> {
        Ok(self.get())
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
        T::from_u8(self).ok_or(Error::EntryTypeMismatch)
    }
}
impl<T: FromPrimitive> Getter<Result<T>> for U32<LittleEndian> {
    fn get1(self) -> Result<T> {
        T::from_u32(self.get()).ok_or(Error::EntryTypeMismatch)
    }
}
// For Token
impl<T: FromPrimitive> Getter<Result<T>> for u32 {
    fn get1(self) -> Result<T> {
        T::from_u32(self).ok_or(Error::EntryTypeMismatch)
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

impl From<bool> for BU8 {
    fn from(value: bool) -> Self {
        match value {
            true => Self(1),
            false => Self(0),
        }
    }
}

#[derive(Debug, PartialEq, FromBytes, AsBytes, Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct BLU16(pub(crate) U16<LittleEndian>);
impl Getter<Result<bool>> for BLU16 {
    fn get1(self) -> Result<bool> {
        match self.0.get() {
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
impl<T: ToPrimitive> Setter<T> for U16<LittleEndian> {
    // maybe never happens
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
impl Setter<bool> for BLU16 {
    fn set1(&mut self, value: bool) {
        *self = Self(match value {
            false => 0.into(),
            true => 1.into(),
        })
    }
}

/// This macro expects a struct as a parameter (attributes are fine) and then,
/// first, defines the exact same struct. Afterwards, it automatically impl
/// getters (and setters) for the fields where there was "get" (and "set")
/// specified.  The getters and setters so generated are hardcoded as calling
/// self.field.get1 and self.field.set1, respectively.  These are usually
/// provided by a Getter and Getter trait impl (for example the ones in the same
/// file this macro is in) Note: If you want to add a docstring, put it before
/// the struct inside the macro parameter at the usage site, not before the
/// macro call. Field syntax:   NAME: TYPE[: pub get TYPE [:pub set TYPE]]
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
        pub fn build(&self) -> Self {
            self.clone()
        }
        $($(
            #[inline]
            $getter_vis
            fn $field_name (self: &'_ Self)
                -> Result<$field_user_ty>
            {
                self.$field_name.get1()
            }
            $(
              paste! {
                  #[inline]
                  $setter_vis
                  fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) {
                      self.$field_name.set1(value)
                  }

                  #[inline]
                  #[must_use]
                  $setter_vis
                  fn [<with_ $field_name>]<'a>(self: &mut Self, value: $field_setter_user_ty) -> &mut Self {
                      let mut result = self;
                      result.$field_name.set1(value);
                      result
                  }
              }
            )?
        )?)*
    }
)}

pub(crate) use make_accessors;
