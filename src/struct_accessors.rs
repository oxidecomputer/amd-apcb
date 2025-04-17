// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![macro_use]

use crate::types::Error;
use crate::types::Result;
use four_cc::FourCC;
use num_traits::{FromPrimitive, ToPrimitive};
use zerocopy::byteorder::LittleEndian;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, U16, U32, U64};

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
impl Getter<Result<i8>> for i8 {
    fn get1(self) -> Result<i8> {
        Ok(self)
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
#[derive(
    Default,
    Debug,
    PartialEq,
    FromBytes,
    Immutable,
    IntoBytes,
    KnownLayout,
    Clone,
    Copy,
)]
#[repr(transparent)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct BU8(pub(crate) u8);
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
impl Setter<i8> for i8 {
    fn set1(&mut self, value: i8) {
        *self = value
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

#[derive(
    Debug, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Copy,
)]
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
#[allow(dead_code)]
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

impl DummyErrorChecks for i8 {}

impl DummyErrorChecks for bool {}

/// This macro expects a struct as a parameter (attributes are fine) and then,
/// first, defines the exact same struct, and also a more user-friendly struct
/// (name starts with "Serde") that can be used for serde (note: if you want
/// to use it for that, you still have to impl Serialize and Deserialize for
/// the former manually--forwarding to the respective "Serde" struct--which
/// does have "Serialize" and "Deserialize" implemented).
/// Afterwards, it automatically impls getters (and maybe setters) for the
/// fields where there was "get" (and maybe "set") specified.
/// The getters and setters so generated are hardcoded as calling
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
/// This also defines a "builder" fn that will be used by the deserializers at
/// the very beginning. That fn just calls "default()" (out of
/// necessity--because we don't want to have lots of extra partial struct
/// definitions). This is different to what make_bitfield_serde does
/// (where it calls "new()") (on purpose).
///
/// Field syntax:
/// NAME [|| [SERDE_META]* SERDE_TYPE] : TYPE
///      [| pub get GETTER_RETURN_TYPE [: pub set SETTER_PARAMETER_TYPE]]
///
/// The brackets denote optional parts.
/// Defines field NAME as TYPE. TYPE usually has to be the lowest-level
/// machine type (so it's "Little-endian u32", or "3 bits", or similar).
/// SERDE_TYPE, if given, will be the type used for the serde field.
/// If SERDE_TYPE is not given, TYPE will be used instead.
/// The "pub get" will use the given GETTER_RETURN_TYPE as the resulting type
/// of the generated getter, using Getter converters to get there as needed.
/// The "pub set" will use the given SETTER_PARAMETER_TYPE as the
/// parameter type of the generated setter, using Setter converters to get
/// there as needed.
macro_rules! make_accessors {(
    $(#[$struct_meta:meta])*
    $struct_vis:vis
    struct $StructName:ident {
        $(
            $(#[$field_meta:meta])*
            $field_vis:vis
            $field_name:ident
            $(|| $(#[$serde_field_orig_meta:meta])* $serde_ty:ty : $field_orig_ty:ty)?
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
        #[allow(dead_code)]
        pub fn builder() -> Self {
            Self::default()
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
                      fn [<with_ $field_name>](self: &mut Self, value: $field_setter_user_ty) -> &mut Self {
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
                pub(crate) fn [<serde_with_ $field_name>](self: &mut Self, value: $serde_ty) -> &mut Self {
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
                pub(crate) fn [<serde_with_ $field_name>](self: &mut Self, value: $field_ty) -> &mut Self {
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
        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
        pub(crate) struct [<Serde $StructName>] {
            $(
                $(pub $field_name: $field_ty,)?
                $($(#[$serde_field_orig_meta])* pub $field_name: $serde_ty,)?
            )*
        }
    }
)}

pub(crate) use make_accessors;
