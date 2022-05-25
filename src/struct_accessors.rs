#![macro_use]

use crate::types::Error;
use crate::types::Result;
use byteorder::LittleEndian;
use four_cc::FourCC;
use num_traits::{FromPrimitive, ToPrimitive};
use zerocopy::{AsBytes, FromBytes, U16, U32, U64};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

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
impl<T: FromPrimitive> Getter<Result<T>> for U16<LittleEndian> {
    fn get1(self) -> Result<T> {
        T::from_u16(self.get()).ok_or(Error::EntryTypeMismatch)
    }
}
impl<T: FromPrimitive> Getter<Result<T>> for U32<LittleEndian> {
    fn get1(self) -> Result<T> {
        T::from_u32(self.get()).ok_or(Error::EntryTypeMismatch)
    }
}
impl<T: FromPrimitive> Getter<Result<T>> for U64<LittleEndian> {
    fn get1(self) -> Result<T> {
        T::from_u64(self.get()).ok_or(Error::EntryTypeMismatch)
    }
}
// For Token
impl<T: FromPrimitive> Getter<Result<T>> for u32 {
    fn get1(self) -> Result<T> {
        T::from_u32(self).ok_or(Error::EntryTypeMismatch)
    }
}
#[derive(Default, Debug, PartialEq, FromBytes, AsBytes, Clone, Copy)]
#[repr(C, packed)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

impl Getter<Result<FourCC>> for [u8; 4] {
    fn get1(self) -> Result<FourCC> {
        Ok(FourCC(self))
    }
}

impl Setter<FourCC> for [u8; 4] {
    fn set1(&mut self, value: FourCC) {
        *self = value.0
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

/// Since primitive types are not a Result, this just wraps them into a Result.
/// The reason this exists is because our macros don't know whether or not
/// the getters return Result--but they have to be able to call map_err
/// on it in case these DO return Result.
pub(crate) trait DummyErrorChecks: Sized {
    fn map_err<F, O>(self, _: O) -> core::result::Result<Self, F>
    where
        O: Fn(Self) -> F,
    {
        Ok(self)
    }
}

impl DummyErrorChecks for u32 {}

impl DummyErrorChecks for u16 {}

impl DummyErrorChecks for u8 {}

impl DummyErrorChecks for bool {}

/// This macro expects a struct as a parameter (attributes are fine) and then,
/// first, defines the exact same struct. Afterwards, it automatically impls
/// getters (and setters) for the fields where there was "get" (and "set")
/// specified.  The getters and setters so generated are hardcoded as calling
/// self.field.get1 and self.field.set1, respectively.  These are usually
/// provided by a Getter and Setter trait impl (for example the ones in the same
/// file this macro is in).
///
/// Note: If you want to add a docstring, put it before the struct inside the
/// macro parameter at the usage site, not before the macro call.
///
/// This call also defines serde_* and serde_with* getters and setters with an
/// optionally specified serde type that is used during serialization and
/// deserialization
///
/// Field syntax:
/// NAME [|| SERDE_TYPE : TYPE] [: TYPE] [| pub get TYPE [: pub set TYPE]]
macro_rules! make_accessors {(
    $(#[$struct_meta:meta])*
    $struct_vis:vis
    struct $StructName:ident {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident
            $(|| $serde_ty:ty : $field_orig_ty:ty)?
            $(: $field_ty:ty)?
            $(| $getter_vis:vis get $field_user_ty:ty
              $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
        ),* $(,)?
    }
) => (
    $(#[$struct_meta])*
    $struct_vis
    struct $StructName {
        $(
            $(#[$field_meta])*
            $field_vis
            $($field_name: $field_ty,)?
            $($field_name: $field_orig_ty,)?
        )*
    }

    impl $StructName {
        #[allow(dead_code)]
        pub fn build(&self) -> Self {
            self.clone()
        }
        $(
            $(
                #[inline]
                #[allow(dead_code)]
                $getter_vis
                fn $field_name (self: &'_ Self)
                    -> Result<$field_user_ty>
                {
                    self.$field_name.get1()
                }
                $(
                  paste! {
                      #[inline]
                      #[allow(dead_code)]
                      $setter_vis
                      fn [<set_ $field_name>] (self: &'_ mut Self, value: $field_setter_user_ty) {
                          self.$field_name.set1(value)
                      }
                      #[inline]
                      #[allow(dead_code)]
                      $setter_vis
                      fn [<with_ $field_name>]<'a>(self: &mut Self, value: $field_setter_user_ty) -> &mut Self {
                          let result = self;
                          result.$field_name.set1(value);
                          result
                      }
                  }
                )?
            )?
            $(
                paste! {
                #[inline]
                #[allow(dead_code)]
                pub(crate) fn [<serde_ $field_name>] (self: &'_ Self)
                    -> Result<$serde_ty>
                {
                    self.$field_name.get1()
                }
                #[inline]
                #[allow(dead_code)]
                pub(crate) fn [<serde_with_ $field_name>]<'a>(self: &mut Self, value: $serde_ty) -> &mut Self {
                    let result = self;
                    result.$field_name.set1(value);
                    result
                }}
            )?
            $(
                paste! {
                #[inline]
                #[allow(dead_code)]
                pub(crate) fn [<serde_ $field_name>] (self: &'_ Self)
                    -> Result<$field_ty>
                {
                    self.$field_name.get1()
                }
                #[inline]
                #[allow(dead_code)]
                pub(crate) fn [<serde_with_ $field_name>]<'a>(self: &mut Self, value: $field_ty) -> &mut Self {
                    let result = self;
                    result.$field_name = value.into();
                    result
                }}
            )?
        )*
    }
    // for serde
    #[cfg(feature = "serde")]
    paste::paste!{
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
        // Doing remoting automatically would make it impossible for the user to use another one.
        // Since the config format presumably needs to be
        // backward-compatible, that wouldn't be such a great idea.
        #[cfg_attr(feature = "serde", serde(rename = "" $StructName))]
        pub(crate) struct [<Serde $StructName>] {
            $(
                $(pub $field_name: $field_ty,)?
                $(pub $field_name: $serde_ty,)?
            )*
        }
    }
)}

pub(crate) use make_accessors;
