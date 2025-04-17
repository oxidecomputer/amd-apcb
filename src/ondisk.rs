// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! This file contains the Apcb on-disk format.  Please only change it in
//! coordination with the AMD PSP team.  Even then, you probably shouldn't.

//#![feature(trace_macros)] trace_macros!(true);
#![allow(non_snake_case)]
#![allow(clippy::new_without_default)]

pub use crate::naples::{ParameterTimePoint, ParameterTokenConfig};
use crate::struct_accessors::{Getter, Setter, make_accessors};
use crate::token_accessors::{Tokens, TokensMut, make_token_accessors};
use crate::types::Error;
use crate::types::PriorityLevel;
use crate::types::Result;
use core::clone::Clone;
use core::cmp::Ordering;
use core::convert::TryFrom;
use core::convert::TryInto;
use core::mem::{size_of, take};
use core::num::{NonZeroU8, NonZeroU16};
use four_cc::FourCC;
use modular_bitfield::prelude::*;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use paste::paste;
use zerocopy::byteorder::LittleEndian;
use zerocopy::{
    FromBytes, Immutable, IntoBytes, KnownLayout, Ref, U16, U32, U64, Unaligned,
};

#[cfg(feature = "serde")]
use byteorder::WriteBytesExt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde-hex")]
use serde_hex::{SerHex, StrictPfx};

/// Work around Rust issue# 51443, in case it ever will be phased out.
/// (zerocopy 0.5.0 has a as_bytes_mut with a Self-where--which is not supposed
/// to be used anymore)
pub trait ElementAsBytes {
    fn element_as_bytes(&self) -> &[u8];
}
pub trait SequenceElementAsBytes {
    /// Checks whether we are compatible with ENTRY_ID.  If so, return our
    /// zerocopy.as_bytes representation.  Otherwise, return None.
    fn checked_as_bytes(&self, entry_id: EntryId) -> Option<&[u8]>;
}

// Note: Implement this on the enum.
pub trait SequenceElementFromBytes<'a>: Sized {
    /// Parses one item from (*WORLD).
    /// WORLD is a reference to a mut slice ((*WORLD) is usually much bigger
    /// than we need).  (*WORLD)'s beginning will be moved to after the parsed
    /// item.
    fn checked_from_bytes(
        entry_id: EntryId,
        world: &mut &'a [u8],
    ) -> Result<Self>;
}

// Note: Implement this on the enum.
pub trait MutSequenceElementFromBytes<'a>: Sized {
    /// Parses one item from (*WORLD).
    /// WORLD is a reference to a mut slice ((*WORLD) is usually much bigger
    /// than we need).  (*WORLD)'s beginning will be moved to after the parsed
    /// item.
    fn checked_from_bytes(
        entry_id: EntryId,
        world: &mut &'a mut [u8],
    ) -> Result<Self>;
}

/// There are (very few) Struct Entries like this: Header S0 S1 S2 S3.
/// This trait is implemented by structs that are used as a header of a
/// sequence.  Then, the header structs specify (in their impl) what the struct
/// type of the sequence will be.
pub trait HeaderWithTail {
    #[cfg(feature = "serde")]
    type TailArrayItemType<'de>: IntoBytes
        + Immutable
        + KnownLayout
        + FromBytes
        + serde::de::Deserialize<'de>;
    #[cfg(not(feature = "serde"))]
    type TailArrayItemType<'de>: IntoBytes + FromBytes + Immutable + KnownLayout;
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. If the item
/// cannot be parsed, returns None and does not advance.
pub(crate) fn take_header_from_collection_mut<
    'a,
    T: Sized + FromBytes + IntoBytes + Immutable + KnownLayout,
>(
    buf: &mut &'a mut [u8],
) -> Option<&'a mut T> {
    let xbuf = take(&mut *buf);
    match Ref::<_, T>::from_prefix(xbuf).ok() {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(Ref::into_mut(item))
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. Assuming that
/// *BUF had been aligned to ALIGNMENT before the call, it also ensures that
/// *BUF is aligned to ALIGNMENT after the call. If the item cannot be parsed,
/// returns None and does not advance.
pub(crate) fn take_body_from_collection_mut<'a>(
    buf: &mut &'a mut [u8],
    size: usize,
    alignment: usize,
) -> Option<&'a mut [u8]> {
    let xbuf = take(&mut *buf);
    if xbuf.len() >= size {
        let (item, xbuf) = xbuf.split_at_mut(size);
        if size % alignment != 0 && xbuf.len() >= alignment - (size % alignment)
        {
            let (_, b) = xbuf.split_at_mut(alignment - (size % alignment));
            *buf = b;
        } else {
            *buf = xbuf;
        }
        Some(item)
    } else {
        None
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. If the item
/// cannot be parsed, returns None and does not advance.
pub(crate) fn take_header_from_collection<
    'a,
    T: Sized + FromBytes + IntoBytes + Immutable + KnownLayout,
>(
    buf: &mut &'a [u8],
) -> Option<&'a T> {
    let xbuf = take(&mut *buf);
    match Ref::<_, T>::from_prefix(xbuf).ok() {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(Ref::into_ref(item))
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. Assuming that
/// *BUF had been aligned to ALIGNMENT before the call, it also ensures that
/// *BUF is aligned to ALIGNMENT after the call. If the item cannot be parsed,
/// returns None and does not advance.
pub(crate) fn take_body_from_collection<'a>(
    buf: &mut &'a [u8],
    size: usize,
    alignment: usize,
) -> Option<&'a [u8]> {
    let xbuf = take(&mut *buf);
    if xbuf.len() >= size {
        let (item, xbuf) = xbuf.split_at(size);
        if size % alignment != 0 && xbuf.len() >= alignment - (size % alignment)
        {
            let (_, b) = xbuf.split_at(alignment - (size % alignment));
            *buf = b;
        } else {
            *buf = xbuf;
        }
        Some(item)
    } else {
        None
    }
}

type LU16 = U16<LittleEndian>;
type LU32 = U32<LittleEndian>;
type LU64 = U64<LittleEndian>;

#[allow(unused_macros)]
macro_rules! make_serde_hex {
    ($serde_ty:ident, $base_ty:ty, $format_string:literal $(, $lu_ty:ty)?) => {
        #[derive(Default, Copy, Clone, FromPrimitive, ToPrimitive)]
        pub struct $serde_ty(
            $base_ty,
        );
        #[cfg(feature = "schemars")]
        impl schemars::JsonSchema for $serde_ty {
            fn schema_name() -> String {
                <&str>::schema_name()
            }
            fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
               // TODO: Further limit which string literals are allowed here.
                <&str>::json_schema(generator)
            }
            fn is_referenceable() -> bool {
                false
            }
        }
        impl From<$base_ty> for $serde_ty {
            fn from(base: $base_ty) -> Self {
                Self(base)
            }
        }
        impl From<$serde_ty> for $base_ty {
            fn from(st: $serde_ty) -> Self {
                st.0
            }
        }
        $(
            impl From<$lu_ty> for $serde_ty {
                fn from(lu: $lu_ty) -> Self {
                    Self(lu.get())
                }
            }
            impl From<$serde_ty> for $lu_ty {
                fn from(st: $serde_ty) -> Self {
                    Self::new(st.0)
                }
            }
        )?
        #[cfg(feature = "std")]
        impl std::fmt::Display for $serde_ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, $format_string, self.0)
            }
        }
        #[cfg(feature = "serde")]
        impl serde::ser::Serialize for $serde_ty {
            fn serialize<S>(
                &self,
                serializer: S
            ) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                format!("{}", self).serialize(serializer)
            }
        }
        #[cfg(feature = "serde")]
        impl<'de> serde::de::Deserialize<'de> for $serde_ty {
            fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                let val: $base_ty = parse_int::parse::<$base_ty>(&s).map_err(
                    |e| serde::de::Error::custom(format!("{:?}", e)))?;
                Ok(Self(val))
            }
        }
    };
}

#[cfg(feature = "serde-hex")]
make_serde_hex!(SerdeHex8, u8, "{:#04x}");
#[cfg(feature = "serde-hex")]
make_serde_hex!(SerdeHex16, u16, "{:#06x}", LU16);
#[cfg(feature = "serde-hex")]
make_serde_hex!(SerdeHex32, u32, "{:#010x}", LU32);
#[cfg(feature = "serde-hex")]
make_serde_hex!(SerdeHex64, u64, "{:#018x}", LU64);

#[cfg(not(feature = "serde-hex"))]
type SerdeHex8 = u8;
#[cfg(not(feature = "serde-hex"))]
type SerdeHex16 = u16;
#[cfg(not(feature = "serde-hex"))]
type SerdeHex32 = u32;
#[cfg(not(feature = "serde-hex"))]
type SerdeHex64 = u64;

macro_rules! make_array_accessors {
    ($res_ty:ty, $array_ty:ty) => {
        impl<const SIZE: usize> Getter<Result<[$res_ty; SIZE]>>
            for [$array_ty; SIZE]
        {
            fn get1(self) -> Result<[$res_ty; SIZE]> {
                Ok(self.map(|v| <$res_ty>::from(v)))
            }
        }

        impl<const SIZE: usize> Setter<[$res_ty; SIZE]> for [$array_ty; SIZE] {
            fn set1(&mut self, value: [$res_ty; SIZE]) {
                *self = value.map(|v| <$array_ty>::from(v));
            }
        }
    };
}

make_array_accessors!(u8, u8);
#[cfg(feature = "serde-hex")]
make_array_accessors!(SerdeHex8, u8);
make_array_accessors!(SerdeHex16, LU16);
make_array_accessors!(SerdeHex32, LU32);
make_array_accessors!(SerdeHex64, LU64);

make_accessors! {
    #[derive(Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Debug, Clone)]
    #[repr(C, packed)]
    pub struct V2_HEADER {
        pub signature || FourCC : [u8; 4],
        // This is automatically recalculated after deserialization.
        pub header_size || #[serde(default)] SerdeHex16 : LU16, // == sizeof(V2_HEADER); but 128 for V3
        pub version || u16 : LU16,     // == 0x30
        // This is automatically recalculated after deserialization.
        pub apcb_size || #[serde(default)] SerdeHex32 : LU32 | pub get u32 : pub set u32,
        pub unique_apcb_instance || SerdeHex32 : LU32 | pub get u32 : pub set u32,
        // This is automatically recalculated after deserialization.
        pub checksum_byte || #[serde(default)] SerdeHex8 : u8,
        _reserved_1 || #[serde(default)] [SerdeHex8; 3] : [u8; 3],
        _reserved_2 || #[serde(default)] [SerdeHex32; 3] : [LU32; 3],
    }
}

impl Default for V2_HEADER {
    fn default() -> Self {
        Self {
            signature: *b"APCB",
            header_size: (size_of::<Self>() as u16).into(),
            version: 0x30u16.into(),
            apcb_size: (size_of::<Self>() as u32).into(),
            unique_apcb_instance: 0u32.into(), // probably invalid
            checksum_byte: 0,                  // probably invalid
            _reserved_1: [0, 0, 0],
            _reserved_2: [0u32.into(); 3],
        }
    }
}

#[allow(dead_code)]
fn serde_v3_header_ext_reserved_2() -> SerdeHex16 {
    V3_HEADER_EXT::default()._reserved_2.into()
}
#[allow(dead_code)]
fn serde_v3_header_ext_reserved_4() -> SerdeHex16 {
    V3_HEADER_EXT::default()._reserved_4.into()
}
#[allow(dead_code)]
fn serde_v3_header_ext_reserved_5() -> SerdeHex16 {
    V3_HEADER_EXT::default()._reserved_5.into()
}

// The funny reserved values here were such that older firmware (which
// expects a V2_HEADER and does not honor its header_size) can skip this
// unknown struct.
// That is done by making the beginning here look like valid GROUP_HEADER
// and ENTRY_HEADER.  The firmware reads [V2_HEADER, GROUP_HEADER,
// ENTRY_HEADER].
// The sizes have since diverged. Now it doesn't make much sense to have
// them any more, except for bug compatibility.
make_accessors! {
    #[derive(
        FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Clone, Copy,
    )]
    #[repr(C, packed)]
    pub struct V3_HEADER_EXT {
        pub signature || FourCC : [u8; 4],
        _reserved_1 || #[serde(default)] SerdeHex16 : LU16,
        // At this location in memory, old readers expect GROUP_HEADER::header_size instead.
        _reserved_2 || #[serde(default = "serde_v3_header_ext_reserved_2")] SerdeHex16 : LU16,
        // At this location in memory, old readers expect GROUP_HEADER::version instead.
        pub struct_version || u16 : LU16,
        // At this location in memory, old readers expect GROUP_HEADER::_reserved_ instead.
        pub data_version || u16 : LU16,
        // At this location in memory, old readers expect GROUP_HEADER::group_size instead.
        pub ext_header_size || SerdeHex32 : LU32 | pub get u32 : pub set u32,
        _reserved_3 || #[serde(default)] SerdeHex16 : LU16, // ENTRY_HEADER::group_id
        // At this location in memory, old readers expect ENTRY_HEADER::entry_id instead.
        _reserved_4 || #[serde(default = "serde_v3_header_ext_reserved_4")] SerdeHex16 : LU16,
        // _reserved_5 includes 0x10 for the entry header; that
        // gives 48 Byte payload for the entry--which is inconsistent with the
        // size of the fields following afterwards.
        // At this location in memory, old readers expect ENTRY_HEADER::entry_size instead.
        _reserved_5 || #[serde(default = "serde_v3_header_ext_reserved_5")] SerdeHex16 : LU16,
        // At this location in memory, old readers expect ENTRY_HEADER::instance_id instead.
        _reserved_6 || #[serde(default)] SerdeHex16 : LU16,
        // ENTRY_HEADER::{context_type, context_format, unit_size, prority_mask,
        // key_size, key_pos, board_instance_mask}.  Those are all of
        // ENTRY_HEADER's fields.
        _reserved_7 || #[serde(default)] [SerdeHex32; 2] : [LU32; 2],
        pub data_offset || SerdeHex16 : LU16, // 0x58
        // This is unused--but make it optional in case we do need to
        // calculate it in the future.
        pub header_checksum || #[serde(default)] SerdeHex8 : u8,
        _reserved_8 || #[serde(default)] SerdeHex8 : u8,
        _reserved_9 || #[serde(default)] [SerdeHex32; 3] : [LU32; 3],
        pub integrity_sign || #[serde(default)] [SerdeHex8; 32] : [u8; 32],
        _reserved_10 || #[serde(default)] [SerdeHex32; 3] : [LU32; 3],
        pub signature_ending || FourCC : [u8; 4],       // "BCPA"
    }
}

impl Default for V3_HEADER_EXT {
    fn default() -> Self {
        Self {
            signature: *b"ECB2",
            _reserved_1: 0u16.into(),
            _reserved_2: 0x10u16.into(),
            struct_version: 0x12u16.into(),
            data_version: 0x100u16.into(),
            ext_header_size: (size_of::<Self>() as u32).into(),
            _reserved_3: 0u16.into(),
            _reserved_4: 0xFFFFu16.into(),
            _reserved_5: 0x40u16.into(),
            _reserved_6: 0x00u16.into(),
            _reserved_7: [0u32.into(); 2],
            data_offset: 0x58u16.into(),
            header_checksum: 0, // invalid--but unused by AMD Rome
            _reserved_8: 0,
            _reserved_9: [0u32.into(); 3],
            integrity_sign: [0; 32], // invalid--but unused by AMD Rome
            _reserved_10: [0u32.into(); 3],
            signature_ending: *b"BCPA",
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GroupId {
    Psp, // usual signature: "PSPG"
    Ccx,
    Df,     // usual signature: "DFG "
    Memory, // usual signature: "MEMG"
    Gnb,
    Fch,
    Cbs,
    Oem,
    Token, // usual signature: "TOKN"
    Unknown(u16),
}

impl ToPrimitive for GroupId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            GroupId::Psp => 0x1701,
            GroupId::Ccx => 0x1702,
            GroupId::Df => 0x1703,
            GroupId::Memory => 0x1704,
            GroupId::Gnb => 0x1705,
            GroupId::Fch => 0x1706,
            GroupId::Cbs => 0x1707,
            GroupId::Oem => 0x1708,
            GroupId::Token => 0x3000,
            GroupId::Unknown(x) => (*x).into(),
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for GroupId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            match value {
                0x1701 => Some(GroupId::Psp),
                0x1702 => Some(GroupId::Ccx),
                0x1703 => Some(GroupId::Df),
                0x1704 => Some(GroupId::Memory),
                0x1705 => Some(GroupId::Gnb),
                0x1706 => Some(GroupId::Fch),
                0x1707 => Some(GroupId::Cbs),
                0x1708 => Some(GroupId::Oem),
                0x3000 => Some(GroupId::Token),
                x => Some(GroupId::Unknown(x as u16)),
                //_ => None,
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PspEntryId {
    BoardIdGettingMethod,
    DefaultParameters, // Naples
    Parameters,        // Naples

    Unknown(u16),
}

impl ToPrimitive for PspEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x01,
            Self::Parameters => 0x02,
            Self::BoardIdGettingMethod => 0x60,
            Self::Unknown(x) => (*x).into(),
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for PspEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            match value {
                0x01 => Some(Self::DefaultParameters),
                0x02 => Some(Self::Parameters),
                0x60 => Some(Self::BoardIdGettingMethod),
                x => Some(Self::Unknown(x as u16)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CcxEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples
    Unknown(u16),
}

impl ToPrimitive for CcxEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x03,
            Self::Parameters => 0x04,
            Self::Unknown(x) => (*x).into(),
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for CcxEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            match value {
                0x03 => Some(Self::DefaultParameters),
                0x04 => Some(Self::Parameters),
                x => Some(Self::Unknown(x as u16)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples
    SlinkConfig,
    XgmiTxEq,
    XgmiPhyOverride,
    Unknown(u16),
}

impl ToPrimitive for DfEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x05,
            Self::Parameters => 0x06,
            Self::SlinkConfig => 0xCC,
            Self::XgmiTxEq => 0xD0,
            Self::XgmiPhyOverride => 0xDD,
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for DfEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0x05 => Self::DefaultParameters,
                0x06 => Self::Parameters,
                0xCC => Self::SlinkConfig,
                0xD0 => Self::XgmiTxEq,
                0xDD => Self::XgmiPhyOverride,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemoryEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples

    SpdInfo,
    DimmInfoSmbus,
    DimmConfigInfoId,
    MemOverclockConfig,
    DdrDqPinMap,
    Ddr5CaPinMap,
    MemDfeSearch,

    PlatformSpecificOverride,

    PsUdimmDdr4OdtPat,
    PsUdimmDdr4CadBus,
    PsUdimmDdr4DataBus,
    PsUdimmDdr4MaxFreq,
    PsUdimmDdr4StretchFreq,

    PsRdimmDdr4OdtPat,
    PsRdimmDdr4CadBus,
    PsRdimmDdr4DataBus,
    PsRdimmDdr4MaxFreq,
    PsRdimmDdr4StretchFreq,
    PsRdimmDdr5Bus,
    PsRdimmDdr5MaxFreq,
    PsRdimmDdr5StretchFreq,
    PsRdimmDdr5MaxFreqC1,

    Ps3dsRdimmDdr4MaxFreq,
    Ps3dsRdimmDdr4StretchFreq,
    Ps3dsRdimmDdr4DataBus,
    Ps3dsRdimmDdr5MaxFreq,
    Ps3dsRdimmDdr5StretchFreq,

    ConsoleOutControl,
    EventControl,
    ErrorOutControl,
    ExtVoltageControl,

    PsLrdimmDdr4OdtPat,
    PsLrdimmDdr4CadBus,
    PsLrdimmDdr4DataBus,
    PsLrdimmDdr4MaxFreq,
    PsLrdimmDdr4StretchFreq,
    PsLrdimmDdr5MaxFreq,
    PsLrdimmDdr5StretchFreq,

    PsSodimmDdr4OdtPat,
    PsSodimmDdr4CadBus,
    PsSodimmDdr4DataBus,
    PsSodimmDdr4MaxFreq,
    PsSodimmDdr4StretchFreq,

    DdrPostPackageRepair,

    PsDramdownDdr4OdtPat,
    PsDramdownDdr4CadBus,
    PsDramdownDdr4DataBus,
    PsDramdownDdr4MaxFreq,
    PsDramdownDdr4StretchFreq,

    PlatformTuning,
    PmuBistVendorAlgorithm,
    Ddr5RawCardConfig,
    Ddr5TrainingOverride,

    Unknown(u16),
}

impl ToPrimitive for MemoryEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x07,
            Self::Parameters => 0x08,

            Self::SpdInfo => 0x30,
            Self::DimmInfoSmbus => 0x31,
            Self::DimmConfigInfoId => 0x32,
            Self::MemOverclockConfig => 0x33,
            Self::DdrDqPinMap => 0x35,
            Self::Ddr5CaPinMap => 0x36,
            Self::MemDfeSearch => 0x37,

            Self::PlatformSpecificOverride => 0x40,

            Self::PsUdimmDdr4OdtPat => 0x41,
            Self::PsUdimmDdr4CadBus => 0x42,
            Self::PsUdimmDdr4DataBus => 0x43,
            Self::PsUdimmDdr4MaxFreq => 0x44,
            Self::PsUdimmDdr4StretchFreq => 0x45,

            Self::PsRdimmDdr4OdtPat => 0x46,
            Self::PsRdimmDdr4CadBus => 0x47,
            Self::PsRdimmDdr4DataBus => 0x48,
            Self::PsRdimmDdr4MaxFreq => 0x49,
            Self::PsRdimmDdr4StretchFreq => 0x4A,
            Self::PsRdimmDdr5Bus => 0x89,
            Self::PsRdimmDdr5MaxFreq => 0x8E,
            Self::PsRdimmDdr5StretchFreq => 0x92,
            Self::PsRdimmDdr5MaxFreqC1 => 0xA3,

            Self::Ps3dsRdimmDdr4MaxFreq => 0x4B,
            Self::Ps3dsRdimmDdr4StretchFreq => 0x4C,
            Self::Ps3dsRdimmDdr4DataBus => 0x4D,
            Self::Ps3dsRdimmDdr5MaxFreq => 0x94,
            Self::Ps3dsRdimmDdr5StretchFreq => 0x95,

            Self::ConsoleOutControl => 0x50,
            Self::EventControl => 0x51,
            Self::ErrorOutControl => 0x52,
            Self::ExtVoltageControl => 0x53,

            Self::PsLrdimmDdr4OdtPat => 0x54,
            Self::PsLrdimmDdr4CadBus => 0x55,
            Self::PsLrdimmDdr4DataBus => 0x56,
            Self::PsLrdimmDdr4MaxFreq => 0x57,
            Self::PsLrdimmDdr4StretchFreq => 0x58,
            Self::PsLrdimmDdr5MaxFreq => 0x8F,
            Self::PsLrdimmDdr5StretchFreq => 0x93,

            Self::PsSodimmDdr4OdtPat => 0x59,
            Self::PsSodimmDdr4CadBus => 0x5A,
            Self::PsSodimmDdr4DataBus => 0x5B,
            Self::PsSodimmDdr4MaxFreq => 0x5C,
            Self::PsSodimmDdr4StretchFreq => 0x5D,

            Self::DdrPostPackageRepair => 0x5E,

            Self::PsDramdownDdr4OdtPat => 0x70,
            Self::PsDramdownDdr4CadBus => 0x71,
            Self::PsDramdownDdr4DataBus => 0x72,
            Self::PsDramdownDdr4MaxFreq => 0x73,
            Self::PsDramdownDdr4StretchFreq => 0x74,

            Self::PlatformTuning => 0x75,
            Self::PmuBistVendorAlgorithm => 0xA1,
            Self::Ddr5RawCardConfig => 0xA2,
            Self::Ddr5TrainingOverride => 0xA4,

            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for MemoryEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0x07 => Self::DefaultParameters,
                0x08 => Self::Parameters,

                0x30 => Self::SpdInfo,
                0x31 => Self::DimmInfoSmbus,
                0x32 => Self::DimmConfigInfoId,
                0x33 => Self::MemOverclockConfig,
                0x35 => Self::DdrDqPinMap,
                0x36 => Self::Ddr5CaPinMap,
                0x37 => Self::MemDfeSearch,

                0x40 => Self::PlatformSpecificOverride,

                0x41 => Self::PsUdimmDdr4OdtPat,
                0x42 => Self::PsUdimmDdr4CadBus,
                0x43 => Self::PsUdimmDdr4DataBus,
                0x44 => Self::PsUdimmDdr4MaxFreq,
                0x45 => Self::PsUdimmDdr4StretchFreq,

                0x46 => Self::PsRdimmDdr4OdtPat,
                0x47 => Self::PsRdimmDdr4CadBus,
                0x48 => Self::PsRdimmDdr4DataBus,
                0x49 => Self::PsRdimmDdr4MaxFreq,
                0x4A => Self::PsRdimmDdr4StretchFreq,

                0x4B => Self::Ps3dsRdimmDdr4MaxFreq,
                0x4C => Self::Ps3dsRdimmDdr4StretchFreq,
                0x4D => Self::Ps3dsRdimmDdr4DataBus,

                0x50 => Self::ConsoleOutControl,
                0x51 => Self::EventControl,
                0x52 => Self::ErrorOutControl,
                0x53 => Self::ExtVoltageControl,

                0x54 => Self::PsLrdimmDdr4OdtPat,
                0x55 => Self::PsLrdimmDdr4CadBus,
                0x56 => Self::PsLrdimmDdr4DataBus,
                0x57 => Self::PsLrdimmDdr4MaxFreq,
                0x58 => Self::PsLrdimmDdr4StretchFreq,

                0x59 => Self::PsSodimmDdr4OdtPat,
                0x5A => Self::PsSodimmDdr4CadBus,
                0x5B => Self::PsSodimmDdr4DataBus,
                0x5C => Self::PsSodimmDdr4MaxFreq,
                0x5D => Self::PsSodimmDdr4StretchFreq,

                0x5E => Self::DdrPostPackageRepair,

                0x70 => Self::PsDramdownDdr4OdtPat,
                0x71 => Self::PsDramdownDdr4CadBus,
                0x72 => Self::PsDramdownDdr4DataBus,
                0x73 => Self::PsDramdownDdr4MaxFreq,
                0x74 => Self::PsDramdownDdr4StretchFreq,

                0x75 => Self::PlatformTuning,

                0x89 => Self::PsRdimmDdr5Bus,
                0x8E => Self::PsRdimmDdr5MaxFreq,
                0x8F => Self::PsLrdimmDdr5MaxFreq,
                0x92 => Self::PsRdimmDdr5StretchFreq,
                0x93 => Self::PsLrdimmDdr5StretchFreq,
                0x94 => Self::Ps3dsRdimmDdr5MaxFreq,
                0x95 => Self::Ps3dsRdimmDdr5StretchFreq,
                0xA1 => Self::PmuBistVendorAlgorithm,
                0xA2 => Self::Ddr5RawCardConfig,
                0xA3 => Self::PsRdimmDdr5MaxFreqC1,
                0xA4 => Self::Ddr5TrainingOverride,

                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GnbEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples
    EarlyPcieConfig,   // Turin
    Unknown(u16),
}

impl ToPrimitive for GnbEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x09,
            Self::Parameters => 0x0A,
            Self::EarlyPcieConfig => 0x1003,
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for GnbEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0x09 => Self::DefaultParameters,
                0x0A => Self::Parameters,
                0x1003 => Self::EarlyPcieConfig,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FchEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples

    EspiInit,    // Genoa
    EspiSioInit, // Turin

    Unknown(u16),
}

impl ToPrimitive for FchEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x0B,
            Self::Parameters => 0x0C,
            Self::EspiInit => 0x2001,
            Self::EspiSioInit => 0x2005,
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for FchEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0x0B => Self::DefaultParameters,
                0x0C => Self::Parameters,
                0x2001 => Self::EspiInit,
                0x2005 => Self::EspiSioInit,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CbsEntryId {
    DefaultParameters, // Naples
    Parameters,        // Naples

    Unknown(u16),
}

impl ToPrimitive for CbsEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x0D,
            Self::Parameters => 0x0E,
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for CbsEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0x0D => Self::DefaultParameters,
                0x0E => Self::Parameters,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum OemEntryId {
    Unknown(u16),
}

impl ToPrimitive for OemEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for OemEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 { Some(Self::Unknown(value as u16)) } else { None }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

// This one is for unknown values.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RawEntryId {
    Unknown(u16),
}

impl ToPrimitive for RawEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for RawEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 { Some(Self::Unknown(value as u16)) } else { None }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TokenEntryId {
    Bool,
    Byte,
    Word,
    Dword,
    Unknown(u16),
}

#[cfg(feature = "serde")]
use std::fmt::{Formatter, Result as FResult};
#[cfg(feature = "serde")]
impl serde::de::Expected for TokenEntryId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FResult {
        write!(f, "{self:?}")
    }
}

impl ToPrimitive for TokenEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Bool => 0,
            Self::Byte => 1,
            Self::Word => 2,
            Self::Dword => 4,
            Self::Unknown(x) => (*x) as i64,
        })
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

impl FromPrimitive for TokenEntryId {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            Some(match value {
                0 => Self::Bool,
                1 => Self::Byte,
                2 => Self::Word,
                4 => Self::Dword,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}

// Note: Keep front part synced with GroupId for easier understanding.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EntryId {
    Psp(PspEntryId),
    Ccx(CcxEntryId),
    Df(DfEntryId),
    Memory(MemoryEntryId),
    Gnb(GnbEntryId),
    Fch(FchEntryId),
    Cbs(CbsEntryId),
    Oem(OemEntryId),
    Token(TokenEntryId),
    Unknown(u16, RawEntryId),
}

impl EntryId {
    pub fn group_id(&self) -> GroupId {
        match self {
            Self::Psp(_) => GroupId::Psp,
            Self::Ccx(_) => GroupId::Ccx,
            Self::Df(_) => GroupId::Df,
            Self::Memory(_) => GroupId::Memory,
            Self::Gnb(_) => GroupId::Gnb,
            Self::Fch(_) => GroupId::Fch,
            Self::Cbs(_) => GroupId::Cbs,
            Self::Oem(_) => GroupId::Oem,
            Self::Token(_) => GroupId::Token,
            Self::Unknown(x, _) => GroupId::Unknown(*x),
        }
    }
    pub fn type_id(&self) -> u16 {
        match self {
            Self::Psp(x) => x.to_u16().unwrap(),
            Self::Ccx(x) => x.to_u16().unwrap(),
            Self::Df(x) => x.to_u16().unwrap(),
            Self::Memory(x) => x.to_u16().unwrap(),
            Self::Gnb(x) => x.to_u16().unwrap(),
            Self::Fch(x) => x.to_u16().unwrap(),
            Self::Cbs(x) => x.to_u16().unwrap(),
            Self::Oem(x) => x.to_u16().unwrap(),
            Self::Token(x) => x.to_u16().unwrap(),
            Self::Unknown(_, x) => x.to_u16().unwrap(),
        }
    }
    pub fn decode(group_id: u16, type_id: u16) -> Self {
        match GroupId::from_u16(group_id).unwrap() {
            GroupId::Psp => Self::Psp(PspEntryId::from_u16(type_id).unwrap()),
            GroupId::Ccx => Self::Ccx(CcxEntryId::from_u16(type_id).unwrap()),
            GroupId::Df => Self::Df(DfEntryId::from_u16(type_id).unwrap()),
            GroupId::Memory => {
                Self::Memory(MemoryEntryId::from_u16(type_id).unwrap())
            }
            GroupId::Gnb => Self::Gnb(GnbEntryId::from_u16(type_id).unwrap()),
            GroupId::Fch => Self::Fch(FchEntryId::from_u16(type_id).unwrap()),
            GroupId::Cbs => Self::Cbs(CbsEntryId::from_u16(type_id).unwrap()),
            GroupId::Oem => Self::Oem(OemEntryId::from_u16(type_id).unwrap()),
            GroupId::Token => {
                Self::Token(TokenEntryId::from_u16(type_id).unwrap())
            }
            GroupId::Unknown(x) => {
                Self::Unknown(x, RawEntryId::from_u16(type_id).unwrap())
            }
        }
    }
}

make_accessors! {
    #[derive(
        FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Clone, Debug,
    )]
    #[repr(C, packed)]
    pub struct GROUP_HEADER {
        pub(crate) signature || FourCC : [u8; 4],
        pub(crate) group_id || SerdeHex16 : LU16,
        pub(crate) header_size || SerdeHex16 : LU16, // == sizeof(GROUP_HEADER)
        pub(crate) version || SerdeHex16 : LU16,     // == 0 << 4 | 1
        _reserved_ || #[serde(default)] SerdeHex16 : LU16,
        // This is automatically calculated on deserialization.
        pub(crate) group_size || #[serde(default)] SerdeHex32 : LU32, // including header!
    }
}

#[derive(FromPrimitive, ToPrimitive, Debug, PartialEq, Copy, Clone)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ContextFormat {
    Raw = 0,
    SortAscending = 1, // (sort by key)
}

#[derive(FromPrimitive, ToPrimitive, Debug, PartialEq, Copy, Clone)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ContextType {
    Struct = 0,
    Parameters = 1,
    Tokens = 2, // then, entry_id means something else
}

impl Default for GROUP_HEADER {
    fn default() -> Self {
        Self {
            signature: *b"    ",   // probably invalid
            group_id: 0u16.into(), // probably invalid
            header_size: (size_of::<Self>() as u16).into(),
            version: 0x01u16.into(),
            _reserved_: 0u16.into(),
            group_size: (size_of::<Self>() as u32).into(), // probably invalid
        }
    }
}

/// A variant of the make_accessors macro for modular_bitfields.
macro_rules! make_bitfield_serde {(
        $(#[$struct_meta:meta])*
        $struct_vis:vis
        struct $StructName:ident {
                $(
                        $(#[$field_meta:meta])*
                        $field_vis:vis
                        $field_name:ident
                        $(|| $(#[$serde_field_orig_meta:meta])* $serde_ty:ty : $field_orig_ty:ty)?
                        $(: $field_ty:ty)?
                        $(| $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
                ),* $(,)?
        }
) => {
    $(#[$struct_meta])*
    $struct_vis
    struct $StructName {
        $(
            $(#[$field_meta])*
            $field_vis
            $($field_name : $field_ty,)?
            $($field_name : $field_orig_ty,)?
        )*
    }

    impl $StructName {
        pub fn builder() -> Self {
            Self::new() // NOT default
        }
        pub fn build(&self) -> Self {
            self.clone()
        }
    }

    #[cfg(feature = "serde")]
    impl $StructName {
        $(
            paste!{
                $(
                    pub(crate) fn [<serde_ $field_name>] (self : &'_ Self)
                        -> Result<$field_ty> {
                        Ok(self.$field_name())
                    }
                )?
                $(
                    pub(crate) fn [<serde_ $field_name>] (self : &'_ Self)
                        -> Result<$serde_ty> {
                        Ok(self.$field_name().into())
                    }
                )?
                $(
                    pub(crate) fn [<serde_with_ $field_name>](self : &mut Self, value: $field_ty) -> &mut Self {
                        self.[<set_ $field_name>](value.into());
                        self
                    }
                )?
                $(
                    pub(crate) fn [<serde_with_ $field_name>](self : &mut Self, value: $serde_ty) -> &mut Self {
                        self.[<set_ $field_name>](value.into());
                        self
                    }
                )?
            }
        )*
    }

    #[cfg(feature = "serde")]
    paste::paste! {
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
        #[cfg_attr(feature = "serde", serde(rename = "" $StructName))]
        pub(crate) struct [<Serde $StructName>] {
            $(
                $(pub $field_name : <$field_ty as Specifier>::InOut,)?
                $($(#[$serde_field_orig_meta])* pub $field_name : $serde_ty,)?
            )*
        }
    }
}}

make_bitfield_serde! {
    #[bitfield(bits = 8)]
    #[repr(u8)]
    #[derive(Copy, Clone)]
    pub struct PriorityLevels {
        #[bits = 1]
        pub hard_force || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub high || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub medium || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub event_logging || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub low || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub normal || #[serde(default)] bool : bool | pub get bool : pub set bool,
        pub _reserved_1 || #[serde(default)] SerdeHex8 : B2,
    }
}

impl Default for PriorityLevels {
    fn default() -> Self {
        Self::new().with_normal(true)
    }
}

macro_rules! impl_bitfield_primitive_conversion {
    ($bitfield:ty, $valid_bits:expr, $bitfield_primitive_compatible_type:ty) => {
        impl $bitfield {
            const VALID_BITS: u64 = $valid_bits;
            pub const fn all_reserved_bits_are_unused(value: u64) -> bool {
                value & !Self::VALID_BITS == 0
            }
        }
        impl ToPrimitive for $bitfield {
            fn to_u64(&self) -> Option<u64> {
                Some(<$bitfield_primitive_compatible_type>::from(*self).into())
            }
            fn to_i64(&self) -> Option<i64> {
                Some(self.to_u64()? as i64)
            }
        }
        impl FromPrimitive for $bitfield {
            fn from_u64(value: u64) -> Option<Self> {
                if Self::all_reserved_bits_are_unused(value) {
                    let cut_value =
                        value as $bitfield_primitive_compatible_type;
                    if cut_value as u64 == value {
                        Some(Self::from(cut_value))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            fn from_i64(value: i64) -> Option<Self> {
                if value >= 0 {
                    Some(Self::from_u64(value as u64)?)
                } else {
                    None
                }
            }
        }
    };
}

impl_bitfield_primitive_conversion!(PriorityLevels, 0b111111, u8);

impl PriorityLevels {
    pub fn from_level(level: PriorityLevel) -> Self {
        let mut result = Self::new();
        match level {
            PriorityLevel::HardForce => {
                result.set_hard_force(true);
            }
            PriorityLevel::High => {
                result.set_high(true);
            }
            PriorityLevel::Medium => {
                result.set_medium(true);
            }
            PriorityLevel::EventLogging => {
                result.set_event_logging(true);
            }
            PriorityLevel::Low => {
                result.set_low(true);
            }
            PriorityLevel::Normal => {
                result.set_normal(true);
            }
        }
        result
    }
}

make_bitfield_serde! {
    #[bitfield(bits = 16)]
    #[repr(u16)]
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub struct BoardInstances {
        #[bits = 1]
        pub instance_0 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_1 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_2 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_3 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_4 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_5 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_6 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_7 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_8 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_9 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_10 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_11 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_12 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_13 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_14 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub instance_15 || #[serde(default)] bool : bool | pub get bool : pub set bool,
    }
}
pub type BoardInstance = u8;

impl BoardInstances {
    pub fn all() -> Self {
        let mut result = Self::new();
        result.set_instance_0(true);
        result.set_instance_1(true);
        result.set_instance_2(true);
        result.set_instance_3(true);
        result.set_instance_4(true);
        result.set_instance_5(true);
        result.set_instance_6(true);
        result.set_instance_7(true);
        result.set_instance_8(true);
        result.set_instance_9(true);
        result.set_instance_10(true);
        result.set_instance_11(true);
        result.set_instance_12(true);
        result.set_instance_13(true);
        result.set_instance_14(true);
        result.set_instance_15(true);
        result
    }
    pub fn from_instance(instance: BoardInstance) -> Result<Self> {
        let mut result = Self::new();
        match instance {
            0 => result.set_instance_0(true),
            1 => result.set_instance_1(true),
            2 => result.set_instance_2(true),
            3 => result.set_instance_3(true),
            4 => result.set_instance_4(true),
            5 => result.set_instance_5(true),
            6 => result.set_instance_6(true),
            7 => result.set_instance_7(true),
            8 => result.set_instance_8(true),
            9 => result.set_instance_9(true),
            10 => result.set_instance_10(true),
            11 => result.set_instance_11(true),
            12 => result.set_instance_12(true),
            13 => result.set_instance_13(true),
            14 => result.set_instance_14(true),
            15 => result.set_instance_15(true),
            _ => {
                return Err(Error::EntryTypeMismatch);
            }
        }
        Ok(result)
    }
}

impl_bitfield_primitive_conversion!(BoardInstances, 0xffff, u16);

pub mod gnb {
    use super::{
        BitfieldSpecifier, EntryCompatible, EntryId, FromBytes, FromPrimitive,
        Getter, GnbEntryId, Immutable, IntoBytes, KnownLayout, Result, Setter,
        ToPrimitive, Unaligned, paste,
    };
    #[cfg(feature = "serde")]
    use super::{Deserialize, SerdeHex8, Serialize};
    use crate::struct_accessors::make_accessors;
    use modular_bitfield::prelude::*;

    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        BitfieldSpecifier,
    )]
    // FIXME #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[bits = 3]
    pub enum EarlyPcieLinkSpeed {
        Maximum = 0,
        Gen1 = 1,
        Gen2 = 2,
        Gen3 = 3,
        Gen4 = 4,
        Gen5 = 5,
        _Reserved6 = 6,
        _Reserved7 = 7,
    }
    //impl_bitfield_primitive_conversion!(LrdimmDdr4DimmRanks, 0b11, u32);

    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        BitfieldSpecifier,
    )]
    // FIXME #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[bits = 2]
    pub enum EarlyPcieResetPin {
        None = 0,
        Gpio26 = 1,
        Gpio266 = 2,
        _Reserved3 = 3,
    }
    //impl_bitfield_primitive_conversion!(LrdimmDdr4DimmRanks, 0b11, u32);

    impl Getter<Result<EarlyPcieLinkSpeed>> for EarlyPcieLinkSpeed {
        fn get1(self) -> Result<Self> {
            Ok(self)
        }
    }

    impl Setter<EarlyPcieLinkSpeed> for EarlyPcieLinkSpeed {
        fn set1(&mut self, value: Self) {
            *self = value;
        }
    }

    impl Getter<Result<EarlyPcieResetPin>> for EarlyPcieResetPin {
        fn get1(self) -> Result<Self> {
            Ok(self)
        }
    }

    impl Setter<EarlyPcieResetPin> for EarlyPcieResetPin {
        fn set1(&mut self, value: Self) {
            *self = value;
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 64)]
        #[repr(u64)]
        #[derive(
            Clone, Copy, Debug, PartialEq, //BitfieldSpecifier,
        )]
        pub struct EarlyPcieConfigBody {
            #[bits = 8]
            pub start_lane || u8 : B8, // (0xFF)
            #[bits = 8]
            pub end_lane || u8 : B8, // (0xFF=UNUSED_LANE if descriptor is unused)
            #[bits = 2]
            pub socket || u8 : B2,
            #[bits = 1]
            pub port_present || bool : bool | pub get bool : pub set bool,
            #[bits = 3]
            pub link_speed: EarlyPcieLinkSpeed | pub get EarlyPcieLinkSpeed : pub set EarlyPcieLinkSpeed,
            #[bits = 2]
            pub reset_pin: EarlyPcieResetPin | pub get EarlyPcieResetPin : pub set EarlyPcieResetPin,
            #[bits = 3]
            pub root_function || u8 : B3 | pub get u8 : pub set u8, // XXX (0: default)
            #[bits = 5]
            pub root_device || u8 : B5 | pub get u8 : pub set u8, // XXX (0: default)
            #[bits = 8]
            pub max_payload || u8 : B8 | pub get u8 : pub set u8, // XXX (0xff: ignore),
            #[bits = 8]
            pub tx_deemphasis || u8 : B8 | pub get u8 : pub set u8, // XXX (0xff: ignore)
            #[bits = 8]
            pub _reserved_1 || #[serde(default)] SerdeHex8 : B8, // (0)
            #[bits = 8]
            pub _reserved_2 || #[serde(default)] SerdeHex8 : B8, // (0)
        }
    }

    impl Default for EarlyPcieConfigBody {
        fn default() -> Self {
            Self::new()
                .with_start_lane(0xff)
                .with_end_lane(0xff)
                .with_socket(0)
                .with_port_present(true)
                .with_link_speed(EarlyPcieLinkSpeed::Gen3)
                .with_reset_pin(EarlyPcieResetPin::None)
                .with_root_function(0)
                .with_root_device(0)
                .with_max_payload(0xff)
                .with_tx_deemphasis(0xff)
        }
    }

    impl_bitfield_primitive_conversion!(
        EarlyPcieConfigBody,
        0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111,
        u64
    );

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct EarlyPcieConfigElement {
            body: [u8; 8], // no| pub get DdrPostPackageRepairBody : pub set DdrPostPackageRepairBody,
        }
    }

    #[cfg(feature = "serde")]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "serde", serde(rename = "EarlyPcieConfig"))]
    pub struct CustomSerdeEarlyPcieConfigElement {
        #[serde(flatten)]
        pub raw_body: EarlyPcieConfigBody,
    }

    #[cfg(feature = "serde")]
    impl EarlyPcieConfigElement {
        pub(crate) fn serde_raw_body(&self) -> Result<EarlyPcieConfigBody> {
            Ok(EarlyPcieConfigBody::from_bytes(self.body))
        }
        pub(crate) fn serde_with_raw_body(
            &mut self,
            value: EarlyPcieConfigBody,
        ) -> &mut Self {
            self.body = value.into_bytes();
            self
        }
    }
    impl EarlyPcieConfigElement {
        #[inline]
        pub fn body(&self) -> Option<EarlyPcieConfigBody> {
            Some(EarlyPcieConfigBody::from_bytes(self.body))
        }
        #[inline]
        pub fn set_body(&mut self, body: EarlyPcieConfigBody) {
            self.body = body.into_bytes();
        }
    }
    impl Default for EarlyPcieConfigElement {
        fn default() -> Self {
            Self { body: EarlyPcieConfigBody::default().into_bytes() }
        }
    }

    impl EntryCompatible for EarlyPcieConfigElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Gnb(GnbEntryId::EarlyPcieConfig))
        }
    }
}

make_accessors! {
    #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Clone, Debug)]
    #[repr(C, packed)]
    pub(crate) struct ENTRY_HEADER {
        pub(crate) group_id || SerdeHex16 : LU16, // should be equal to the group's group_id
        pub(crate) entry_id || SerdeHex16 : LU16, // meaning depends on context_type
        // The value of the field is automatically calculated on deserialization.
        pub(crate) entry_size || #[serde(default)] SerdeHex16 : LU16, // including header
        pub(crate) instance_id || SerdeHex16 : LU16 | pub get u16 : pub set u16,
        pub(crate) context_type || ContextType : u8 | pub get ContextType : pub set ContextType,  // see ContextType enum
        pub(crate) context_format || ContextFormat : u8 | pub get ContextFormat: pub set ContextFormat, // see ContextFormat enum
        pub(crate) unit_size || SerdeHex8 : u8 | pub get u8 : pub set u8, // in Byte.  Applicable when ContextType == 2.  value should be 8
        pub(crate) priority_mask || PriorityLevels : u8 | pub get PriorityLevels : pub set PriorityLevels,
        pub(crate) key_size || SerdeHex8 : u8 | pub get u8 : pub set u8, // Sorting key size; <= unit_size. Applicable when ContextFormat = 1. (or != 0)
        pub(crate) key_pos || SerdeHex8 : u8 | pub get u8 : pub set u8, // Sorting key position of the unit specified of UnitSize
        pub(crate) board_instance_mask || SerdeHex16 : LU16 | pub get u16 : pub set u16, // Board-specific Apcb instance mask
    }
}

impl Default for ENTRY_HEADER {
    fn default() -> Self {
        Self {
            group_id: 0u16.into(), // probably invalid
            entry_id: 0u16.into(), // probably invalid
            entry_size: (size_of::<Self>() as u16).into(), // probably invalid
            instance_id: 0u16.into(), // probably invalid
            context_type: 0,
            context_format: 0,
            unit_size: 0,
            priority_mask: 0x20, // maybe want to change that at runtime
            key_size: 0,
            key_pos: 0,
            board_instance_mask: 0xFFFFu16.into(),
        }
    }
}

pub const ENTRY_ALIGNMENT: usize = 4;

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Unaligned)]
#[repr(C, packed)]
pub struct TOKEN_ENTRY {
    pub key: LU32,
    pub value: LU32,
}

impl core::fmt::Debug for TOKEN_ENTRY {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fmt.debug_struct("TOKEN_ENTRY")
            .field("key", &self.key.get())
            .field("value", &self.value.get())
            .finish()
    }
}

/*
Apcb:
        Header V2
        V3 Header Ext
        [Group]

Group:
        Header
        Body
                [Type * alignment(4 Byte)]
Type:
    Header
    Body:
        If Header.context_format == token:
            [Token]
    Alignment

Token:
    id
    value
*/

pub trait EntryCompatible {
    /// Returns whether the ENTRY_ID can in principle house the impl of the
    /// trait EntryCompatible. Note: Usually, caller still needs to check
    /// whether the size is correct.  Since arrays are allowed and the ondisk
    /// structures then are array Element only, the payload size can be a
    /// natural multiple of the struct size.
    fn is_entry_compatible(_entry_id: EntryId, _prefix: &[u8]) -> bool {
        false
    }
    /// Returns (type, size).  Note that size is the entire stride necessary to
    /// skip one entire blurb.  Note that skip_step also needs to call
    /// is_entry_compatible automatically.
    fn skip_step(_entry_id: EntryId, _prefix: &[u8]) -> Option<(u16, usize)> {
        None
    }
}

// Starting here come the actual Entry formats (struct )

make_bitfield_serde! {
    /// For Naples.
    #[bitfield(bits = 32)]
    #[repr(u32)]
    #[derive(Copy, Clone, Debug)]
    pub struct ParameterAttributes {
        #[bits = 8]
        pub time_point: ParameterTimePoint | pub get ParameterTimePoint : pub set ParameterTimePoint,
        #[bits = 13]
        pub token: ParameterTokenConfig | pub get ParameterTokenConfig : pub set ParameterTokenConfig,
        #[bits = 3]
        pub size_minus_one || u8 : B3,
        #[bits = 8]
        pub _reserved_0 || #[serde(default)] u8 : B8,
    }
}

impl_bitfield_primitive_conversion!(
    ParameterAttributes,
    0b1111_1111_1111_1111_1111_1111,
    u32
);

impl ParameterAttributes {
    pub fn size(&self) -> u16 {
        (self.size_minus_one() as u16) + 1
    }
    pub fn terminator() -> Self {
        Self::new()
            .with_time_point(ParameterTimePoint::Never)
            .with_token(ParameterTokenConfig::Limit)
            .with_size_minus_one(0)
    }
}

/// For Naples.
pub struct ParametersIter<'a> {
    keys: &'a [u8],
    values: &'a [u8],
}

impl<'a> ParametersIter<'a> {
    pub(crate) fn next_attributes(
        buf: &mut &[u8],
    ) -> Result<ParameterAttributes> {
        match take_header_from_collection::<u32>(buf) {
            Some(attributes) => {
                let attributes = ParameterAttributes::from_u32(*attributes)
                    .ok_or(Error::ParameterRange)?;
                Ok(attributes)
            }
            None => Err(Error::ParameterRange),
        }
    }
    pub fn new(buf: &'a [u8]) -> Result<Self> {
        let beginning = buf;
        let mut buf = buf;
        loop {
            match Self::next_attributes(&mut buf) {
                Err(e) => return Err(e),
                Ok(attributes) => {
                    if attributes.token() == ParameterTokenConfig::Limit {
                        return Ok(Self {
                            keys: beginning, /* TODO: split before buf would
                                              * be enough. */
                            values: buf,
                        });
                    }
                }
            }
        }
    }
    fn read_u8(raw_value: &[u8]) -> Option<u8> {
        <[u8; 1]>::try_from(raw_value).ok().map(u8::from_le_bytes)
    }
    fn read_u16le(raw_value: &[u8]) -> Option<u16> {
        <[u8; 2]>::try_from(raw_value).ok().map(u16::from_le_bytes)
    }
    fn read_u32le(raw_value: &[u8]) -> Option<u32> {
        <[u8; 4]>::try_from(raw_value).ok().map(u32::from_le_bytes)
    }
    fn read_u64le(raw_value: &[u8]) -> Option<u64> {
        <[u8; 8]>::try_from(raw_value).ok().map(u64::from_le_bytes)
    }
}

impl Iterator for ParametersIter<'_> {
    type Item = Parameter;
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let attributes = Self::next_attributes(&mut self.keys).ok()?;
        if attributes.token() == ParameterTokenConfig::Limit {
            return None;
        }
        let size = usize::from(attributes.size());
        match size {
            1 | 2 | 4 | 8 => {
                let raw_value =
                    take_body_from_collection(&mut self.values, size, 1)?;
                match raw_value.len() {
                    1 => Some(
                        Parameter::new(
                            &attributes,
                            Self::read_u8(raw_value)?.into(),
                        )
                        .ok()?,
                    ),
                    2 => Some(
                        Parameter::new(
                            &attributes,
                            Self::read_u16le(raw_value)?.into(),
                        )
                        .ok()?,
                    ),
                    4 => Some(
                        Parameter::new(
                            &attributes,
                            Self::read_u32le(raw_value)?.into(),
                        )
                        .ok()?,
                    ),
                    8 => Some(
                        Parameter::new(
                            &attributes,
                            Self::read_u64le(raw_value)?,
                        )
                        .ok()?,
                    ),
                    _ => None, // TODO: Raise error
                }
            }
            _ => None,
        }
    }
}

/// For Naples.
#[derive(
    FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
)]
#[repr(C, packed)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Parameters {}

impl HeaderWithTail for Parameters {
    type TailArrayItemType<'de> = u8;
}

impl EntryCompatible for Parameters {
    fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
        matches!(
            entry_id,
            EntryId::Psp(PspEntryId::DefaultParameters)
                | EntryId::Psp(PspEntryId::Parameters)
                | EntryId::Ccx(CcxEntryId::DefaultParameters)
                | EntryId::Ccx(CcxEntryId::Parameters)
                | EntryId::Df(DfEntryId::DefaultParameters)
                | EntryId::Df(DfEntryId::Parameters)
                | EntryId::Memory(MemoryEntryId::DefaultParameters)
                | EntryId::Memory(MemoryEntryId::Parameters)
                | EntryId::Gnb(GnbEntryId::DefaultParameters)
                | EntryId::Gnb(GnbEntryId::Parameters)
                | EntryId::Fch(FchEntryId::DefaultParameters)
                | EntryId::Fch(FchEntryId::Parameters)
                | EntryId::Cbs(CbsEntryId::DefaultParameters)
                | EntryId::Cbs(CbsEntryId::Parameters)
        )
    }
}

make_accessors! {
    /// This is actually just a helper struct and is not on disk (at least not exactly).
    /// It's needed because the value area is not adjacent to the attribute.
    #[derive(Debug, Copy, Clone)]
    pub struct Parameter {
        time_point: ParameterTimePoint | pub get ParameterTimePoint : pub set ParameterTimePoint,
        token: ParameterTokenConfig | pub get ParameterTokenConfig : pub set ParameterTokenConfig,
        value_size || u16 : LU16 | pub get u16 : pub set u16,
        value || u64 : LU64 | pub get u64 : pub set u64,
        _reserved_0 || #[serde(default)] u8 : u8 | pub get u8 : pub set u8,
    }
}

impl Parameter {
    // Note: VALUE is in an extra area
    pub fn attributes(&self) -> Result<ParameterAttributes> {
        Ok(ParameterAttributes::new()
            .with_time_point(self.time_point)
            .with_token(self.token)
            .with_size_minus_one(
                self.value_size()?
                    .checked_sub(1)
                    .ok_or(Error::EntryTypeMismatch)?
                    .try_into()
                    .map_err(|_| Error::EntryTypeMismatch)?,
            )
            .with__reserved_0(self._reserved_0))
    }
    pub fn new(attributes: &ParameterAttributes, value: u64) -> Result<Self> {
        Ok(Self {
            time_point: attributes.time_point(),
            token: attributes.token(),
            value_size: attributes.size().into(),
            _reserved_0: attributes._reserved_0(),
            value: value.into(),
        })
    }
}

impl Default for Parameter {
    fn default() -> Self {
        Self::new(&ParameterAttributes::new(), 0).unwrap()
    }
}

impl Parameters {
    /// Create a new Parameters Tail with the items from SOURCE.
    /// Note that the last entry in SOURCE must be
    /// Parameter::new(&ParameterAttributes::terminator(), 0xff).
    #[cfg(feature = "serde")]
    pub(crate) fn new_tail_from_vec(source: Vec<Parameter>) -> Result<Vec<u8>> {
        //let iter = source.into_iter();
        //let total_size: usize = source.map(|x|
        // size_of::<ParameterAttributes>() + x.value_size).sum();
        let mut result = Vec::<u8>::new(); // with_capacity(total_size);
        for parameter in &source {
            let raw_attributes = u32::from(parameter.attributes()?);
            result
                .write_u32::<byteorder::LittleEndian>(raw_attributes)
                .map_err(|_| Error::ParameterRange)?;
        }
        for parameter in &source {
            let value = parameter.value()?;
            match parameter.value_size()? {
                1 => result
                    .write_u8(value as u8)
                    .map_err(|_| Error::ParameterRange)?,
                2 => result
                    .write_u16::<byteorder::LittleEndian>(value as u16)
                    .map_err(|_| Error::ParameterRange)?,
                4 => result
                    .write_u32::<byteorder::LittleEndian>(value as u32)
                    .map_err(|_| Error::ParameterRange)?,
                8 => result
                    .write_u64::<byteorder::LittleEndian>(value)
                    .map_err(|_| Error::ParameterRange)?,
                _ => Err(Error::EntryTypeMismatch)?,
            }
        }
        Ok(result)
    }
}

pub mod df {
    use super::*;
    use crate::struct_accessors::{Getter, Setter, make_accessors};
    use crate::types::Result;

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum SlinkRegionInterleavingSize {
        #[cfg_attr(feature = "serde", serde(rename = "256 B"))]
        _256B = 0,
        #[cfg_attr(feature = "serde", serde(rename = "512 B"))]
        _512B = 1,
        #[cfg_attr(feature = "serde", serde(rename = "1024 B"))]
        _1024B = 2,
        #[cfg_attr(feature = "serde", serde(rename = "2048 B"))]
        _2048B = 3,
        Auto = 7,
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct SlinkRegion {
            size || SerdeHex64 : LU64,
            alignment || SerdeHex8 : u8 | pub get u8 : pub set u8,
            socket: u8, // 0|1
            phys_nbio_map || SerdeHex8 : u8 | pub get u8 : pub set u8, // bitmap
            interleaving || SlinkRegionInterleavingSize : u8 | pub get SlinkRegionInterleavingSize : pub set SlinkRegionInterleavingSize,
            _reserved_ || #[serde(default)] [SerdeHex8; 4] : [u8; 4],
        }
    }
    impl Default for SlinkRegion {
        fn default() -> Self {
            Self {
                size: 0.into(),
                alignment: 0,
                socket: 0,
                phys_nbio_map: 0,
                interleaving: SlinkRegionInterleavingSize::_256B as u8,
                _reserved_: [0; 4],
            }
        }
    }

    impl SlinkRegion {
        // Not sure why AMD still uses those--but it does use them, so whatever.
        pub fn dummy(socket: u8) -> SlinkRegion {
            SlinkRegion { socket, ..Self::default() }
        }
    }

    // Rome only; even there, it's almost all 0s
    #[derive(
        FromBytes,
        IntoBytes,
        Immutable,
        KnownLayout,
        Unaligned,
        PartialEq,
        Debug,
        Copy,
        Clone,
    )]
    #[repr(C, packed)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub struct SlinkConfig {
        pub regions: [SlinkRegion; 4],
    }

    impl EntryCompatible for SlinkConfig {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Df(DfEntryId::SlinkConfig))
        }
    }

    impl HeaderWithTail for SlinkConfig {
        type TailArrayItemType<'de> = ();
    }

    impl SlinkConfig {
        pub fn new(regions: [SlinkRegion; 4]) -> Self {
            Self { regions }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use static_assertions::const_assert;

        #[test]
        fn test_df_structs() {
            const_assert!(size_of::<SlinkConfig>() == 16 * 4);
            const_assert!(size_of::<SlinkRegion>() == 16);
            assert!(offset_of!(SlinkRegion, alignment) == 8);
            assert!(offset_of!(SlinkRegion, socket) == 9);
            assert!(offset_of!(SlinkRegion, phys_nbio_map) == 10);
            assert!(offset_of!(SlinkRegion, interleaving) == 11);
        }

        #[test]
        fn test_df_struct_accessors() {
            let slink_config = SlinkConfig::new([
                SlinkRegion::dummy(0),
                SlinkRegion::dummy(0),
                SlinkRegion::dummy(1),
                SlinkRegion::dummy(1),
            ]);
            assert!(slink_config.regions[0].socket == 0);
            assert!(slink_config.regions[1].socket == 0);
            assert!(slink_config.regions[2].socket == 1);
            assert!(slink_config.regions[3].socket == 1);
        }
    }
}

pub mod memory {
    use super::*;
    use crate::struct_accessors::{
        BLU16, BU8, DummyErrorChecks, Getter, Setter, make_accessors,
    };
    use crate::types::Result;

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct DimmInfoSmbusElement {
            dimm_slot_present || bool : BU8 | pub get bool : pub set bool, // if false, it's soldered-down and not a slot
            socket_id || SerdeHex8 : u8 | pub get u8 : pub set u8,
            channel_id || SerdeHex8 : u8 | pub get u8 : pub set u8,
            dimm_id || SerdeHex8 : u8 | pub get u8 : pub set u8,
            // For soldered-down DIMMs, SPD data is hardcoded in APCB (in entry MemoryEntryId::SpdInfo), and DIMM_SMBUS_ADDRESS here is the index in the structure for SpdInfo.
            dimm_smbus_address || SerdeHex8 : u8,
            i2c_mux_address || SerdeHex8 : u8,
            mux_control_address || SerdeHex8 : u8,
            mux_channel || SerdeHex8 : u8,
        }
    }
    impl DimmInfoSmbusElement {
        pub fn i2c_mux_address(&self) -> Option<u8> {
            match self.i2c_mux_address {
                0xFF => None,
                x => Some(x),
            }
        }
        pub fn set_i2c_mux_address(&mut self, value: Option<u8>) -> Result<()> {
            self.i2c_mux_address = match value {
                None => 0xFF,
                Some(0xFF) => {
                    return Err(Error::EntryTypeMismatch);
                }
                Some(x) => x,
            };
            Ok(())
        }
        pub fn dimm_smbus_address(&self) -> Option<u8> {
            match self.dimm_slot_present {
                BU8(0) => None,
                _ => Some(self.dimm_smbus_address),
            }
        }
        pub fn set_dimm_smbus_address(&mut self, value: u8) -> Result<()> {
            match self.dimm_slot_present {
                BU8(0) => Err(Error::EntryTypeMismatch),
                _ => {
                    self.dimm_smbus_address = value;
                    Ok(())
                }
            }
        }
        pub fn dimm_spd_info_index(&self) -> Option<u8> {
            match self.dimm_slot_present {
                BU8(0) => Some(self.dimm_smbus_address),
                _ => None,
            }
        }
        pub fn set_dimm_spd_info_index(&mut self, value: u8) -> Result<()> {
            self.dimm_smbus_address = match self.dimm_slot_present {
                BU8(0) => value,
                _ => {
                    return Err(Error::EntryTypeMismatch);
                }
            };
            Ok(())
        }
        pub fn mux_control_address(&self) -> Option<u8> {
            match self.mux_control_address {
                0xFF => None,
                x => Some(x),
            }
        }
        pub fn set_mux_control_address(
            &mut self,
            value: Option<u8>,
        ) -> Result<()> {
            self.mux_control_address = match value {
                None => 0xFF,
                Some(0xFF) => {
                    return Err(Error::EntryTypeMismatch);
                }
                Some(x) => x,
            };
            Ok(())
        }
        pub fn mux_channel(&self) -> Option<u8> {
            match self.mux_channel {
                0xFF => None,
                x => Some(x),
            }
        }
        pub fn set_mux_channel(&mut self, value: Option<u8>) -> Result<()> {
            self.mux_channel = match value {
                None => 0xFF,
                Some(0xFF) => {
                    return Err(Error::EntryTypeMismatch);
                }
                Some(x) => x,
            };
            Ok(())
        }
    }

    impl DimmInfoSmbusElement {
        pub fn new_slot(
            socket_id: u8,
            channel_id: u8,
            dimm_id: u8,
            dimm_smbus_address: u8,
            i2c_mux_address: Option<u8>,
            mux_control_address: Option<u8>,
            mux_channel: Option<u8>,
        ) -> Result<Self> {
            let mut result = Self {
                dimm_slot_present: BU8(1),
                socket_id,
                channel_id,
                dimm_id,
                dimm_smbus_address,
                i2c_mux_address: 0xFF,
                mux_control_address: 0xFF,
                mux_channel: 0xFF,
            };
            result.set_i2c_mux_address(i2c_mux_address)?;
            result.set_mux_control_address(mux_control_address)?;
            result.set_mux_channel(mux_channel)?;
            Ok(result)
        }
        pub fn new_soldered_down(
            socket_id: u8,
            channel_id: u8,
            dimm_id: u8,
            dimm_spd_info_index: u8,
            i2c_mux_address: Option<u8>,
            mux_control_address: Option<u8>,
            mux_channel: Option<u8>,
        ) -> Result<Self> {
            let mut result = Self {
                dimm_slot_present: BU8(0),
                socket_id,
                channel_id,
                dimm_id,
                dimm_smbus_address: dimm_spd_info_index,
                i2c_mux_address: 0xFF,
                mux_control_address: 0xFF,
                mux_channel: 0xFF,
            };
            result.set_i2c_mux_address(i2c_mux_address)?;
            result.set_mux_control_address(mux_control_address)?;
            result.set_mux_channel(mux_channel)?;
            Ok(result)
        }
    }

    impl EntryCompatible for DimmInfoSmbusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::DimmInfoSmbus))
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct AblConsoleOutControl {
            enable_console_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_flow_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_setreg_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_getreg_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_status_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_sram_read_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_sram_write_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_test_verbose_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_basic_output_logging || bool : BU8 | pub get bool : pub set bool,
            _reserved_ || #[serde(default)] SerdeHex16 : LU16,
            abl_console_port || SerdeHex32 : LU32 | pub get u32 : pub set u32,
        }
    }
    impl Default for AblConsoleOutControl {
        fn default() -> Self {
            Self {
                enable_console_logging: BU8(1),
                enable_mem_flow_logging: BU8(1),
                enable_mem_setreg_logging: BU8(1),
                enable_mem_getreg_logging: BU8(0),
                enable_mem_status_logging: BU8(0),
                enable_mem_pmu_logging: BU8(0),
                enable_mem_pmu_sram_read_logging: BU8(0),
                enable_mem_pmu_sram_write_logging: BU8(0),
                enable_mem_test_verbose_logging: BU8(0),
                enable_mem_basic_output_logging: BU8(0),
                _reserved_: 0u16.into(),
                abl_console_port: 0x80u32.into(),
            }
        }
    }

    impl Getter<Result<AblConsoleOutControl>> for AblConsoleOutControl {
        fn get1(self) -> Result<AblConsoleOutControl> {
            Ok(self)
        }
    }

    impl Setter<AblConsoleOutControl> for AblConsoleOutControl {
        fn set1(&mut self, value: AblConsoleOutControl) {
            *self = value;
        }
    }

    impl AblConsoleOutControl {
        pub fn new() -> Self {
            Self::default()
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct AblBreakpointControl {
            enable_breakpoint || bool : BU8 | pub get bool : pub set bool,
            break_on_all_dies || bool : BU8 | pub get bool : pub set bool,
        }
    }
    impl Default for AblBreakpointControl {
        fn default() -> Self {
            Self { enable_breakpoint: BU8(1), break_on_all_dies: BU8(1) }
        }
    }

    impl Getter<Result<AblBreakpointControl>> for AblBreakpointControl {
        fn get1(self) -> Result<AblBreakpointControl> {
            Ok(self)
        }
    }

    impl Setter<AblBreakpointControl> for AblBreakpointControl {
        fn set1(&mut self, value: AblBreakpointControl) {
            *self = value;
        }
    }

    impl AblBreakpointControl {
        pub fn new(enable_breakpoint: bool, break_on_all_dies: bool) -> Self {
            let mut result = Self::default();
            result.set_enable_breakpoint(enable_breakpoint);
            result.set_break_on_all_dies(break_on_all_dies);
            result
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct ConsoleOutControl {
            pub abl_console_out_control: AblConsoleOutControl,
            pub abl_breakpoint_control: AblBreakpointControl,
            _reserved_ || #[serde(default)] SerdeHex16 : LU16,
        }
    }

    impl Default for ConsoleOutControl {
        fn default() -> Self {
            Self {
                abl_console_out_control: AblConsoleOutControl::default(),
                abl_breakpoint_control: AblBreakpointControl::default(),
                _reserved_: 0.into(),
            }
        }
    }

    impl EntryCompatible for ConsoleOutControl {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            /* On Naples, size_of::<NaplesConsoleOutControl>() == 20.
               On newer models, size_of::<ConsoleOutControl>() == 20.
               But the structs are not at all the same!
               Therefore, we use heuristics here:
                 On Naples, at offset 4, there's a LU32 port number.
                 On newer models, at offset 4, there's four bools.
                 It's very unlikely for the LSB of the port number to be
                 0 or 1.
            */
            if prefix.len() >= 20 && prefix[4] <= 1 {
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::ConsoleOutControl)
                )
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for ConsoleOutControl {
        type TailArrayItemType<'de> = ();
    }

    impl ConsoleOutControl {
        pub fn new(
            abl_console_out_control: AblConsoleOutControl,
            abl_breakpoint_control: AblBreakpointControl,
        ) -> Self {
            Self {
                abl_console_out_control,
                abl_breakpoint_control,
                _reserved_: 0.into(),
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct NaplesAblConsoleOutControl {
            enable_console_logging || bool : BU8 | pub get bool : pub set bool,
            _reserved_0 || #[serde(default)] [SerdeHex8; 3] : [u8; 3],
            abl_console_port || SerdeHex32 : U32<LittleEndian> | pub get u32 : pub set u32,
            enable_mem_flow_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_setreg_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_getreg_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_status_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_sram_read_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_pmu_sram_write_logging || bool : BU8 | pub get bool : pub set bool,
            enable_mem_test_verbose_logging || bool : BU8 | pub get bool : pub set bool,
        }
    }
    impl Default for NaplesAblConsoleOutControl {
        fn default() -> Self {
            Self {
                enable_console_logging: BU8(1),
                _reserved_0: [0u8; 3],
                enable_mem_flow_logging: BU8(1),
                enable_mem_setreg_logging: BU8(1),
                enable_mem_getreg_logging: BU8(0),
                enable_mem_status_logging: BU8(0),
                enable_mem_pmu_logging: BU8(0),
                enable_mem_pmu_sram_read_logging: BU8(0),
                enable_mem_pmu_sram_write_logging: BU8(0),
                enable_mem_test_verbose_logging: BU8(0),
                abl_console_port: 0x80u32.into(),
            }
        }
    }

    impl Getter<Result<NaplesAblConsoleOutControl>> for NaplesAblConsoleOutControl {
        fn get1(self) -> Result<Self> {
            Ok(self)
        }
    }

    impl Setter<NaplesAblConsoleOutControl> for NaplesAblConsoleOutControl {
        fn set1(&mut self, value: Self) {
            *self = value;
        }
    }

    impl NaplesAblConsoleOutControl {
        pub fn new() -> Self {
            Self::default()
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct NaplesConsoleOutControl {
            pub abl_console_out_control: NaplesAblConsoleOutControl,
            pub abl_breakpoint_control: AblBreakpointControl,
            _reserved_ || #[serde(default)] SerdeHex16 : U16<LittleEndian>,
        }
    }

    impl Default for NaplesConsoleOutControl {
        fn default() -> Self {
            Self {
                abl_console_out_control: NaplesAblConsoleOutControl::default(),
                abl_breakpoint_control: AblBreakpointControl::default(),
                _reserved_: 0.into(),
            }
        }
    }

    impl EntryCompatible for NaplesConsoleOutControl {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            /* On Naples, size_of::<NaplesConsoleOutControl>() == 20.
               On newer models, size_of::<ConsoleOutControl>() == 20.
               But the structs are not at all the same!
               Therefore, we use heuristics here:
                 On Naples, at offset 4, there's a LU32 port number.
                 On newer models, at offset 4, there's four bools.
                 It's very unlikely for the LSB of the port number to be
                 0 or 1.
            */
            if prefix.len() >= 20 && prefix[4] > 1 {
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::ConsoleOutControl)
                )
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for NaplesConsoleOutControl {
        type TailArrayItemType<'de> = ();
    }

    impl NaplesConsoleOutControl {
        pub fn new(
            abl_console_out_control: NaplesAblConsoleOutControl,
            abl_breakpoint_control: AblBreakpointControl,
        ) -> Self {
            Self {
                abl_console_out_control,
                abl_breakpoint_control,
                _reserved_: 0.into(),
            }
        }
    }

    #[derive(
        Debug, Default, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone,
    )]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum PortType {
        PcieHt0 = 0,
        PcieHt1 = 2,
        PcieMmio = 5,
        #[default]
        FchHtIo = 6,
        FchMmio = 7,
    }

    #[derive(
        Debug, Default, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone,
    )]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum PortSize {
        #[cfg_attr(feature = "serde", serde(rename = "8 Bit"))]
        _8Bit = 1,
        #[cfg_attr(feature = "serde", serde(rename = "16 Bit"))]
        _16Bit = 2,
        #[cfg_attr(feature = "serde", serde(rename = "32 Bit"))]
        #[default]
        _32Bit = 4,
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct ExtVoltageControl {
            enabled || bool : BU8 | pub get bool : pub set bool,
            _reserved_ || #[serde(default)] [SerdeHex8; 3] : [u8; 3],
            input_port || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            output_port || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            input_port_size || PortSize : LU32 | pub get PortSize : pub set PortSize,
            output_port_size || PortSize : LU32 | pub get PortSize : pub set PortSize,
            input_port_type || PortType : LU32 | pub get PortType : pub set PortType, // default: 6 (FCH)
            output_port_type || PortType : LU32 | pub get PortType : pub set PortType, // default: 6 (FCH)
            clear_acknowledgement || bool : BU8 | pub get bool : pub set bool,
            _reserved_2 || #[serde(default)] [SerdeHex8; 3] : [u8; 3],
        }
    }

    impl EntryCompatible for ExtVoltageControl {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::ExtVoltageControl)
            )
        }
    }

    impl HeaderWithTail for ExtVoltageControl {
        type TailArrayItemType<'de> = ();
    }

    impl Default for ExtVoltageControl {
        fn default() -> Self {
            Self {
                enabled: BU8(0),
                _reserved_: [0; 3],
                input_port: 0x84u32.into(),
                output_port: 0x80u32.into(),
                input_port_size: 4u32.into(),
                output_port_size: 4u32.into(),
                input_port_type: 6u32.into(),
                output_port_type: 6u32.into(),
                clear_acknowledgement: BU8(0),
                _reserved_2: [0; 3],
            }
        }
    }

    impl ExtVoltageControl {
        /// "input", "output": From the point of view of the PSP.
        pub fn new_enabled(
            input_port_type: PortType,
            input_port: u32,
            input_port_size: PortSize,
            output_port_type: PortType,
            output_port: u32,
            output_port_size: PortSize,
            clear_acknowledgement: bool,
        ) -> Self {
            let mut result = Self::default();
            result.set_enabled(true);
            result.set_input_port_type(input_port_type);
            result.set_input_port(input_port);
            result.set_input_port_size(input_port_size);
            result.set_output_port_type(output_port_type);
            result.set_output_port(output_port);
            result.set_output_port_size(output_port_size);
            result.set_clear_acknowledgement(clear_acknowledgement);
            result
        }
        pub fn new_disabled() -> Self {
            Self::default()
        }
    }

    make_bitfield_serde!(
        /// For example, single_rank=true && dual_rank=true means that this
        /// allows either single or dual rank.
        #[bitfield(bits = 4)]
        #[derive(
            Default, Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct Ddr4DimmRanks {
            #[bits = 1]
            pub unpopulated || #[serde(default)] bool : bool | pub get bool : pub set bool,
            #[bits = 1]
            pub single_rank || #[serde(default)] bool : bool | pub get bool : pub set bool,
            #[bits = 1]
            pub dual_rank || #[serde(default)] bool : bool | pub get bool : pub set bool,
            #[bits = 1]
            pub quad_rank || #[serde(default)] bool : bool | pub get bool : pub set bool,
        }
    );
    impl DummyErrorChecks for Ddr4DimmRanks {}

    impl From<Ddr4DimmRanks> for u32 {
        fn from(source: Ddr4DimmRanks) -> u32 {
            let bytes = source.into_bytes();
            bytes[0] as u32
        }
    }

    impl From<u32> for Ddr4DimmRanks {
        fn from(source: u32) -> Ddr4DimmRanks {
            assert!(source <= 0xFF);
            Ddr4DimmRanks::from_bytes([source as u8])
        }
    }

    impl_bitfield_primitive_conversion!(Ddr4DimmRanks, 0b1111, u32);

    make_bitfield_serde!(
        /// For example, unpopulated=true && lr=true means that this allows
        /// either unpopulated or lr.
        #[bitfield(bits = 4)]
        #[derive(
            Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct LrdimmDdr4DimmRanks {
            #[bits = 1]
            pub unpopulated || #[serde(default)] bool : bool | pub get bool : pub set bool,
            #[bits = 1]
            pub lr || #[serde(default)] bool : bool | pub get bool : pub set bool,
            #[bits = 2]
            pub _reserved_1 || #[serde(default)] SerdeHex8 : B2,
        }
    );

    impl DummyErrorChecks for LrdimmDdr4DimmRanks {}
    impl Default for LrdimmDdr4DimmRanks {
        fn default() -> Self {
            Self::new()
        }
    }

    impl From<LrdimmDdr4DimmRanks> for u32 {
        fn from(source: LrdimmDdr4DimmRanks) -> u32 {
            let bytes = source.into_bytes();
            bytes[0] as u32
        }
    }

    impl From<u32> for LrdimmDdr4DimmRanks {
        fn from(source: u32) -> LrdimmDdr4DimmRanks {
            assert!(source <= 0xFF);
            LrdimmDdr4DimmRanks::from_bytes([source as u8])
        }
    }

    impl_bitfield_primitive_conversion!(LrdimmDdr4DimmRanks, 0b11, u32);

    #[derive(Clone, Copy, PartialEq, FromPrimitive, ToPrimitive)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum CadBusClkDriveStrength {
        Auto = 0xFF,
        #[cfg_attr(feature = "serde", serde(rename = "120 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "120 Ohm"))]
        _120Ohm = 0,
        #[cfg_attr(feature = "serde", serde(rename = "60 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "60 Ohm"))]
        _60Ohm = 1,
        #[cfg_attr(feature = "serde", serde(rename = "40 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "40 Ohm"))]
        _40Ohm = 3,
        #[cfg_attr(feature = "serde", serde(rename = "30 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "30 Ohm"))]
        _30Ohm = 7,
        #[cfg_attr(feature = "serde", serde(rename = "24 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "24 Ohm"))]
        _24Ohm = 15,
        #[cfg_attr(feature = "serde", serde(rename = "20 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "20 Ohm"))]
        _20Ohm = 31,
    }

    pub type CadBusAddressCommandDriveStrength = CadBusClkDriveStrength;
    pub type CadBusCkeDriveStrength = CadBusClkDriveStrength;
    pub type CadBusCsOdtDriveStrength = CadBusClkDriveStrength;

    make_bitfield_serde!(
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy, PartialEq)]
        pub struct DdrRates {
            // Note: Bit index is (x/2)//66 of ddrx
            pub _reserved_1 || #[serde(default)] bool : bool,
            pub _reserved_2 || #[serde(default)] bool : bool,
            pub _reserved_3 || #[serde(default)] bool : bool,
            pub ddr400 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @3
            pub ddr533 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @4
            pub ddr667 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @5
            pub ddr800 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @6
            pub _reserved_4 || #[serde(default)] bool : bool,
            pub ddr1066 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @8
            pub _reserved_5 || #[serde(default)] bool : bool,
            pub ddr1333 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @10
            pub _reserved_6 || #[serde(default)] bool : bool,
            pub ddr1600 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @12
            pub _reserved_7 || #[serde(default)] bool : bool,
            pub ddr1866 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @14
            pub _reserved_8 || #[serde(default)] bool : bool, // AMD-missing pub ddr2100 // @15
            pub ddr2133 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @16
            pub _reserved_9 || #[serde(default)] bool : bool,
            pub ddr2400: bool | pub get bool : pub set bool, // @18
            pub _reserved_10 || #[serde(default)] bool : bool,
            pub ddr2667: bool | pub get bool : pub set bool, // @20
            pub _reserved_11 || #[serde(default)] bool : bool, // AMD-missing pub ddr2800 // @21
            pub ddr2933: bool | pub get bool : pub set bool, // @22
            pub _reserved_12 || #[serde(default)] bool : bool, // AMD-missing pub ddr3066: // @23
            pub ddr3200: bool | pub get bool : pub set bool, // @24
            pub _reserved_13 || #[serde(default)] bool : bool, // @25
            pub _reserved_14 || #[serde(default)] bool : bool, // @26
            pub _reserved_15 || #[serde(default)] bool : bool, // @27
            pub _reserved_16 || #[serde(default)] bool : bool, // @28
            pub _reserved_17 || #[serde(default)] bool : bool, // @29
            pub _reserved_18 || #[serde(default)] bool : bool, // @30
            pub _reserved_19 || #[serde(default)] bool : bool, /* @31
                                * AMD-missing pub ddr3334, set_ddr3334: 25,
                                * AMD-missing pub ddr3466, set_ddr3466: 26,
                                * AMD-missing pub ddr3600, set_ddr3600: 27,
                                * AMD-missing pub ddr3734, set_ddr3734: 28,
                                * AMD-missing pub ddr3866, set_ddr3866: 29,
                                * AMD-missing pub ddr4000, set_ddr4000: 30,
                                * AMD-missing pub ddr4200, set_ddr4200: 31,
                                * AMD-missing pub ddr4266, set_ddr4266: 32,
                                * AMD-missing pub ddr4334, set_ddr4334: 32,
                                * AMD-missing pub ddr4400, set_ddr4400: 33, */
        }
    );
    impl_bitfield_primitive_conversion!(
        DdrRates,
        0b0000_0001_0101_0101_0101_0101_0111_1000,
        u32
    );

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct DimmsPerChannelSelector {
            pub one_dimm || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub two_dimms || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub three_dimms || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub four_dimms || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B28,
        }
    }

    impl_bitfield_primitive_conversion!(DimmsPerChannelSelector, 0b1111, u32);

    #[derive(Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum DimmsPerChannel {
        NoSlot,   // 0xf0
        DontCare, // 0xff
        Specific(DimmsPerChannelSelector),
    }
    impl FromPrimitive for DimmsPerChannel {
        #[inline]
        fn from_u64(raw_value: u64) -> Option<Self> {
            if raw_value > 0xffff_ffff {
                None
            } else {
                let raw_value = raw_value as u32;
                if raw_value == 0xff {
                    Some(Self::DontCare)
                } else if raw_value == 0xf0 {
                    Some(Self::NoSlot)
                } else {
                    Some(Self::Specific(DimmsPerChannelSelector::from_u32(
                        raw_value,
                    )?))
                }
            }
        }
        #[inline]
        fn from_i64(raw_value: i64) -> Option<Self> {
            if raw_value >= 0 { Self::from_u64(raw_value as u64) } else { None }
        }
    }

    impl ToPrimitive for DimmsPerChannel {
        #[inline]
        fn to_i64(&self) -> Option<i64> {
            match self {
                Self::NoSlot => Some(0xf0),
                Self::DontCare => Some(0xff),
                Self::Specific(dimms) => {
                    let value = dimms.to_i64()?;
                    assert!(value != 0xf0 && value != 0xff);
                    Some(value)
                }
            }
        }
        #[inline]
        fn to_u64(&self) -> Option<u64> {
            Some(self.to_i64()? as u64)
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct RdimmDdr4Voltages {
            pub _1_2V || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B31,
            // all = 7
        }
    }
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "serde", serde(rename = "RdimmDdr4Voltages"))]
    pub struct CustomSerdeRdimmDdr4Voltages {
        #[cfg_attr(feature = "serde", serde(rename = "1.2 V"))]
        pub _1_2V: bool,
        #[cfg_attr(feature = "serde", serde(default))]
        pub _reserved_1: u32,
    }
    macro_rules! define_compat_bitfield_field {
        ($compat_field:ident, $current_field:ident) => {
            paste! {
                #[deprecated(note = "The name of this fn has since been fixed to '" $current_field "_or_err'")]
                pub fn [<$compat_field _or_err>](&self)
                    -> core::result::Result<bool,
                       modular_bitfield::error::InvalidBitPattern<<bool as modular_bitfield::Specifier>::Bytes>>
                {
                    self.[<$current_field _or_err>]()
                }
                #[deprecated(note = "The name of this fn has since been fixed to '" $current_field "'")]
                pub fn [<$compat_field>](&self) -> bool {
                    self.[<$current_field>]()
                }
                #[deprecated(note = "The name of this fn has since been fixed to 'with_" $current_field "'")]
                pub fn [<with_ $compat_field>](self, new_val: bool) -> Self {
                   self.[<with_ $current_field>](new_val)
                }
                #[deprecated(note = "The name of this fn has since been fixed to 'with_" $current_field "_checked'")]
                pub fn [<with_ $compat_field _checked>](self, new_val: bool)
                    -> core::result::Result<Self, modular_bitfield::error::OutOfBounds>
                {
                    self.[<with_ $current_field _checked>](new_val)
                }
                #[deprecated(note = "The name of this fn has since been fixed to 'set_" $current_field "_checked'")]
                pub fn [<set_ $compat_field _checked>](&mut self, new_val: bool)
                    -> core::result::Result<(), modular_bitfield::error::OutOfBounds>
                {
                    self.[<set_ $current_field _checked>](new_val)
                }
                #[deprecated(note = "The name of this fn has since been fixed to 'set_" $current_field "'")]
                pub fn [<set_ $compat_field>](&mut self, new_val: bool) {
                    self.[<set_ $current_field>](new_val)
                }
            }
        }
    }
    impl RdimmDdr4Voltages {
        define_compat_bitfield_field!(v_1_2, _1_2V);
    }
    impl_bitfield_primitive_conversion!(RdimmDdr4Voltages, 0b1, u32);
    impl Default for RdimmDdr4Voltages {
        fn default() -> Self {
            let mut r = Self::new();
            r.set__1_2V(true);
            r
        }
    }

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct RdimmDdr4CadBusElement {
            dimm_slots_per_channel || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            ddr_rates || DdrRates : LU32 | pub get DdrRates : pub set DdrRates,
            vdd_io || RdimmDdr4Voltages : LU32 | pub get RdimmDdr4Voltages : pub set RdimmDdr4Voltages,
            dimm0_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            gear_down_mode || bool : BLU16 | pub get bool : pub set bool,
            _reserved_ || #[serde(default)] SerdeHex16 : LU16,
            slow_mode || bool : BLU16 | pub get bool : pub set bool, // (probably) 2T is slow, 1T is fast
            _reserved_2 || #[serde(default)] SerdeHex16 : LU16,
            address_command_control || SerdeHex32 : LU32 | pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength || CadBusCkeDriveStrength : u8 | pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength || CadBusCsOdtDriveStrength : u8 | pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength || CadBusAddressCommandDriveStrength : u8 | pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength || CadBusClkDriveStrength : u8 | pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
        }
    }

    impl Default for RdimmDdr4CadBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rates: 0xa00.into(),
                vdd_io: 1.into(),
                dimm0_ranks: 6.into(), // maybe invalid
                dimm1_ranks: 1.into(), // maybe invalid

                gear_down_mode: BLU16(0.into()),
                _reserved_: 0.into(),
                slow_mode: BLU16(0.into()),
                _reserved_2: 0.into(),
                address_command_control: 0x272727.into(), // maybe invalid

                cke_drive_strength: 7,
                cs_odt_drive_strength: 7,
                address_command_drive_strength: 7,
                clk_drive_strength: 7,
            }
        }
    }

    impl RdimmDdr4CadBusElement {
        pub fn new(
            dimm_slots_per_channel: u32,
            ddr_rates: DdrRates,
            dimm0_ranks: Ddr4DimmRanks,
            dimm1_ranks: Ddr4DimmRanks,
            address_command_control: u32,
        ) -> Result<Self> {
            if address_command_control < 0x100_0000 {
                Ok(RdimmDdr4CadBusElement {
                    dimm_slots_per_channel: dimm_slots_per_channel.into(),
                    ddr_rates: ddr_rates
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    dimm0_ranks: dimm0_ranks
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    dimm1_ranks: dimm1_ranks
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    address_command_control: address_command_control.into(),
                    ..Self::default()
                })
            } else {
                Err(Error::EntryTypeMismatch)
            }
        }
    }

    impl EntryCompatible for RdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                // definitely not:
                // EntryId::Memory(MemoryEntryId::PsUdimmDdr4CadBus) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus) => true,
                // TODO: Check
                // EntryId::Memory(MemoryEntryId::PsSodimmDdr4CadBus) => true
                // Definitely not: PsDramdownDdr4CadBus.
                _ => false,
            }
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct UdimmDdr4Voltages {
            pub _1_5V || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub _1_35V || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub _1_25V || #[serde(default)] bool : bool | pub get bool : pub set bool,
            // all = 7
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B29,
        }
    }
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "serde", serde(rename = "UdimmDdr4Voltages"))]
    pub struct CustomSerdeUdimmDdr4Voltages {
        #[cfg_attr(feature = "serde", serde(rename = "1.5 V"))]
        pub _1_5V: bool,
        #[cfg_attr(feature = "serde", serde(rename = "1.35 V"))]
        pub _1_35V: bool,
        #[cfg_attr(feature = "serde", serde(rename = "1.25 V"))]
        pub _1_25V: bool,
        #[cfg_attr(feature = "serde", serde(default))]
        pub _reserved_1: u32,
    }
    impl UdimmDdr4Voltages {
        define_compat_bitfield_field!(v_1_5, _1_5V);
        define_compat_bitfield_field!(v_1_35, _1_35V);
        define_compat_bitfield_field!(v_1_25, _1_25V);
    }
    impl_bitfield_primitive_conversion!(UdimmDdr4Voltages, 0b111, u32);

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct UdimmDdr4CadBusElement {
            dimm_slots_per_channel || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            ddr_rates || DdrRates : LU32 | pub get DdrRates : pub set DdrRates,
            vdd_io || UdimmDdr4Voltages : LU32 | pub get UdimmDdr4Voltages : pub set UdimmDdr4Voltages,
            dimm0_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            gear_down_mode || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            _reserved_ || #[serde(default)] SerdeHex16 : LU16,
            slow_mode || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            _reserved_2 || #[serde(default)] SerdeHex16 : LU16,
            address_command_control || SerdeHex32 : LU32 | pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength || CadBusAddressCommandDriveStrength : u8 | pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength || CadBusCsOdtDriveStrength : u8 | pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength || CadBusAddressCommandDriveStrength : u8 | pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength || CadBusClkDriveStrength : u8 | pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
        }
    }

    impl Default for UdimmDdr4CadBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rates: 0xa00.into(),
                vdd_io: 1.into(),
                dimm0_ranks: 6.into(), // maybe invalid
                dimm1_ranks: 1.into(), // maybe invalid

                gear_down_mode: 0.into(),
                _reserved_: 0.into(),
                slow_mode: 0.into(),
                _reserved_2: 0.into(),
                address_command_control: 0x272727.into(), // maybe invalid

                cke_drive_strength: 7,
                cs_odt_drive_strength: 7,
                address_command_drive_strength: 7,
                clk_drive_strength: 7,
            }
        }
    }

    impl EntryCompatible for UdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4CadBus) => true,
                // definitely not:
                // EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus) => true,
                // TODO: Check
                // EntryId::Memory(MemoryEntryId::PsSodimmDdr4CadBus) => true
                // Definitely not: PsDramdownDdr4CadBus.
                _ => false,
            }
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct LrdimmDdr4Voltages {
            pub _1_2V || #[serde(default)] bool : bool | pub get bool : pub set bool,
            // all = 7
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B31,
        }
    }
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "serde", serde(rename = "LrdimmDdr4Voltages"))]
    pub struct CustomSerdeLrdimmDdr4Voltages {
        #[cfg_attr(feature = "serde", serde(rename = "1.2 V"))]
        pub _1_2V: bool,
        #[cfg_attr(feature = "serde", serde(default))]
        pub _reserved_1: u32,
    }

    impl LrdimmDdr4Voltages {
        define_compat_bitfield_field!(v_1_2, _1_2V);
    }
    impl_bitfield_primitive_conversion!(LrdimmDdr4Voltages, 0b1, u32);
    impl Default for LrdimmDdr4Voltages {
        fn default() -> Self {
            let mut lr = Self::new();
            lr.set__1_2V(true);
            lr
        }
    }

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4CadBusElement {
            dimm_slots_per_channel || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            ddr_rates || DdrRates : LU32 | pub get DdrRates : pub set DdrRates,
            vdd_io || LrdimmDdr4Voltages : LU32 | pub get LrdimmDdr4Voltages : pub set LrdimmDdr4Voltages,
            dimm0_ranks || LrdimmDdr4DimmRanks : LU32 | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks || LrdimmDdr4DimmRanks : LU32 | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,

            gear_down_mode || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            _reserved_ || #[serde(default)] SerdeHex16 : LU16,
            slow_mode || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            _reserved_2 || #[serde(default)] SerdeHex16 : LU16,
            address_command_control || SerdeHex32 : LU32 | pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength || CadBusCkeDriveStrength : u8 | pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength || CadBusCsOdtDriveStrength : u8 | pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength || CadBusAddressCommandDriveStrength : u8 | pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength || CadBusClkDriveStrength : u8 | pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
        }
    }

    impl Default for LrdimmDdr4CadBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rates: 0xa00.into(),
                vdd_io: 1.into(),
                dimm0_ranks: 2.into(), // maybe invalid
                dimm1_ranks: 1.into(), // maybe invalid

                gear_down_mode: 0.into(),
                _reserved_: 0.into(),
                slow_mode: 0.into(),
                _reserved_2: 0.into(),
                address_command_control: 0x373737.into(), // maybe invalid

                cke_drive_strength: 7,
                cs_odt_drive_strength: 7,
                address_command_drive_strength: 7,
                clk_drive_strength: 7,
            }
        }
    }

    impl LrdimmDdr4CadBusElement {
        pub fn new(
            dimm_slots_per_channel: u32,
            ddr_rates: DdrRates,
            dimm0_ranks: LrdimmDdr4DimmRanks,
            dimm1_ranks: LrdimmDdr4DimmRanks,
            address_command_control: u32,
        ) -> Result<Self> {
            if address_command_control < 0x100_0000 {
                Ok(LrdimmDdr4CadBusElement {
                    dimm_slots_per_channel: dimm_slots_per_channel.into(),
                    ddr_rates: ddr_rates
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    dimm0_ranks: dimm0_ranks
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    dimm1_ranks: dimm1_ranks
                        .to_u32()
                        .ok_or(Error::EntryTypeMismatch)?
                        .into(),
                    address_command_control: address_command_control.into(),
                    ..Self::default()
                })
            } else {
                Err(Error::EntryTypeMismatch)
            }
        }
    }

    impl EntryCompatible for LrdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4CadBus)
            )
        }
    }

    // Those are all divisors of 240
    // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 3
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum RttNom {
        Off = 0,
        #[cfg_attr(feature = "serde", serde(rename = "60 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "60 Ohm"))]
        _60Ohm = 1,
        #[cfg_attr(feature = "serde", serde(rename = "120 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "120 Ohm"))]
        _120Ohm = 2,
        #[cfg_attr(feature = "serde", serde(rename = "40 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "40 Ohm"))]
        _40Ohm = 3,
        #[cfg_attr(feature = "serde", serde(rename = "240 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "240 Ohm"))]
        _240Ohm = 4,
        #[cfg_attr(feature = "serde", serde(rename = "48 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "48 Ohm"))]
        _48Ohm = 5,
        #[cfg_attr(feature = "serde", serde(rename = "80 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "80 Ohm"))]
        _80Ohm = 6,
        #[cfg_attr(feature = "serde", serde(rename = "34 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "34 Ohm"))]
        _34Ohm = 7,
    }
    // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 11
    pub type RttPark = RttNom;
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum RttWr {
        Off = 0,
        #[cfg_attr(feature = "serde", serde(rename = "120 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "120 Ohm"))]
        _120Ohm = 1,
        #[cfg_attr(feature = "serde", serde(rename = "240 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "240 Ohm"))]
        _240Ohm = 2,
        Floating = 3,
        #[cfg_attr(feature = "serde", serde(rename = "80 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "80 Ohm"))]
        _80Ohm = 4,
    }

    #[derive(FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum VrefDqRange1 {
        #[cfg_attr(feature = "serde", serde(rename = "60.00%"))]
        _60_00P = 0b00_0000,
        #[cfg_attr(feature = "serde", serde(rename = "60.65%"))]
        _60_65P = 0b00_0001,
        #[cfg_attr(feature = "serde", serde(rename = "61.30%"))]
        _61_30P = 0b00_0010,
        #[cfg_attr(feature = "serde", serde(rename = "61.95%"))]
        _61_95P = 0b00_0011,
        #[cfg_attr(feature = "serde", serde(rename = "62.60%"))]
        _62_60P = 0b00_0100,
        #[cfg_attr(feature = "serde", serde(rename = "63.25%"))]
        _63_25P = 0b00_0101,
        #[cfg_attr(feature = "serde", serde(rename = "63.90%"))]
        _63_90P = 0b00_0110,
        #[cfg_attr(feature = "serde", serde(rename = "64.55%"))]
        _64_55P = 0b00_0111,
        #[cfg_attr(feature = "serde", serde(rename = "65.20%"))]
        _65_20P = 0b00_1000,
        #[cfg_attr(feature = "serde", serde(rename = "65.85%"))]
        _65_85P = 0b00_1001,
        #[cfg_attr(feature = "serde", serde(rename = "66.50%"))]
        _66_50P = 0b00_1010,
        #[cfg_attr(feature = "serde", serde(rename = "67.15%"))]
        _67_15P = 0b00_1011,
        #[cfg_attr(feature = "serde", serde(rename = "67.80%"))]
        _67_80P = 0b00_1100,
        #[cfg_attr(feature = "serde", serde(rename = "68.45%"))]
        _68_45P = 0b00_1101,
        #[cfg_attr(feature = "serde", serde(rename = "69.10%"))]
        _69_10P = 0b00_1110,
        #[cfg_attr(feature = "serde", serde(rename = "69.75%"))]
        _69_75P = 0b00_1111,
        #[cfg_attr(feature = "serde", serde(rename = "70.40%"))]
        _70_40P = 0b01_0000,
        #[cfg_attr(feature = "serde", serde(rename = "71.05%"))]
        _71_05P = 0b01_0001,
        #[cfg_attr(feature = "serde", serde(rename = "71.70%"))]
        _71_70P = 0b01_0010,
        #[cfg_attr(feature = "serde", serde(rename = "72.35%"))]
        _72_35P = 0b01_0011,
        #[cfg_attr(feature = "serde", serde(rename = "73.00%"))]
        _73_00P = 0b01_0100,
        #[cfg_attr(feature = "serde", serde(rename = "73.65%"))]
        _73_65P = 0b01_0101,
        #[cfg_attr(feature = "serde", serde(rename = "74.30%"))]
        _74_30P = 0b01_0110,
        #[cfg_attr(feature = "serde", serde(rename = "74.95%"))]
        _74_95P = 0b01_0111,
        #[cfg_attr(feature = "serde", serde(rename = "75.60%"))]
        _75_60P = 0b01_1000,
        #[cfg_attr(feature = "serde", serde(rename = "76.25%"))]
        _76_25P = 0b01_1001,
        #[cfg_attr(feature = "serde", serde(rename = "76.90%"))]
        _76_90P = 0b01_1010,
        #[cfg_attr(feature = "serde", serde(rename = "77.55%"))]
        _77_55P = 0b01_1011,
        #[cfg_attr(feature = "serde", serde(rename = "78.20%"))]
        _78_20P = 0b01_1100,
        #[cfg_attr(feature = "serde", serde(rename = "78.85%"))]
        _78_85P = 0b01_1101,
        #[cfg_attr(feature = "serde", serde(rename = "79.50%"))]
        _79_50P = 0b01_1110,
        #[cfg_attr(feature = "serde", serde(rename = "80.15%"))]
        _80_15P = 0b01_1111,
        #[cfg_attr(feature = "serde", serde(rename = "80.80%"))]
        _80_80P = 0b10_0000,
        #[cfg_attr(feature = "serde", serde(rename = "81.45%"))]
        _81_45P = 0b10_0001,
        #[cfg_attr(feature = "serde", serde(rename = "82.10%"))]
        _82_10P = 0b10_0010,
        #[cfg_attr(feature = "serde", serde(rename = "82.75%"))]
        _82_75P = 0b10_0011,
        #[cfg_attr(feature = "serde", serde(rename = "83.40%"))]
        _83_40P = 0b10_0100,
        #[cfg_attr(feature = "serde", serde(rename = "84.05%"))]
        _84_05P = 0b10_0101,
        #[cfg_attr(feature = "serde", serde(rename = "84.70%"))]
        _84_70P = 0b10_0110,
        #[cfg_attr(feature = "serde", serde(rename = "85.35%"))]
        _85_35P = 0b10_0111,
        #[cfg_attr(feature = "serde", serde(rename = "86.00%"))]
        _86_00P = 0b10_1000,
        #[cfg_attr(feature = "serde", serde(rename = "86.65%"))]
        _86_65P = 0b10_1001,
        #[cfg_attr(feature = "serde", serde(rename = "87.30%"))]
        _87_30P = 0b10_1010,
        #[cfg_attr(feature = "serde", serde(rename = "87.95%"))]
        _87_95P = 0b10_1011,
        #[cfg_attr(feature = "serde", serde(rename = "88.60%"))]
        _88_60P = 0b10_1100,
        #[cfg_attr(feature = "serde", serde(rename = "89.25%"))]
        _89_25P = 0b10_1101,
        #[cfg_attr(feature = "serde", serde(rename = "89.90%"))]
        _89_90P = 0b10_1110,
        #[cfg_attr(feature = "serde", serde(rename = "90.55%"))]
        _90_55P = 0b10_1111,
        #[cfg_attr(feature = "serde", serde(rename = "91.20%"))]
        _91_20P = 0b11_0000,
        #[cfg_attr(feature = "serde", serde(rename = "91.85%"))]
        _91_85P = 0b11_0001,
        #[cfg_attr(feature = "serde", serde(rename = "92.50%"))]
        _92_50P = 0b11_0010,
    }

    #[derive(FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum VrefDqRange2 {
        #[cfg_attr(feature = "serde", serde(rename = "45.00%"))]
        _45_00P = 0b00_0000,
        #[cfg_attr(feature = "serde", serde(rename = "45.65%"))]
        _45_65P = 0b00_0001,
        #[cfg_attr(feature = "serde", serde(rename = "46.30%"))]
        _46_30P = 0b00_0010,
        #[cfg_attr(feature = "serde", serde(rename = "46.95%"))]
        _46_95P = 0b00_0011,
        #[cfg_attr(feature = "serde", serde(rename = "47.60%"))]
        _47_60P = 0b00_0100,
        #[cfg_attr(feature = "serde", serde(rename = "48.25%"))]
        _48_25P = 0b00_0101,
        #[cfg_attr(feature = "serde", serde(rename = "48.90%"))]
        _48_90P = 0b00_0110,
        #[cfg_attr(feature = "serde", serde(rename = "49.55%"))]
        _49_55P = 0b00_0111,
        #[cfg_attr(feature = "serde", serde(rename = "50.20%"))]
        _50_20P = 0b00_1000,
        #[cfg_attr(feature = "serde", serde(rename = "50.85%"))]
        _50_85P = 0b00_1001,
        #[cfg_attr(feature = "serde", serde(rename = "51.50%"))]
        _51_50P = 0b00_1010,
        #[cfg_attr(feature = "serde", serde(rename = "52.15%"))]
        _52_15P = 0b00_1011,
        #[cfg_attr(feature = "serde", serde(rename = "52.80%"))]
        _52_80P = 0b00_1100,
        #[cfg_attr(feature = "serde", serde(rename = "53.45%"))]
        _53_45P = 0b00_1101,
        #[cfg_attr(feature = "serde", serde(rename = "54.10%"))]
        _54_10P = 0b00_1110,
        #[cfg_attr(feature = "serde", serde(rename = "54.75%"))]
        _54_75P = 0b00_1111,
        #[cfg_attr(feature = "serde", serde(rename = "55.40%"))]
        _55_40P = 0b01_0000,
        #[cfg_attr(feature = "serde", serde(rename = "56.05%"))]
        _56_05P = 0b01_0001,
        #[cfg_attr(feature = "serde", serde(rename = "56.70%"))]
        _56_70P = 0b01_0010,
        #[cfg_attr(feature = "serde", serde(rename = "57.35%"))]
        _57_35P = 0b01_0011,
        #[cfg_attr(feature = "serde", serde(rename = "58.00%"))]
        _58_00P = 0b01_0100,
        #[cfg_attr(feature = "serde", serde(rename = "58.65%"))]
        _58_65P = 0b01_0101,
        #[cfg_attr(feature = "serde", serde(rename = "59.30%"))]
        _59_30P = 0b01_0110,
        #[cfg_attr(feature = "serde", serde(rename = "59.95%"))]
        _59_95P = 0b01_0111,
        #[cfg_attr(feature = "serde", serde(rename = "60.60%"))]
        _60_60P = 0b01_1000,
        #[cfg_attr(feature = "serde", serde(rename = "61.25%"))]
        _61_25P = 0b01_1001,
        #[cfg_attr(feature = "serde", serde(rename = "61.90%"))]
        _61_90P = 0b01_1010,
        #[cfg_attr(feature = "serde", serde(rename = "62.55%"))]
        _62_55P = 0b01_1011,
        #[cfg_attr(feature = "serde", serde(rename = "63.20%"))]
        _63_20P = 0b01_1100,
        #[cfg_attr(feature = "serde", serde(rename = "63.85%"))]
        _63_85P = 0b01_1101,
        #[cfg_attr(feature = "serde", serde(rename = "64.50%"))]
        _64_50P = 0b01_1110,
        #[cfg_attr(feature = "serde", serde(rename = "65.15%"))]
        _65_15P = 0b01_1111,
        #[cfg_attr(feature = "serde", serde(rename = "65.80%"))]
        _65_80P = 0b10_0000,
        #[cfg_attr(feature = "serde", serde(rename = "66.45%"))]
        _66_45P = 0b10_0001,
        #[cfg_attr(feature = "serde", serde(rename = "67.10%"))]
        _67_10P = 0b10_0010,
        #[cfg_attr(feature = "serde", serde(rename = "67.75%"))]
        _67_75P = 0b10_0011,
        #[cfg_attr(feature = "serde", serde(rename = "68.40%"))]
        _68_40P = 0b10_0100,
        #[cfg_attr(feature = "serde", serde(rename = "69.05%"))]
        _69_05P = 0b10_0101,
        #[cfg_attr(feature = "serde", serde(rename = "69.70%"))]
        _69_70P = 0b10_0110,
        #[cfg_attr(feature = "serde", serde(rename = "70.35%"))]
        _70_35P = 0b10_0111,
        #[cfg_attr(feature = "serde", serde(rename = "71.00%"))]
        _71_00P = 0b10_1000,
        #[cfg_attr(feature = "serde", serde(rename = "71.65%"))]
        _71_65P = 0b10_1001,
        #[cfg_attr(feature = "serde", serde(rename = "72.30%"))]
        _72_30P = 0b10_1010,
        #[cfg_attr(feature = "serde", serde(rename = "72.95%"))]
        _72_95P = 0b10_1011,
        #[cfg_attr(feature = "serde", serde(rename = "73.60%"))]
        _73_60P = 0b10_1100,
        #[cfg_attr(feature = "serde", serde(rename = "74.25%"))]
        _74_25P = 0b10_1101,
        #[cfg_attr(feature = "serde", serde(rename = "74.90%"))]
        _74_90P = 0b10_1110,
        #[cfg_attr(feature = "serde", serde(rename = "75.55%"))]
        _75_55P = 0b10_1111,
        #[cfg_attr(feature = "serde", serde(rename = "76.20%"))]
        _76_20P = 0b11_0000,
        #[cfg_attr(feature = "serde", serde(rename = "76.85%"))]
        _76_85P = 0b11_0001,
        #[cfg_attr(feature = "serde", serde(rename = "77.50%"))]
        _77_50P = 0b11_0010,
    }

    /// VrefDq can be set to either Range1 (between 60% and 92.5% of VDDQ) or
    /// Range2 (between 45% and 77.5% of VDDQ). Range1 is intended for
    /// module-based systems, while Range2 is intended for point-to-point-based
    /// systems. In each range, Vref can be adjusted in steps of 0.65% VDDQ.
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum VrefDq {
        Range1(VrefDqRange1),
        Range2(VrefDqRange2),
    }

    impl VrefDq {
        const RANGE_MASK: i64 = 1 << 6;
    }

    impl ToPrimitive for VrefDq {
        fn to_i64(&self) -> Option<i64> {
            // Assumption: x.to_i64() is disjunct from Self::RANGE_MASK.
            Some(match self {
                Self::Range1(x) => x.to_i64()?,
                Self::Range2(x) => x.to_i64()? | Self::RANGE_MASK,
            })
        }
        fn to_u64(&self) -> Option<u64> {
            Some(self.to_i64()? as u64)
        }
    }

    impl FromPrimitive for VrefDq {
        fn from_u64(value: u64) -> Option<Self> {
            let range = value & (Self::RANGE_MASK as u64) != 0;
            let val = (value & 0x3f) as u8;
            if value & !(0x3f | (Self::RANGE_MASK as u64)) != 0 {
                // garbage bits
                return None;
            }
            match range {
                false => Some(VrefDq::Range1(VrefDqRange1::from_u8(val)?)),
                true => Some(VrefDq::Range2(VrefDqRange2::from_u8(val)?)),
            }
        }
        fn from_i64(value: i64) -> Option<Self> {
            if value >= 0 {
                let value: u64 = value.try_into().unwrap();
                Self::from_u64(value)
            } else {
                None
            }
        }
    }

    // See <https://www.micron.com/-/media/client/global/documents/products/data-sheet/dram/ddr4/8gb_auto_ddr4_dram.pdf>
    // Usually an array of those is used
    // Note: This structure is not used for soldered-down DRAM!
    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr4DataBusElement {
            dimm_slots_per_channel || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            ddr_rates || DdrRates : LU32 | pub get DdrRates : pub set DdrRates,
            vdd_io || RdimmDdr4Voltages : LU32 | pub get RdimmDdr4Voltages : pub set RdimmDdr4Voltages,
            dimm0_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks || Ddr4DimmRanks : LU32 | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            rtt_nom || RttNom : LU32 | pub get RttNom : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr || RttWr : LU32 | pub get RttWr : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park || RttPark : LU32 | pub get RttPark : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for data
            dqs_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for data strobe (bit clock)
            odt_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for on-die termination
            pmu_phy_vref || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 15
            pub(crate) vref_dq || VrefDq : LU32 | pub get VrefDq : pub set VrefDq, // MR6 vref calibration value; 23|30|32
        }
    }

    pub type RdimmDdr4DataBusElement = Ddr4DataBusElement; // AMD does this implicitly.
    pub type UdimmDdr4DataBusElement = Ddr4DataBusElement; // AMD does this implicitly.

    impl Default for Ddr4DataBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rates: 0x1373200.into(),
                vdd_io: 1.into(), // always
                dimm0_ranks: 2.into(),
                dimm1_ranks: 1.into(),

                rtt_nom: 0.into(),
                rtt_wr: 0.into(),
                rtt_park: 5.into(),
                dq_drive_strength: 62.into(), // always
                dqs_drive_strength: 62.into(), // always
                odt_drive_strength: 24.into(), // always
                pmu_phy_vref: 91.into(),
                vref_dq: 23.into(),
            }
        }
    }

    impl Ddr4DataBusElement {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            dimm_slots_per_channel: u32,
            ddr_rates: DdrRates,
            dimm0_ranks: Ddr4DimmRanks,
            dimm1_ranks: Ddr4DimmRanks,
            rtt_nom: RttNom,
            rtt_wr: RttWr,
            rtt_park: RttPark,
            pmu_phy_vref: u32,
            vref_dq: VrefDq,
        ) -> Result<Self> {
            Ok(Self {
                dimm_slots_per_channel: dimm_slots_per_channel.into(),
                ddr_rates: ddr_rates
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                dimm0_ranks: dimm0_ranks
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                dimm1_ranks: dimm1_ranks
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                rtt_nom: rtt_nom
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                rtt_wr: rtt_wr.to_u32().ok_or(Error::EntryTypeMismatch)?.into(),
                rtt_park: rtt_park
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                pmu_phy_vref: pmu_phy_vref.into(),
                vref_dq: vref_dq
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                ..Self::default()
            })
        }
    }

    impl EntryCompatible for Ddr4DataBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4DataBus) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4DataBus) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4DataBus) => true,
                // TODO: Check EntryId::Memory(PsSodimmDdr4DataBus) => true
                // Definitely not: EntryId::PsDramdownDdr4DataBus => true
                // Definitely not: MemoryEntryId::PsLrdimmDdr4DataBus
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4DataBusElement {
            dimm_slots_per_channel || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            ddr_rates || DdrRates : LU32 | pub get DdrRates : pub set DdrRates,
            vdd_io || LrdimmDdr4Voltages : LU32 | pub get LrdimmDdr4Voltages : pub set LrdimmDdr4Voltages,
            dimm0_ranks || LrdimmDdr4DimmRanks : LU32 | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks || LrdimmDdr4DimmRanks : LU32 | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,

            rtt_nom || RttNom : LU32 | pub get RttNom : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr || RttWr : LU32 | pub get RttWr : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park || RttPark : LU32 | pub get RttPark : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for data
            dqs_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for data strobe (bit clock)
            odt_drive_strength || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for on-die termination
            pmu_phy_vref || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            vref_dq || SerdeHex32 : LU32 | pub get u32 : pub set u32, // MR6 vref calibration value; 23|30|32
        }
    }

    impl Default for LrdimmDdr4DataBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rates: 0x1373200.into(),
                vdd_io: 1.into(),
                dimm0_ranks: 2.into(),
                dimm1_ranks: 1.into(),

                rtt_nom: 0.into(),
                rtt_wr: 0.into(),
                rtt_park: 5.into(),
                dq_drive_strength: 62.into(), // always
                dqs_drive_strength: 62.into(), // always
                odt_drive_strength: 24.into(), // always
                pmu_phy_vref: 91.into(),
                vref_dq: 23.into(),
            }
        }
    }

    impl LrdimmDdr4DataBusElement {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            dimm_slots_per_channel: u32,
            ddr_rates: DdrRates,
            dimm0_ranks: LrdimmDdr4DimmRanks,
            dimm1_ranks: LrdimmDdr4DimmRanks,
            rtt_nom: RttNom,
            rtt_wr: RttWr,
            rtt_park: RttPark,
            pmu_phy_vref: u32,
            vref_dq: VrefDq,
        ) -> Result<Self> {
            Ok(Self {
                dimm_slots_per_channel: dimm_slots_per_channel.into(),
                ddr_rates: ddr_rates
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                dimm0_ranks: dimm0_ranks
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                dimm1_ranks: dimm1_ranks
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                rtt_nom: rtt_nom
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                rtt_wr: rtt_wr.to_u32().ok_or(Error::EntryTypeMismatch)?.into(),
                rtt_park: rtt_park
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                pmu_phy_vref: pmu_phy_vref.into(),
                vref_dq: vref_dq
                    .to_u32()
                    .ok_or(Error::EntryTypeMismatch)?
                    .into(),
                ..Self::default()
            })
        }
    }

    impl EntryCompatible for LrdimmDdr4DataBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4DataBus) => true,
                // TODO: Check EntryId::Memory(PsSodimmDdr4DataBus) => true
                // Definitely not: EntryId::PsDramdownDdr4DataBus => true
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct RdimmDdr5BusElementHeader {
            total_size || u32 : LU32,
            target_memclk || u32 : LU32,
            dimm_slots_per_channel: u8,
            dimm0_rank_bitmap: u8,
            dimm1_rank_bitmap: u8,
            sdram_io_width_bitmap: u8,
        }
    }

    impl Default for RdimmDdr5BusElementHeader {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                target_memclk: 2000.into(),
                dimm_slots_per_channel: 2,
                dimm0_rank_bitmap: 4,
                dimm1_rank_bitmap: 4,
                sdram_io_width_bitmap: 1,
            }
        }
    }

    impl Getter<Result<RdimmDdr5BusElementHeader>> for RdimmDdr5BusElementHeader {
        fn get1(self) -> Result<RdimmDdr5BusElementHeader> {
            Ok(self)
        }
    }

    impl Setter<RdimmDdr5BusElementHeader> for RdimmDdr5BusElementHeader {
        fn set1(&mut self, value: RdimmDdr5BusElementHeader) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct RdimmDdr5BusElementPayload {
            total_size || u32 : LU32,
            ca_timing_mode || u32 : LU32,
            dimm0_rttnomwr || u32 : LU32,
            dimm0_rttnomrd || u32 : LU32,
            dimm0_rttwr || u32 : LU32,
            dimm0_rttpack || u32 : LU32,
            dimm0_dqs_rttpark || u32 : LU32,
            dimm1_rttnomwr || u32 : LU32,
            dimm1_rttnomrd || u32 : LU32,
            dimm1_rttwr || u32 : LU32,
            dimm1_rttpack || u32 : LU32,
            dimm1_dqs_rttpark || u32 : LU32,
            dram_drv || u32 : LU32,
            ck_odt_a || u32 : LU32,
            cs_odt_a || u32 : LU32,
            ca_odt_a || u32 : LU32,
            ck_odt_b || u32 : LU32,
            cs_odt_b || u32 : LU32,
            ca_odt_b || u32 : LU32,
            p_odt || u32 : LU32,
            dq_drv || u32 : LU32,
            alert_pullup || u32 : LU32,
            ca_drv || u32 : LU32,
            phy_vref || u32 : LU32,
            dq_vref || u32 : LU32,
            ca_vref || u32 : LU32,
            cs_vref || u32 : LU32,
            d_ca_vref || u32 : LU32,
            d_cs_vref || u32 : LU32,
            rx_dfe || u32 : LU32,
            tx_dfe || u32 : LU32,
        }
    }

    impl Default for RdimmDdr5BusElementPayload {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                ca_timing_mode: 1.into(),
                dimm0_rttnomwr: 120.into(),
                dimm0_rttnomrd: 120.into(),
                dimm0_rttwr: 240.into(),
                dimm0_rttpack: 60.into(),
                dimm0_dqs_rttpark: 80.into(),
                dimm1_rttnomwr: 120.into(),
                dimm1_rttnomrd: 120.into(),
                dimm1_rttwr: 240.into(),
                dimm1_rttpack: 60.into(),
                dimm1_dqs_rttpark: 80.into(),
                dram_drv: 34.into(),
                ck_odt_a: 0.into(),
                cs_odt_a: 0.into(),
                ca_odt_a: 0.into(),
                ck_odt_b: 40.into(),
                cs_odt_b: 40.into(),
                ca_odt_b: 60.into(),
                p_odt: 60.into(),
                dq_drv: 34.into(),
                alert_pullup: 80.into(),
                ca_drv: 40.into(),
                phy_vref: 45.into(),
                dq_vref: 45.into(),
                ca_vref: 63.into(),
                cs_vref: 56.into(),
                d_ca_vref: 32.into(),
                d_cs_vref: 40.into(),
                rx_dfe: 1.into(),
                tx_dfe: 1.into(),
            }
        }
    }

    impl Getter<Result<RdimmDdr5BusElementPayload>> for RdimmDdr5BusElementPayload {
        fn get1(self) -> Result<RdimmDdr5BusElementPayload> {
            Ok(self)
        }
    }

    impl Setter<RdimmDdr5BusElementPayload> for RdimmDdr5BusElementPayload {
        fn set1(&mut self, value: RdimmDdr5BusElementPayload) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Default, Copy, Clone)]
        #[repr(C, packed)]
        pub struct RdimmDdr5BusElement {
            header: RdimmDdr5BusElementHeader,
            payload: RdimmDdr5BusElementPayload,
        }
    }

    impl EntryCompatible for RdimmDdr5BusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::PsRdimmDdr5Bus))
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElementHeader { // Genoa
            total_size || u32 : LU32,
            dimm_slots_per_channel: u8,
            dimm0_rank_bitmap: u8,
            dimm1_rank_bitmap: u8,
            sdram_io_width_bitmap: u8,
        }
    }

    impl Default for MemDfeSearchElementHeader {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                dimm_slots_per_channel: 1,
                dimm0_rank_bitmap: 2,
                dimm1_rank_bitmap: 1,
                sdram_io_width_bitmap: 255,
            }
        }
    }

    impl Getter<Result<MemDfeSearchElementHeader>> for MemDfeSearchElementHeader {
        fn get1(self) -> Result<MemDfeSearchElementHeader> {
            Ok(self)
        }
    }

    impl Setter<MemDfeSearchElementHeader> for MemDfeSearchElementHeader {
        fn set1(&mut self, value: MemDfeSearchElementHeader) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElementHeader12 {
            total_size || u32 : LU32,
            _reserved || #[serde(default)] u32 : LU32,
            dimm_slots_per_channel: u8,
            dimm0_rank_bitmap: u8,
            dimm1_rank_bitmap: u8,
            sdram_io_width_bitmap: u8,
        }
    }

    impl Default for MemDfeSearchElementHeader12 {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                _reserved: 0.into(),
                dimm_slots_per_channel: 1,
                dimm0_rank_bitmap: 2,
                dimm1_rank_bitmap: 1,
                sdram_io_width_bitmap: 255,
            }
        }
    }

    impl Getter<Result<MemDfeSearchElementHeader12>>
        for MemDfeSearchElementHeader12
    {
        fn get1(self) -> Result<MemDfeSearchElementHeader12> {
            Ok(self)
        }
    }

    impl Setter<MemDfeSearchElementHeader12> for MemDfeSearchElementHeader12 {
        fn set1(&mut self, value: MemDfeSearchElementHeader12) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElementPayload12 {
            total_size || u32 : LU32,
            /// -40..=40
            tx_dfe_tap_1_start || u8 : u8,
            /// -40..=40
            tx_dfe_tap_1_end || u8 : u8,
            /// -15..=15
            tx_dfe_tap_2_start || u8 : u8,
            /// -15..=15
            tx_dfe_tap_2_end || u8 : u8,
            /// -12..=12
            tx_dfe_tap_3_start || u8 : u8,
            /// -12..=12
            tx_dfe_tap_3_end || u8 : u8,
            /// -8..=8
            tx_dfe_tap_4_start || u8 : u8,
            /// -8..=8
            tx_dfe_tap_4_end || u8 : u8,
        }
    }
    impl Default for MemDfeSearchElementPayload12 {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                tx_dfe_tap_1_start: 0,
                tx_dfe_tap_1_end: 24,
                tx_dfe_tap_2_start: 0,
                tx_dfe_tap_2_end: 0,
                tx_dfe_tap_3_start: 0,
                tx_dfe_tap_3_end: 0,
                tx_dfe_tap_4_start: 0,
                tx_dfe_tap_4_end: 0,
            }
        }
    }

    impl Getter<Result<MemDfeSearchElementPayload12>>
        for MemDfeSearchElementPayload12
    {
        fn get1(self) -> Result<MemDfeSearchElementPayload12> {
            Ok(self)
        }
    }

    impl Setter<MemDfeSearchElementPayload12> for MemDfeSearchElementPayload12 {
        fn set1(&mut self, value: MemDfeSearchElementPayload12) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElementPayloadExt12 {
            total_size || u32 : LU32,
            rx_dfe_tap_2_min_mv || i8 : i8,
            rx_dfe_tap_2_max_mv || i8 : i8,
            rx_dfe_tap_3_min_mv || i8 : i8,
            rx_dfe_tap_3_max_mv || i8 : i8,
            rx_dfe_tap_4_min_mv || i8 : i8,
            rx_dfe_tap_4_max_mv || i8 : i8,
            _reserved_1 || #[serde(default)] SerdeHex8 : u8,
            _reserved_2 || #[serde(default)] SerdeHex8 : u8,
        }
    }
    impl Default for MemDfeSearchElementPayloadExt12 {
        fn default() -> Self {
            Self {
                total_size: (size_of::<Self>() as u32).into(),
                rx_dfe_tap_2_min_mv: 0,
                rx_dfe_tap_2_max_mv: 0,
                rx_dfe_tap_3_min_mv: 0,
                rx_dfe_tap_3_max_mv: 0,
                rx_dfe_tap_4_min_mv: 0,
                rx_dfe_tap_4_max_mv: 0,
                _reserved_1: 0, // FIXME check
                _reserved_2: 0, // FIXME check
            }
        }
    }

    impl Getter<Result<MemDfeSearchElementPayloadExt12>>
        for MemDfeSearchElementPayloadExt12
    {
        fn get1(self) -> Result<MemDfeSearchElementPayloadExt12> {
            Ok(self)
        }
    }

    impl Setter<MemDfeSearchElementPayloadExt12>
        for MemDfeSearchElementPayloadExt12
    {
        fn set1(&mut self, value: MemDfeSearchElementPayloadExt12) {
            *self = value
        }
    }

    make_accessors! {
        /// Decision Feedback Equalization.
        /// See also UMC::Phy::RxDFETapCtrl in the memory controller.
        /// See <https://ieeexplore.ieee.org/document/1455678>.
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Default, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElement36 {
            header: MemDfeSearchElementHeader12,
            payload: MemDfeSearchElementPayload12,
            payload_ext: MemDfeSearchElementPayloadExt12,
        }
    }

    impl EntryCompatible for MemDfeSearchElement36 {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::MemDfeSearch))
        }
    }

    make_accessors! {
        /// Decision Feedback Equalization.
        /// See also UMC::Phy::RxDFETapCtrl in the memory controller.
        /// See <https://ieeexplore.ieee.org/document/1455678>.
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Default, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MemDfeSearchElement32 {
            header: MemDfeSearchElementHeader,
            payload: MemDfeSearchElementPayload12,
            payload_ext: MemDfeSearchElementPayloadExt12,
        }
    }

    impl EntryCompatible for MemDfeSearchElement32 {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::MemDfeSearch))
        }
    }

    // ACTUAL 1/T, where T is one period.  For DDR, that means DDR400 has
    // frequency 200.
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum DdrSpeed {
        Ddr400 = 200,
        Ddr533 = 266,
        Ddr667 = 333,
        Ddr800 = 400,
        Ddr1066 = 533,
        Ddr1333 = 667,
        Ddr1600 = 800,
        Ddr1866 = 933,
        Ddr2100 = 1050,
        Ddr2133 = 1067,
        Ddr2400 = 1200,
        Ddr2667 = 1333,
        Ddr2733 = 1367,
        Ddr2800 = 1400,
        Ddr2867 = 1433,
        Ddr2933 = 1467,
        Ddr3000 = 1500,
        Ddr3067 = 1533,
        Ddr3133 = 1567,
        Ddr3200 = 1600,
        Ddr3267 = 1633,
        Ddr3333 = 1667,
        Ddr3400 = 1700,
        Ddr3467 = 1733,
        Ddr3533 = 1767,
        Ddr3600 = 1800,
        Ddr3667 = 1833,
        Ddr3733 = 1867,
        Ddr3800 = 1900,
        Ddr3867 = 1933,
        Ddr3933 = 1967,
        Ddr4000 = 2000,
        Ddr4066 = 2033,
        Ddr4134 = 2067,
        Ddr4200 = 2100,
        Ddr4266 = 2133,
        Ddr4334 = 2167,
        Ddr4400 = 2200,
        Ddr4466 = 2233,
        Ddr4534 = 2267,
        Ddr4600 = 2300,
        Ddr4666 = 2333,
        Ddr4734 = 2367,
        Ddr4800 = 2400,
        Ddr4866 = 2433,
        Ddr4934 = 2467,
        Ddr5000 = 2500,
        Ddr5100 = 2550,
        Ddr5200 = 2600,
        Ddr5300 = 2650,
        Ddr5400 = 2700,
        Ddr5500 = 2750,
        Ddr5600 = 2800,
        Ddr5700 = 2850,
        Ddr5800 = 2900,
        Ddr5900 = 2950,
        Ddr6000 = 3000,
        Ddr6100 = 3050,
        Ddr6200 = 3100,
        Ddr6300 = 3150,
        Ddr6400 = 3200,
        Ddr6500 = 3250,
        Ddr6600 = 3300,
        Ddr6700 = 3350,
        Ddr6800 = 3400,
        Ddr6900 = 3450,
        Ddr7000 = 3500,
        Ddr7100 = 3550,
        Ddr7200 = 3600,
        Ddr7300 = 3650,
        Ddr7400 = 3700,
        Ddr7500 = 3750,
        Ddr7600 = 3800,
        Ddr7700 = 3850,
        Ddr7800 = 3900,
        Ddr7900 = 3950,
        Ddr8000 = 4000,
        Ddr8100 = 4050,
        Ddr8200 = 4100,
        Ddr8300 = 4150,
        Ddr8400 = 4200,
        Ddr8500 = 4250,
        Ddr8600 = 4300,
        Ddr8700 = 4350,
        Ddr8800 = 4400,
        UnsupportedRome = 2201,
        UnsupportedMilan = 4401, // and Turin
    }

    // Usually an array of those is used
    // Note: This structure is not used for LR DRAM
    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MaxFreqElement {
            dimm_slots_per_channel || DimmsPerChannel : u8 | pub get DimmsPerChannel : pub set DimmsPerChannel,
            _reserved_ || #[serde(default)] SerdeHex8 : u8,
            conditions || [SerdeHex16; 4] : [LU16; 4], // number of dimm on a channel, number of single-rank dimm, number of dual-rank dimm, number of quad-rank dimm
            speeds || [SerdeHex16; 3] : [LU16; 3], // speed limit with voltage 1.5 V, 1.35 V, 1.25 V // FIXME make accessible
        }
    }
    impl MaxFreqElement {
        pub fn dimm_count(&self) -> Result<u16> {
            Ok(self.conditions[0].get())
        }
        pub fn set_dimm_count(&mut self, value: u16) {
            self.conditions[0].set(value);
        }
        pub fn single_rank_count(&self) -> Result<u16> {
            Ok(self.conditions[1].get())
        }
        pub fn set_single_rank_count(&mut self, value: u16) {
            self.conditions[1].set(value);
        }
        pub fn dual_rank_count(&self) -> Result<u16> {
            Ok(self.conditions[2].get())
        }
        pub fn set_dual_rank_count(&mut self, value: u16) {
            self.conditions[2].set(value);
        }
        pub fn quad_rank_count(&self) -> Result<u16> {
            Ok(self.conditions[3].get())
        }
        pub fn set_quad_rank_count(&mut self, value: u16) {
            self.conditions[3].set(value);
        }
        pub fn speed(&self) -> Result<DdrSpeed> {
            DdrSpeed::from_u16(self.speeds[0].get())
                .ok_or(Error::EntryTypeMismatch)
        }
        pub fn set_speed(&mut self, value: DdrSpeed) {
            self.speeds[0].set(value.to_u16().unwrap())
        }

        /// Note: unsupported_speed differs between Rome and Milan--so pass
        /// UnsupportedRome or UnsupportedMilan here as appropriate.
        pub fn new(
            unsupported_speed: DdrSpeed,
            dimm_slots_per_channel: DimmsPerChannel,
            dimm_count: u16,
            single_rank_count: u16,
            dual_rank_count: u16,
            quad_rank_count: u16,
            speed_0: DdrSpeed,
        ) -> Self {
            Self {
                dimm_slots_per_channel: dimm_slots_per_channel.to_u8().unwrap(),
                conditions: [
                    dimm_count.into(),
                    single_rank_count.into(),
                    dual_rank_count.into(),
                    quad_rank_count.into(),
                ],
                speeds: [
                    speed_0.to_u16().unwrap().into(),
                    unsupported_speed.to_u16().unwrap().into(),
                    unsupported_speed.to_u16().unwrap().into(),
                ],
                ..Self::default()
            }
        }
    }

    impl Default for MaxFreqElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1,
                _reserved_: 0,
                conditions: [1.into(), 1.into(), 0.into(), 0.into()],
                speeds: [1600.into(), 4401.into(), 4401.into()],
            }
        }
    }

    // AMD does this, and we are compatible to it
    pub type StretchFreqElement = MaxFreqElement;

    impl EntryCompatible for MaxFreqElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr5MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr5MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr5MaxFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr5MaxFreqC1) => true,

                // Definitely not: EntryId::Memory(MemoryEntryId::PsLrdimmDdr4) => true.
                // TODO (bug# 124): EntryId::Memory(PsSodimmDdr4MaxFreq) => true
                // Definitely not: EntryId::PsDramdownDdr4MaxFreq => true
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4StretchFreq) => {
                    true
                }
                EntryId::Memory(MemoryEntryId::PsRdimmDdr5StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr5StretchFreq) => {
                    true
                }
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr5StretchFreq) => true,

                _ => false,
            }
        }
    }

    // Usually an array of those is used
    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrMaxFreqElement {
            dimm_slots_per_channel || SerdeHex8 : u8 | pub get u8 : pub set u8,
            _reserved_ || #[serde(default)] SerdeHex8 : u8,
            pub conditions || [SerdeHex16; 4] : [LU16; 4], // maybe: number of dimm on a channel, 0, number of lr dimm, 0 // FIXME: Make accessible
            pub speeds || [SerdeHex16; 3] : [LU16; 3], // maybe: speed limit with voltage 1.5 V, 1.35 V, 1.25 V; FIXME: Make accessible
        }
    }

    impl Default for LrMaxFreqElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1,
                _reserved_: 0,
                conditions: [1.into(), 0.into(), 1.into(), 0.into()],
                speeds: [1600.into(), 4401.into(), 4401.into()],
            }
        }
    }

    impl LrMaxFreqElement {
        /// Note: unsupported_speed differs between Rome and Milan--so pass
        /// UnsupportedRome or UnsupportedMilan here as appropriate.
        pub fn new(
            unsupported_speed: DdrSpeed,
            dimm_slots_per_channel: DimmsPerChannel,
            dimm_count: u16,
            v_1_5_count: u16,
            v_1_35_count: u16,
            v_1_25_count: u16,
            speed_0: DdrSpeed,
        ) -> Self {
            Self {
                dimm_slots_per_channel: dimm_slots_per_channel.to_u8().unwrap(),
                conditions: [
                    dimm_count.into(),
                    v_1_5_count.into(),
                    v_1_35_count.into(),
                    v_1_25_count.into(),
                ],
                speeds: [
                    speed_0.to_u16().unwrap().into(),
                    unsupported_speed.to_u16().unwrap().into(),
                    unsupported_speed.to_u16().unwrap().into(),
                ],
                ..Self::default()
            }
        }
    }

    impl EntryCompatible for LrMaxFreqElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4MaxFreq)
            )
        }
    }

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Clone, Copy)]
        #[repr(C, packed)]
        pub struct Gpio {
            pin || SerdeHex8 : u8 | pub get u8 : pub set u8, // in FCH
            iomux_control || SerdeHex8 : u8 | pub get u8 : pub set u8, // how to configure that pin
            bank_control || SerdeHex8 : u8 | pub get u8 : pub set u8, // how to configure bank control
        }
    }

    impl Getter<Result<Gpio>> for Gpio {
        fn get1(self) -> Result<Gpio> {
            Ok(self)
        }
    }

    impl Setter<Gpio> for Gpio {
        fn set1(&mut self, value: Gpio) {
            *self = value;
        }
    }

    impl Gpio {
        pub fn new(pin: u8, iomux_control: u8, bank_control: u8) -> Self {
            Self { pin, iomux_control, bank_control }
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[derive(PartialEq, Debug, Copy, Clone)]
        #[repr(u32)]
        pub struct ErrorOutControlBeepCodePeakAttr {
            #[bits = 5]
            pub peak_count || u8 : B5 | pub get u8 : pub set u8,
            /// PULSE_WIDTH: in units of 0.1 s
            #[bits = 3]
            pub pulse_width || u8 : B3 | pub get u8 : pub set u8,
            #[bits = 4]
            pub repeat_count || u8 : B4 | pub get u8 : pub set u8,
            #[bits = 20]
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        ErrorOutControlBeepCodePeakAttr,
        0b1111_1111_1111,
        u32
    );

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum ErrorOutControlBeepCodeErrorType {
        General = 3,
        Memory = 4,
        Df = 5,
        Ccx = 6,
        Gnb = 7,
        Psp = 8,
        Smu = 9,
        Unknown = 0xf, // that's specified as "Unknown".
    }
    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct ErrorOutControlBeepCode {
            error_type || ErrorOutControlBeepCodeErrorType : LU16,
            peak_map || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            peak_attr || ErrorOutControlBeepCodePeakAttr : LU32 | pub get ErrorOutControlBeepCodePeakAttr : pub set ErrorOutControlBeepCodePeakAttr,
        }
    }
    #[cfg(feature = "serde")]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "serde", serde(rename = "ErrorOutControlBeepCode"))]
    pub(crate) struct CustomSerdeErrorOutControlBeepCode {
        pub custom_error_type: ErrorOutControlBeepCodeErrorType,
        pub peak_map: SerdeHex16,
        pub peak_attr: ErrorOutControlBeepCodePeakAttr,
    }
    #[cfg(feature = "serde")]
    impl ErrorOutControlBeepCode {
        pub(crate) fn serde_custom_error_type(
            &self,
        ) -> Result<ErrorOutControlBeepCodeErrorType> {
            self.error_type()
        }
        pub(crate) fn serde_with_custom_error_type(
            &mut self,
            value: ErrorOutControlBeepCodeErrorType,
        ) -> &mut Self {
            self.with_error_type(value)
        }
    }
    impl Default for ErrorOutControlBeepCode {
        fn default() -> Self {
            ErrorOutControlBeepCode::new(
                ErrorOutControlBeepCodeErrorType::General,
                0,
                ErrorOutControlBeepCodePeakAttr::new(),
            )
        }
    }
    impl ErrorOutControlBeepCode {
        pub fn error_type(&self) -> Result<ErrorOutControlBeepCodeErrorType> {
            ErrorOutControlBeepCodeErrorType::from_u16(
                (self.error_type.get() & 0xF000) >> 12,
            )
            .ok_or(Error::EntryTypeMismatch)
        }
        pub fn set_error_type(
            &mut self,
            value: ErrorOutControlBeepCodeErrorType,
        ) {
            self.error_type.set(
                (self.error_type.get() & 0x0FFF)
                    | (value.to_u16().unwrap() << 12),
            );
        }
        pub fn with_error_type(
            &mut self,
            value: ErrorOutControlBeepCodeErrorType,
        ) -> &mut Self {
            self.set_error_type(value);
            self
        }
        pub fn new(
            error_type: ErrorOutControlBeepCodeErrorType,
            peak_map: u16,
            peak_attr: ErrorOutControlBeepCodePeakAttr,
        ) -> Self {
            Self {
                error_type: ((error_type.to_u16().unwrap() << 12) | 0xFFF)
                    .into(),
                peak_map: peak_map.into(),
                peak_attr: peak_attr.to_u32().unwrap().into(),
            }
        }
    }

    impl Getter<Result<[ErrorOutControlBeepCode; 8]>>
        for [ErrorOutControlBeepCode; 8]
    {
        fn get1(self) -> Result<[ErrorOutControlBeepCode; 8]> {
            Ok(self.map(|v| v))
        }
    }

    impl Setter<[ErrorOutControlBeepCode; 8]> for [ErrorOutControlBeepCode; 8] {
        fn set1(&mut self, value: [ErrorOutControlBeepCode; 8]) {
            *self = value.map(|v| v);
        }
    }

    macro_rules! define_ErrorOutControl {($struct_name:ident, $padding_before_gpio:expr, $padding_after_gpio: expr) => (
        make_accessors! {
            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy,
Clone)]
            #[repr(C, packed)]
            pub struct $struct_name {
                enable_error_reporting || bool : BU8 | pub get bool : pub set bool,
                enable_error_reporting_gpio || bool : BU8 | pub get bool : pub set bool, // FIXME: Remove
                enable_error_reporting_beep_codes || bool : BU8 | pub get bool : pub set bool,
                /// Note: Receiver of the error log: Send 0xDEAD5555 to the INPUT_PORT to acknowledge.
                enable_using_handshake || bool : BU8 | pub get bool : pub set bool, // otherwise see output_delay
                input_port || SerdeHex32 : LU32 | pub get u32 : pub set u32, // for handshake
                output_delay || SerdeHex32 : LU32 | pub get u32 : pub set u32, // if no handshake; in units of 10 ns.
                output_port || SerdeHex32 : LU32 | pub get u32 : pub set u32,
                stop_on_first_fatal_error || bool : BU8 | pub get bool : pub set bool,
                _reserved_ || #[serde(default)] [SerdeHex8; 3] : [u8; 3],
                input_port_size || PortSize : LU32 | pub get PortSize : pub set PortSize,
                output_port_size || PortSize : LU32 | pub get PortSize : pub set PortSize,
                input_port_type || PortType : LU32 | pub get PortType : pub set PortType, // PortType; default: 6
                output_port_type || PortType : LU32 | pub get PortType : pub set PortType, // PortType; default: 6
                clear_acknowledgement || bool : BU8 | pub get bool : pub set bool,
                _reserved_before_gpio || #[serde(default)] [SerdeHex8; $padding_before_gpio] : [u8; $padding_before_gpio],
                pub error_reporting_gpio: Gpio,
                _reserved_after_gpio || #[serde(default)] [SerdeHex8; $padding_after_gpio] : [u8; $padding_after_gpio],
                beep_code_table: [ErrorOutControlBeepCode; 8], // FIXME: Make accessible
                //beep_code_table: [ErrorOutControlBeepCode; 8] | pub get
                //    [ErrorOutControlBeepCode; 8] : pub set [ErrorOutControlBeepCode; 8], // FIXME: Make accessible
                enable_heart_beat || bool : BU8 | pub get bool : pub set bool,
                enable_power_good_gpio || bool : BU8 | pub get bool : pub set bool,
                pub power_good_gpio: Gpio,
                _reserved_end || #[serde(default)] [SerdeHex8; 3] : [u8; 3], // pad
            }
        }

        impl $struct_name {
            pub fn error_reporting_gpio(&self) -> Result<Option<Gpio>> {
                match self.enable_error_reporting_gpio {
                    BU8(1) => Ok(Some(self.error_reporting_gpio)),
                    BU8(0) => Ok(None),
                    _ => Err(Error::EntryTypeMismatch),
                }
            }
            pub fn set_error_reporting_gpio(&mut self, value: Option<Gpio>) {
                match value {
                    Some(value) => {
                        self.enable_error_reporting_gpio = BU8(1);
                        self.error_reporting_gpio = value;
                    },
                    None => {
                        self.enable_error_reporting_gpio = BU8(0);
                        self.error_reporting_gpio = Gpio::new(0, 0, 0);
                    },
                }
            }
            pub fn with_error_reporting_gpio(&mut self, value: Option<Gpio>) -> &mut Self{
                self.set_error_reporting_gpio(value);
                self
            }
            pub fn beep_code_table(&self) -> Result<[ErrorOutControlBeepCode; 8]> {
                self.beep_code_table.get1()
            }
            pub fn set_beep_code_table(&mut self, value: [ErrorOutControlBeepCode; 8]) {
                self.beep_code_table.set1(value);
            }
            pub fn with_beep_code_table(&mut self, value:[ErrorOutControlBeepCode; 8]) -> &mut Self {
                self.set_beep_code_table(value);
                self
            }
            pub fn power_good_gpio(&self) -> Result<Option<Gpio>> {
                match self.enable_power_good_gpio {
                    BU8(1) => Ok(Some(self.power_good_gpio)),
                    BU8(0) => Ok(None),
                    _ => Err(Error::EntryTypeMismatch),
                }
            }
            pub fn set_power_good_gpio(&mut self, value: Option<Gpio>) {
                match value {
                    Some(value) => {
                        self.enable_power_good_gpio = BU8(1);
                        self.power_good_gpio = value;
                    },
                    None => {
                        self.enable_power_good_gpio = BU8(0);
                        self.power_good_gpio = Gpio::new(0, 0, 0);
                    },
                }
            }
            pub fn with_power_good_gpio(&mut self, value: Option<Gpio>) -> &mut Self {
                self.set_power_good_gpio(value);
                self
            }
        }

        impl EntryCompatible for $struct_name {
            fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
                match entry_id {
                    EntryId::Memory(MemoryEntryId::ErrorOutControl) => true,
                    _ => false,
                }
            }
        }

        impl HeaderWithTail for $struct_name {
            type TailArrayItemType<'de> = ();
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Self {
                    enable_error_reporting: BU8(0),
                    enable_error_reporting_gpio: BU8(0),
                    enable_error_reporting_beep_codes: BU8(0),
                    enable_using_handshake: BU8(0),
                    input_port: 0.into(),
                    output_delay: 15000.into(),
                    output_port: 0x80.into(),
                    stop_on_first_fatal_error: BU8(1), // !
                    _reserved_: [0; 3],
                    input_port_size: 1.into(),
                    output_port_size: 4.into(),
                    input_port_type: (PortType::FchHtIo as u32).into(),
                    output_port_type: (PortType::FchHtIo as u32).into(),
                    clear_acknowledgement: BU8(1),
                    _reserved_before_gpio: [0; $padding_before_gpio],
                    error_reporting_gpio: Gpio::new(0, 0, 0), // probably invalid
                    _reserved_after_gpio: [0; $padding_after_gpio],
                    beep_code_table: [
                        ErrorOutControlBeepCode {
                            error_type: U16::<LittleEndian>::new(0x3fff),
                            peak_map: 1.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(8).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x4fff.into(),
                            peak_map: 2.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x5fff.into(),
                            peak_map: 3.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x6fff.into(),
                            peak_map: 4.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x7fff.into(),
                            peak_map: 5.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x8fff.into(),
                            peak_map: 6.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0x9fff.into(),
                            peak_map: 7.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(20).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                        ErrorOutControlBeepCode {
                            error_type: 0xffff.into(),
                            peak_map: 2.into(),
                            peak_attr: ErrorOutControlBeepCodePeakAttr::new().with_peak_count(4).with_pulse_width(0).with_repeat_count(0).to_u32().unwrap().into(),
                        },
                    ],
                    enable_heart_beat: BU8(1),
                    enable_power_good_gpio: BU8(0),
                    power_good_gpio: Gpio::new(0, 0, 0), // probably invalid
                    _reserved_end: [0; 3],
                }
            }
        }
        impl $struct_name {
            pub fn new() -> Self {
                Self::default()
            }
        }
    )}

    define_ErrorOutControl!(ErrorOutControl116, 3, 1); // Milan
    define_ErrorOutControl!(ErrorOutControl112, 0, 0);

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Default, Clone, Copy, BitfieldSpecifier)]
        pub struct Ddr4OdtPatDimmRankBitmaps {
            #[bits = 4]
            pub dimm0: Ddr4DimmRanks | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @0
            #[bits = 4]
            pub dimm1: Ddr4DimmRanks | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @4
            #[bits = 4]
            pub dimm2: Ddr4DimmRanks | pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @8
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        Ddr4OdtPatDimmRankBitmaps,
        0b0111_0111_0111,
        u32
    );
    type OdtPatPattern = B4; // TODO: Meaning

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct OdtPatPatterns {
            pub reading_pattern || SerdeHex8 : OdtPatPattern | pub get OdtPatPattern : pub set OdtPatPattern, // @bit 0
            pub _reserved_1 || #[serde(default)] SerdeHex8 : B4,             // @bit 4
            pub writing_pattern || SerdeHex8 : OdtPatPattern | pub get OdtPatPattern : pub set OdtPatPattern, // @bit 8
            pub _reserved_2 || #[serde(default)] SerdeHex32 : B20,
        }
    }
    impl_bitfield_primitive_conversion!(OdtPatPatterns, 0b1111_0000_1111, u32);

    make_accessors! {
        /// See PPR 12.7.2.2 DRAM ODT Pin Control
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr4OdtPatElement {
            dimm_rank_bitmaps || Ddr4OdtPatDimmRankBitmaps : LU32 | pub get Ddr4OdtPatDimmRankBitmaps : pub set Ddr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs1_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs2_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs3_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
        }
    }

    impl Default for Ddr4OdtPatElement {
        fn default() -> Self {
            Self {
                dimm_rank_bitmaps: (1 | (2 << 4)).into(),
                cs0_odt_patterns: 0.into(),
                cs1_odt_patterns: 0.into(),
                cs2_odt_patterns: 0.into(),
                cs3_odt_patterns: 0.into(),
            }
        }
    }

    impl Ddr4OdtPatElement {
        pub fn new(
            dimm_rank_bitmaps: Ddr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns: OdtPatPatterns,
            cs1_odt_patterns: OdtPatPatterns,
            cs2_odt_patterns: OdtPatPatterns,
            cs3_odt_patterns: OdtPatPatterns,
        ) -> Self {
            Self {
                dimm_rank_bitmaps: dimm_rank_bitmaps.to_u32().unwrap().into(),
                cs0_odt_patterns: cs0_odt_patterns.to_u32().unwrap().into(),
                cs1_odt_patterns: cs1_odt_patterns.to_u32().unwrap().into(),
                cs2_odt_patterns: cs2_odt_patterns.to_u32().unwrap().into(),
                cs3_odt_patterns: cs3_odt_patterns.to_u32().unwrap().into(),
            }
        }
    }

    impl EntryCompatible for Ddr4OdtPatElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                // definitely not
                // EntryId::Memory(MemoryEntryId::PsLrdimmDdr4OdtPat) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4OdtPat) => true,
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4OdtPat) => true,
                _ => false,
            }
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy, BitfieldSpecifier)]
        pub struct LrdimmDdr4OdtPatDimmRankBitmaps {
            pub dimm0: LrdimmDdr4DimmRanks | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 0
            pub dimm1: LrdimmDdr4DimmRanks | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 4
            pub dimm2: LrdimmDdr4DimmRanks | pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 8
            pub _reserved_1 || #[serde(default)] SerdeHex32 : B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        LrdimmDdr4OdtPatDimmRankBitmaps,
        0b0011_0011_0011,
        u32
    );

    make_accessors! {
        /// See PPR DRAM ODT Pin Control
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4OdtPatElement {
            dimm_rank_bitmaps || LrdimmDdr4OdtPatDimmRankBitmaps : LU32 | pub get LrdimmDdr4OdtPatDimmRankBitmaps : pub set LrdimmDdr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs1_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs2_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs3_odt_patterns || OdtPatPatterns : LU32 | pub get OdtPatPatterns : pub set OdtPatPatterns,
        }
    }

    impl Default for LrdimmDdr4OdtPatElement {
        fn default() -> Self {
            Self {
                dimm_rank_bitmaps: (1 | (2 << 4)).into(),
                cs0_odt_patterns: 0.into(),
                cs1_odt_patterns: 0.into(),
                cs2_odt_patterns: 0.into(),
                cs3_odt_patterns: 0.into(),
            }
        }
    }
    impl LrdimmDdr4OdtPatElement {
        pub fn new(
            dimm_rank_bitmaps: LrdimmDdr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns: OdtPatPatterns,
            cs1_odt_patterns: OdtPatPatterns,
            cs2_odt_patterns: OdtPatPatterns,
            cs3_odt_patterns: OdtPatPatterns,
        ) -> Self {
            Self {
                dimm_rank_bitmaps: dimm_rank_bitmaps.to_u32().unwrap().into(),
                cs0_odt_patterns: cs0_odt_patterns.to_u32().unwrap().into(),
                cs1_odt_patterns: cs1_odt_patterns.to_u32().unwrap().into(),
                cs2_odt_patterns: cs2_odt_patterns.to_u32().unwrap().into(),
                cs3_odt_patterns: cs3_odt_patterns.to_u32().unwrap().into(),
            }
        }
    }

    impl EntryCompatible for LrdimmDdr4OdtPatElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4OdtPat) => true,
                // definitely not
                // EntryId::Memory(MemoryEntryId::PsRdimmDdr4OdtPat) => true,
                _ => false,
            }
        }
    }

    /*
        #[derive(BitfieldSpecifier, Debug, PartialEq)]
        #[bits = 1]
        pub enum DdrPostPackageRepairBodyDeviceWidth {
            Width0 = 0,
            Width30 = 0x1e,
            NotApplicable = 0x1f,
        }
    */

    make_bitfield_serde! {
        #[bitfield(bits = 64)]
        #[repr(u64)]
        #[derive(Default, Clone, Copy)]
        pub struct DdrPostPackageRepairBody {
            pub bank || SerdeHex8 : B5 | pub get u8 : pub set u8,
            pub rank_multiplier || SerdeHex8 : B3 | pub get u8 : pub set u8,
            pub xdevice_width || SerdeHex8 : B5 | pub get u8 : pub set u8, /* device width of DIMMs to repair; or 0x1F for
                                * heeding target_device instead */
            pub chip_select || SerdeHex8 : B2 | pub get u8 : pub set u8,
            pub column || SerdeHex16 : B10 | pub get u16 : pub set u16,
            pub hard_repair || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub valid || #[serde(default)] bool : bool | pub get bool : pub set bool,
            pub target_device || SerdeHex8 : B5 | pub get u8 : pub set u8,
            pub row || SerdeHex32 : B18 | pub get u32 : pub set u32,
            pub socket || SerdeHex8 : B3 | pub get u8 : pub set u8,
            pub channel || SerdeHex8 : B3 | pub get u8 : pub set u8,
            pub _reserved_1 || #[serde(default)] SerdeHex8 : B8,
        }
    }
    impl DdrPostPackageRepairBody {
        pub fn device_width(&self) -> Option<u8> {
            match self.xdevice_width() {
                0x1f => None,
                x => Some(x),
            }
        }
        pub fn set_device_width(&mut self, value: Option<u8>) {
            self.set_xdevice_width(value.unwrap_or(0x1f));
        }
        pub fn with_device_width(&mut self, value: Option<u8>) -> &mut Self {
            self.set_xdevice_width(value.unwrap_or(0x1f));
            self
        }
    }
    impl_bitfield_primitive_conversion!(
        DdrPostPackageRepairBody,
        0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111,
        u64
    );

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct DdrPostPackageRepairElement {
            body: [u8; 8], // no| pub get DdrPostPackageRepairBody : pub set DdrPostPackageRepairBody,
        }
    }

    #[cfg(feature = "serde")]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(
        feature = "serde",
        serde(rename = "DdrPostPackageRepairElement")
    )]
    pub struct CustomSerdeDdrPostPackageRepairElement {
        pub raw_body: DdrPostPackageRepairBody,
    }
    #[cfg(feature = "serde")]
    impl DdrPostPackageRepairElement {
        pub(crate) fn serde_raw_body(
            &self,
        ) -> Result<DdrPostPackageRepairBody> {
            Ok(DdrPostPackageRepairBody::from_bytes(self.body))
        }
        pub(crate) fn serde_with_raw_body(
            &mut self,
            value: DdrPostPackageRepairBody,
        ) -> &mut Self {
            self.body = value.into_bytes();
            self
        }
    }
    impl DdrPostPackageRepairElement {
        #[inline]
        pub fn body(&self) -> Option<DdrPostPackageRepairBody> {
            if self.body[3] & (1 << 2) != 0 {
                // valid @ ((3 * 8 + 2 == 26))
                Some(DdrPostPackageRepairBody::from_bytes(self.body))
            } else {
                None
            }
        }
        #[inline]
        pub fn invalid() -> DdrPostPackageRepairElement {
            Self { body: [0, 0, 0, 0, 0, 0, 0, 0xff] }
        }
        #[inline]
        pub fn set_body(&mut self, value: Option<DdrPostPackageRepairBody>) {
            match value {
                Some(body) => {
                    self.body = body.into_bytes();
                }
                None => {
                    self.body = Self::invalid().body;
                }
            }
        }
    }
    impl Default for DdrPostPackageRepairElement {
        fn default() -> Self {
            Self::invalid()
        }
    }

    impl EntryCompatible for DdrPostPackageRepairElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::DdrPostPackageRepair)
            )
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct DdrDqPinMapElementLane {
            pins: [u8; 8],
        }
    }

    impl DdrDqPinMapElementLane {
        pub fn new(pins: [u8; 8]) -> Self {
            Self { pins }
        }
    }

    impl Default for DdrDqPinMapElementLane {
        fn default() -> Self {
            Self { pins: [0, 1, 2, 3, 4, 5, 6, 7] }
        }
    }

    impl Getter<Result<[DdrDqPinMapElementLane; 8]>>
        for [DdrDqPinMapElementLane; 8]
    {
        fn get1(self) -> Result<[DdrDqPinMapElementLane; 8]> {
            Ok(self.map(|v| v))
        }
    }

    impl Setter<[DdrDqPinMapElementLane; 8]> for [DdrDqPinMapElementLane; 8] {
        fn set1(&mut self, value: [DdrDqPinMapElementLane; 8]) {
            *self = value.map(|v| v);
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct DdrDqPinMapElement {
            pub lanes: [DdrDqPinMapElementLane; 8], // lanes[lane][bit] == pin
        }
    }

    impl Default for DdrDqPinMapElement {
        fn default() -> Self {
            Self {
                lanes: [
                    DdrDqPinMapElementLane::new([0, 1, 2, 3, 4, 5, 6, 7]),
                    DdrDqPinMapElementLane::new([8, 9, 10, 11, 12, 13, 14, 15]),
                    DdrDqPinMapElementLane::new([
                        16, 17, 18, 19, 20, 21, 22, 23,
                    ]),
                    DdrDqPinMapElementLane::new([
                        24, 25, 26, 27, 28, 29, 30, 31,
                    ]),
                    DdrDqPinMapElementLane::new([0, 1, 2, 3, 4, 5, 6, 7]),
                    DdrDqPinMapElementLane::new([8, 9, 10, 11, 12, 13, 14, 15]),
                    DdrDqPinMapElementLane::new([
                        16, 17, 18, 19, 20, 21, 22, 23,
                    ]),
                    DdrDqPinMapElementLane::new([
                        24, 25, 26, 27, 28, 29, 30, 31,
                    ]),
                ],
            }
        }
    }

    impl EntryCompatible for DdrDqPinMapElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::DdrDqPinMap))
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5CaPinMapElementLane {
            pub pins: [u8; 14], // TODO (#124): nicer pin type
        }
    }

    impl Default for Ddr5CaPinMapElementLane {
        fn default() -> Self {
            Self { pins: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13] }
        }
    }

    impl Getter<Result<[Ddr5CaPinMapElementLane; 2]>>
        for [Ddr5CaPinMapElementLane; 2]
    {
        fn get1(self) -> Result<[Ddr5CaPinMapElementLane; 2]> {
            Ok(self.map(|v| v))
        }
    }

    impl Setter<[Ddr5CaPinMapElementLane; 2]> for [Ddr5CaPinMapElementLane; 2] {
        fn set1(&mut self, value: [Ddr5CaPinMapElementLane; 2]) {
            *self = value.map(|v| v);
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Default, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5CaPinMapElement {
            pub lanes: [Ddr5CaPinMapElementLane; 2], // pins[lane][bit] == pin; pin == 0xff means un?
        }
    }

    impl EntryCompatible for Ddr5CaPinMapElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Memory(MemoryEntryId::Ddr5CaPinMap))
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct PmuBistVendorAlgorithmElement {
            pub dram_manufacturer_id || u16 : LU16 | pub get u16 : pub set u16, // jedec id
            pub algorithm_bit_mask || u16 : LU16 | pub get u16 : pub set u16,
        }
    }

    impl HeaderWithTail for PmuBistVendorAlgorithmElement {
        type TailArrayItemType<'de> = ();
    }

    impl Default for PmuBistVendorAlgorithmElement {
        fn default() -> Self {
            Self {
                dram_manufacturer_id: 0.into(), // FIXME
                algorithm_bit_mask: 0.into(),   // FIXME
            }
        }
    }

    impl EntryCompatible for PmuBistVendorAlgorithmElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::PmuBistVendorAlgorithm)
            )
        }
    }

    make_accessors! {
        // FIXME default
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5RawCardConfigElementHeader32 {
            total_size || u32 : LU32,

            pub mem_clk || DdrSpeed : LU32 | pub get DdrSpeed : pub set DdrSpeed,
            pub dimm_type: u8, // bitmap rank type
            pub dev_width: u8, // bitmap of SDRAM IO width
            pub _reserved_1 || #[serde(default)] u8 : u8,
            pub _reserved_2 || #[serde(default)] u8 : u8,
            pub rcd_manufacturer_id || u32 : LU32, // JEDEC
            pub rcd_generation: u8,
            pub _reserved_3 || #[serde(default)] u8 : u8,
            pub _reserved_4 || #[serde(default)] u8 : u8,
            pub _reserved_5 || #[serde(default)] u8 : u8,
            pub raw_card_dev || u32 : LU32, // raw card revision
            pub dram_die_stepping_revision || u32 : LU32,
            pub dram_density: u8,
            pub _reserved_6 || #[serde(default)] u8 : u8,
            pub _reserved_7 || #[serde(default)] u8 : u8,
            pub _reserved_8 || #[serde(default)] u8 : u8,
        }
    }
    impl Getter<Result<Ddr5RawCardConfigElementHeader32>>
        for Ddr5RawCardConfigElementHeader32
    {
        fn get1(self) -> Result<Self> {
            Ok(self)
        }
    }

    impl Setter<Ddr5RawCardConfigElementHeader32>
        for Ddr5RawCardConfigElementHeader32
    {
        fn set1(&mut self, value: Self) {
            *self = value
        }
    }

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum Ddr5RawCardImpedance {
        Off = 0,

        #[cfg_attr(feature = "serde", serde(rename = "10 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "10 Ohm"))]
        _10Ohm = 10,

        #[cfg_attr(feature = "serde", serde(rename = "14 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "14 Ohm"))]
        _14Ohm = 14,

        #[cfg_attr(feature = "serde", serde(rename = "20 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "20 Ohm"))]
        _20Ohm = 20,

        #[cfg_attr(feature = "serde", serde(rename = "25 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "25 Ohm"))]
        _25Ohm = 25,

        #[cfg_attr(feature = "serde", serde(rename = "26 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "26 Ohm"))]
        _26Ohm = 26,

        #[cfg_attr(feature = "serde", serde(rename = "27 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "27 Ohm"))]
        _27Ohm = 27,

        #[cfg_attr(feature = "serde", serde(rename = "28 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "28 Ohm"))]
        _28Ohm = 28,

        #[cfg_attr(feature = "serde", serde(rename = "30 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "30 Ohm"))]
        _30Ohm = 30,

        #[cfg_attr(feature = "serde", serde(rename = "32 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "32 Ohm"))]
        _32Ohm = 32,

        #[cfg_attr(feature = "serde", serde(rename = "34 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "34 Ohm"))]
        _34Ohm = 34,

        #[cfg_attr(feature = "serde", serde(rename = "36 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "36 Ohm"))]
        _36Ohm = 36,

        #[cfg_attr(feature = "serde", serde(rename = "40 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "40 Ohm"))]
        _40Ohm = 40,

        #[cfg_attr(feature = "serde", serde(rename = "43 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "43 Ohm"))]
        _43Ohm = 43,

        #[cfg_attr(feature = "serde", serde(rename = "48 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "48 Ohm"))]
        _48Ohm = 48,

        #[cfg_attr(feature = "serde", serde(rename = "53 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "53 Ohm"))]
        _53Ohm = 53,

        #[cfg_attr(feature = "serde", serde(rename = "60 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "60 Ohm"))]
        _60Ohm = 60,

        #[cfg_attr(feature = "serde", serde(rename = "68 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "68 Ohm"))]
        _68Ohm = 68,

        #[cfg_attr(feature = "serde", serde(rename = "80 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "80 Ohm"))]
        _80Ohm = 80,

        #[cfg_attr(feature = "serde", serde(rename = "96 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "96 Ohm"))]
        _96Ohm = 96,

        #[cfg_attr(feature = "serde", serde(rename = "120 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "120 Ohm"))]
        _120Ohm = 120,

        #[cfg_attr(feature = "serde", serde(rename = "160 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "160 Ohm"))]
        _160Ohm = 160,

        #[cfg_attr(feature = "serde", serde(rename = "240 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "240 Ohm"))]
        _240Ohm = 240,

        #[cfg_attr(feature = "serde", serde(rename = "480 Ω"))]
        #[cfg_attr(feature = "serde", serde(alias = "480 Ohm"))]
        _480Ohm = 480,
    }

    // TODO: 14, 20, 27
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum Ddr5RawCardDriveStrength {
        Light = 0,
        Moderate = 1,
        Strong = 2,
    }

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum Ddr5RawCardSlew {
        Moderate = 0,
        Fast = 1,
        Slow = 2,
        Default = 255, // FIXME
    }

    // Note: From name to encoding: encoding = (97.5 - name)/0.5
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum Ddr5RawCardVref {
        #[cfg_attr(feature = "serde", serde(rename = "35.0%"))]
        _35_0P = 0x7d,

        #[cfg_attr(feature = "serde", serde(rename = "35.5%"))]
        _35_5P = 0x7c,

        #[cfg_attr(feature = "serde", serde(rename = "36.0%"))]
        _36_0P = 0x7b,

        #[cfg_attr(feature = "serde", serde(rename = "36.5%"))]
        _36_5P = 0x7a,

        #[cfg_attr(feature = "serde", serde(rename = "37.0%"))]
        _37_0P = 0x79,

        #[cfg_attr(feature = "serde", serde(rename = "37.5%"))]
        _37_5P = 0x78,

        #[cfg_attr(feature = "serde", serde(rename = "38.0%"))]
        _38_0P = 0x77,

        #[cfg_attr(feature = "serde", serde(rename = "38.5%"))]
        _38_5P = 0x76,

        #[cfg_attr(feature = "serde", serde(rename = "39.0%"))]
        _39_0P = 0x75,

        #[cfg_attr(feature = "serde", serde(rename = "39.5%"))]
        _39_5P = 0x74,

        #[cfg_attr(feature = "serde", serde(rename = "40.0%"))]
        _40_0P = 0x73,

        #[cfg_attr(feature = "serde", serde(rename = "40.5%"))]
        _40_5P = 0x72,

        #[cfg_attr(feature = "serde", serde(rename = "41.0%"))]
        _41_0P = 0x71,

        #[cfg_attr(feature = "serde", serde(rename = "41.5%"))]
        _41_5P = 0x70,

        #[cfg_attr(feature = "serde", serde(rename = "42.0%"))]
        _42_0P = 0x6f,

        #[cfg_attr(feature = "serde", serde(rename = "42.5%"))]
        _42_5P = 0x6e,

        #[cfg_attr(feature = "serde", serde(rename = "43.0%"))]
        _43_0P = 0x6d,

        #[cfg_attr(feature = "serde", serde(rename = "43.5%"))]
        _43_5P = 0x6c,

        #[cfg_attr(feature = "serde", serde(rename = "44.0%"))]
        _44_0P = 0x6b,

        #[cfg_attr(feature = "serde", serde(rename = "44.5%"))]
        _44_5P = 0x6a,

        #[cfg_attr(feature = "serde", serde(rename = "45.0%"))]
        _45_0P = 0x69,

        #[cfg_attr(feature = "serde", serde(rename = "45.5%"))]
        _45_5P = 0x68,

        #[cfg_attr(feature = "serde", serde(rename = "46.0%"))]
        _46_0P = 0x67,

        #[cfg_attr(feature = "serde", serde(rename = "46.5%"))]
        _46_5P = 0x66,

        #[cfg_attr(feature = "serde", serde(rename = "47.0%"))]
        _47_0P = 0x65,

        #[cfg_attr(feature = "serde", serde(rename = "47.5%"))]
        _47_5P = 0x64,

        #[cfg_attr(feature = "serde", serde(rename = "48.0%"))]
        _48_0P = 0x63,

        #[cfg_attr(feature = "serde", serde(rename = "48.5%"))]
        _48_5P = 0x62,

        #[cfg_attr(feature = "serde", serde(rename = "49.0%"))]
        _49_0P = 0x61,

        #[cfg_attr(feature = "serde", serde(rename = "49.5%"))]
        _49_5P = 0x60,

        #[cfg_attr(feature = "serde", serde(rename = "50.0%"))]
        _50_0P = 0x5f,

        #[cfg_attr(feature = "serde", serde(rename = "50.5%"))]
        _50_5P = 0x5e,

        #[cfg_attr(feature = "serde", serde(rename = "51.0%"))]
        _51_0P = 0x5d,

        #[cfg_attr(feature = "serde", serde(rename = "51.5%"))]
        _51_5P = 0x5c,

        #[cfg_attr(feature = "serde", serde(rename = "52.0%"))]
        _52_0P = 0x5b,

        #[cfg_attr(feature = "serde", serde(rename = "52.5%"))]
        _52_5P = 0x5a,

        #[cfg_attr(feature = "serde", serde(rename = "53.0%"))]
        _53_0P = 0x59,

        #[cfg_attr(feature = "serde", serde(rename = "53.5%"))]
        _53_5P = 0x58,

        #[cfg_attr(feature = "serde", serde(rename = "54.0%"))]
        _54_0P = 0x57,

        #[cfg_attr(feature = "serde", serde(rename = "54.5%"))]
        _54_5P = 0x56,

        #[cfg_attr(feature = "serde", serde(rename = "55.0%"))]
        _55_0P = 0x55,

        #[cfg_attr(feature = "serde", serde(rename = "55.5%"))]
        _55_5P = 0x54,

        #[cfg_attr(feature = "serde", serde(rename = "56.0%"))]
        _56_0P = 0x53,

        #[cfg_attr(feature = "serde", serde(rename = "56.5%"))]
        _56_5P = 0x52,

        #[cfg_attr(feature = "serde", serde(rename = "57.0%"))]
        _57_0P = 0x51,

        #[cfg_attr(feature = "serde", serde(rename = "57.5%"))]
        _57_5P = 0x50,

        #[cfg_attr(feature = "serde", serde(rename = "58.0%"))]
        _58_0P = 0x4f,

        #[cfg_attr(feature = "serde", serde(rename = "58.5%"))]
        _58_5P = 0x4e,

        #[cfg_attr(feature = "serde", serde(rename = "59.0%"))]
        _59_0P = 0x4d,

        #[cfg_attr(feature = "serde", serde(rename = "59.5%"))]
        _59_5P = 0x4c,

        #[cfg_attr(feature = "serde", serde(rename = "60.0%"))]
        _60_0P = 0x4b,

        #[cfg_attr(feature = "serde", serde(rename = "60.5%"))]
        _60_5P = 0x4a,

        #[cfg_attr(feature = "serde", serde(rename = "61.0%"))]
        _61_0P = 0x49,

        #[cfg_attr(feature = "serde", serde(rename = "61.5%"))]
        _61_5P = 0x48,

        #[cfg_attr(feature = "serde", serde(rename = "62.0%"))]
        _62_0P = 0x47,

        #[cfg_attr(feature = "serde", serde(rename = "62.5%"))]
        _62_5P = 0x46,

        #[cfg_attr(feature = "serde", serde(rename = "63.0%"))]
        _63_0P = 0x45,

        #[cfg_attr(feature = "serde", serde(rename = "63.5%"))]
        _63_5P = 0x44,

        #[cfg_attr(feature = "serde", serde(rename = "64.0%"))]
        _64_0P = 0x43,

        #[cfg_attr(feature = "serde", serde(rename = "64.5%"))]
        _64_5P = 0x42,

        #[cfg_attr(feature = "serde", serde(rename = "65.0%"))]
        _65_0P = 0x41,

        #[cfg_attr(feature = "serde", serde(rename = "65.5%"))]
        _65_5P = 0x40,

        #[cfg_attr(feature = "serde", serde(rename = "66.0%"))]
        _66_0P = 0x3f,

        #[cfg_attr(feature = "serde", serde(rename = "66.5%"))]
        _66_5P = 0x3e,

        #[cfg_attr(feature = "serde", serde(rename = "67.0%"))]
        _67_0P = 0x3d,

        #[cfg_attr(feature = "serde", serde(rename = "67.5%"))]
        _67_5P = 0x3c,

        #[cfg_attr(feature = "serde", serde(rename = "68.0%"))]
        _68_0P = 0x3b,

        #[cfg_attr(feature = "serde", serde(rename = "68.5%"))]
        _68_5P = 0x3a,

        #[cfg_attr(feature = "serde", serde(rename = "69.0%"))]
        _69_0P = 0x39,

        #[cfg_attr(feature = "serde", serde(rename = "69.5%"))]
        _69_5P = 0x38,

        #[cfg_attr(feature = "serde", serde(rename = "70.0%"))]
        _70_0P = 0x37,

        #[cfg_attr(feature = "serde", serde(rename = "70.5%"))]
        _70_5P = 0x36,

        #[cfg_attr(feature = "serde", serde(rename = "71.0%"))]
        _71_0P = 0x35,

        #[cfg_attr(feature = "serde", serde(rename = "71.5%"))]
        _71_5P = 0x34,

        #[cfg_attr(feature = "serde", serde(rename = "72.0%"))]
        _72_0P = 0x33,

        #[cfg_attr(feature = "serde", serde(rename = "72.5%"))]
        _72_5P = 0x32,

        #[cfg_attr(feature = "serde", serde(rename = "73.0%"))]
        _73_0P = 0x31,

        #[cfg_attr(feature = "serde", serde(rename = "73.5%"))]
        _73_5P = 0x30,

        #[cfg_attr(feature = "serde", serde(rename = "74.0%"))]
        _74_0P = 0x2f,

        #[cfg_attr(feature = "serde", serde(rename = "74.5%"))]
        _74_5P = 0x2e,

        #[cfg_attr(feature = "serde", serde(rename = "75.0%"))]
        _75_0P = 0x2d,

        #[cfg_attr(feature = "serde", serde(rename = "75.5%"))]
        _75_5P = 0x2c,

        #[cfg_attr(feature = "serde", serde(rename = "76.0%"))]
        _76_0P = 0x2b,

        #[cfg_attr(feature = "serde", serde(rename = "76.5%"))]
        _76_5P = 0x2a,

        #[cfg_attr(feature = "serde", serde(rename = "77.0%"))]
        _77_0P = 0x29,

        #[cfg_attr(feature = "serde", serde(rename = "77.5%"))]
        _77_5P = 0x28,

        #[cfg_attr(feature = "serde", serde(rename = "78.0%"))]
        _78_0P = 0x27,

        #[cfg_attr(feature = "serde", serde(rename = "78.5%"))]
        _78_5P = 0x26,

        #[cfg_attr(feature = "serde", serde(rename = "79.0%"))]
        _79_0P = 0x25,

        #[cfg_attr(feature = "serde", serde(rename = "79.5%"))]
        _79_5P = 0x24,

        #[cfg_attr(feature = "serde", serde(rename = "80.0%"))]
        _80_0P = 0x23,

        #[cfg_attr(feature = "serde", serde(rename = "80.5%"))]
        _80_5P = 0x22,

        #[cfg_attr(feature = "serde", serde(rename = "81.0%"))]
        _81_0P = 0x21,

        #[cfg_attr(feature = "serde", serde(rename = "81.5%"))]
        _81_5P = 0x20,

        #[cfg_attr(feature = "serde", serde(rename = "82.0%"))]
        _82_0P = 0x1f,

        #[cfg_attr(feature = "serde", serde(rename = "82.5%"))]
        _82_5P = 0x1e,

        #[cfg_attr(feature = "serde", serde(rename = "83.0%"))]
        _83_0P = 0x1d,

        #[cfg_attr(feature = "serde", serde(rename = "83.5%"))]
        _83_5P = 0x1c,

        #[cfg_attr(feature = "serde", serde(rename = "84.0%"))]
        _84_0P = 0x1b,

        #[cfg_attr(feature = "serde", serde(rename = "84.5%"))]
        _84_5P = 0x1a,

        #[cfg_attr(feature = "serde", serde(rename = "85.0%"))]
        _85_0P = 0x19,

        #[cfg_attr(feature = "serde", serde(rename = "85.5%"))]
        _85_5P = 0x18,

        #[cfg_attr(feature = "serde", serde(rename = "86.0%"))]
        _86_0P = 0x17,

        #[cfg_attr(feature = "serde", serde(rename = "86.5%"))]
        _86_5P = 0x16,

        #[cfg_attr(feature = "serde", serde(rename = "87.0%"))]
        _87_0P = 0x15,

        #[cfg_attr(feature = "serde", serde(rename = "87.5%"))]
        _87_5P = 0x14,

        #[cfg_attr(feature = "serde", serde(rename = "88.0%"))]
        _88_0P = 0x13,

        #[cfg_attr(feature = "serde", serde(rename = "88.5%"))]
        _88_5P = 0x12,

        #[cfg_attr(feature = "serde", serde(rename = "89.0%"))]
        _89_0P = 0x11,

        #[cfg_attr(feature = "serde", serde(rename = "89.5%"))]
        _89_5P = 0x10,

        #[cfg_attr(feature = "serde", serde(rename = "90.0%"))]
        _90_0P = 0x0f,

        #[cfg_attr(feature = "serde", serde(rename = "90.5%"))]
        _90_5P = 0x0e,

        #[cfg_attr(feature = "serde", serde(rename = "91.0%"))]
        _91_0P = 0x0d,

        #[cfg_attr(feature = "serde", serde(rename = "91.5%"))]
        _91_5P = 0x0c,

        #[cfg_attr(feature = "serde", serde(rename = "92.0%"))]
        _92_0P = 0x0b,

        #[cfg_attr(feature = "serde", serde(rename = "92.5%"))]
        _92_5P = 0x0a,

        #[cfg_attr(feature = "serde", serde(rename = "93.0%"))]
        _93_0P = 0x09,

        #[cfg_attr(feature = "serde", serde(rename = "93.5%"))]
        _93_5P = 0x08,

        #[cfg_attr(feature = "serde", serde(rename = "94.0%"))]
        _94_0P = 0x07,

        #[cfg_attr(feature = "serde", serde(rename = "94.5%"))]
        _94_5P = 0x06,

        #[cfg_attr(feature = "serde", serde(rename = "95.0%"))]
        _95_0P = 0x05,

        #[cfg_attr(feature = "serde", serde(rename = "95.5%"))]
        _95_5P = 0x04,

        #[cfg_attr(feature = "serde", serde(rename = "96.0%"))]
        _96_0P = 0x03,

        #[cfg_attr(feature = "serde", serde(rename = "96.5%"))]
        _96_5P = 0x02,

        #[cfg_attr(feature = "serde", serde(rename = "97.0%"))]
        _97_0P = 0x01,

        #[cfg_attr(feature = "serde", serde(rename = "97.5%"))]
        _97_5P = 0x00,
    }

    make_accessors! {
        // FIXME default
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5RawCardConfigElementPayload {
            total_size || u32 : LU32,

            pub qck_dev0 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev1 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev2 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev3 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev4 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev5 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev6 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev7 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev8 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_dev9 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qck_drive_strength || u16 : LU16,
            pub qck_slew || Ddr5RawCardSlew : LU16,

            pub qcs_dev0 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev1 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev2 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev3 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev4 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev5 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev6 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev7 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev8 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_dev9 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qcs_drive_strength || u16 : LU16,
            pub qcs_slew || Ddr5RawCardSlew : LU16,
            pub qcs_vref || Ddr5RawCardVref : LU16 | pub get Ddr5RawCardVref : pub set Ddr5RawCardVref,

            pub qca_dev0 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev1 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev2 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev3 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev4 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev5 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev6 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev7 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev8 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_dev9 || Ddr5RawCardImpedance : LU16 | pub get Ddr5RawCardImpedance : pub set Ddr5RawCardImpedance,
            pub qca_drive_strength || u16 : LU16,
            pub qca_slew || Ddr5RawCardSlew : LU16,
            pub qca_vref || Ddr5RawCardVref : LU16 | pub get Ddr5RawCardVref : pub set Ddr5RawCardVref,
        }
    }
    impl Ddr5RawCardConfigElementPayload {
        pub fn qck_drive_strength(
            &self,
        ) -> Result<Option<Ddr5RawCardDriveStrength>> {
            Ok(match self.qck_drive_strength.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardDriveStrength::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qck_drive_strength(
            &mut self,
            value: Option<Ddr5RawCardDriveStrength>,
        ) {
            self.qck_drive_strength.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }
        pub fn qck_slew(&self) -> Result<Option<Ddr5RawCardSlew>> {
            Ok(match self.qck_slew.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardSlew::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qck_slew(&mut self, value: Option<Ddr5RawCardSlew>) {
            self.qck_slew.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }
        pub fn qcs_drive_strength(
            &self,
        ) -> Result<Option<Ddr5RawCardDriveStrength>> {
            Ok(match self.qcs_drive_strength.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardDriveStrength::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qcs_drive_strength(
            &mut self,
            value: Option<Ddr5RawCardDriveStrength>,
        ) {
            self.qcs_drive_strength.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }
        pub fn qcs_slew(&self) -> Result<Option<Ddr5RawCardSlew>> {
            Ok(match self.qcs_slew.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardSlew::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qcs_slew(&mut self, value: Option<Ddr5RawCardSlew>) {
            self.qcs_slew.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }

        pub fn qca_drive_strength(
            &self,
        ) -> Result<Option<Ddr5RawCardDriveStrength>> {
            Ok(match self.qca_drive_strength.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardDriveStrength::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qca_drive_strength(
            &mut self,
            value: Option<Ddr5RawCardDriveStrength>,
        ) {
            self.qca_drive_strength.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }
        pub fn qca_slew(&self) -> Result<Option<Ddr5RawCardSlew>> {
            Ok(match self.qca_slew.get() {
                0xff => None,
                x => Some(
                    Ddr5RawCardSlew::from_u16(x)
                        .ok_or(Error::EntryTypeMismatch)?,
                ),
            })
        }
        pub fn set_qca_slew(&mut self, value: Option<Ddr5RawCardSlew>) {
            self.qca_slew.set(match value {
                None => 0xff,
                Some(x) => x.to_u16().unwrap(),
            })
        }
    }
    impl Getter<Result<Ddr5RawCardConfigElementPayload>>
        for Ddr5RawCardConfigElementPayload
    {
        fn get1(self) -> Result<Self> {
            Ok(self)
        }
    }

    impl Setter<Ddr5RawCardConfigElementPayload>
        for Ddr5RawCardConfigElementPayload
    {
        fn set1(&mut self, value: Self) {
            *self = value
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Default, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5RawCardConfigElement {
            header: Ddr5RawCardConfigElementHeader32,
            payload: Ddr5RawCardConfigElementPayload,
        }
    }

    impl EntryCompatible for Ddr5RawCardConfigElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::Ddr5RawCardConfig)
            )
        }
    }

    make_bitfield_serde!(
        #[bitfield(bits = 12)]
        #[derive(
            Default, Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct Ddr5TrainingOverrideEntryHeaderChannelSelectorMask {
            pub c_0: bool | pub get bool : pub set bool,
            pub c_1: bool | pub get bool : pub set bool,
            pub c_2: bool | pub get bool : pub set bool,
            pub c_3: bool | pub get bool : pub set bool,
            pub c_4: bool | pub get bool : pub set bool,
            pub c_5: bool | pub get bool : pub set bool,
            pub c_6: bool | pub get bool : pub set bool,
            pub c_7: bool | pub get bool : pub set bool,
            pub c_8: bool | pub get bool : pub set bool,
            pub c_9: bool | pub get bool : pub set bool,
            pub c_10: bool | pub get bool : pub set bool,
            pub c_11: bool | pub get bool : pub set bool,
        }
    );

    make_bitfield_serde!(
        #[bitfield(bits = 4)]
        #[derive(
            Default, Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask {
            pub ddr4800: bool | pub get bool : pub set bool,
            pub ddr5600: bool | pub get bool : pub set bool,
            pub ddr6000: bool | pub get bool : pub set bool,
            pub ddr6400: bool | pub get bool : pub set bool,
        }
    );

    // Note: Using make_bitfield_serde would expose the fact that modular-bitfield doesn't actually have i8 (or i4).
    #[bitfield(bits = 32)]
    #[derive(Default, Clone, Copy, PartialEq, BitfieldSpecifier)]
    pub(crate) struct Ddr5TrainingOverrideEntryHeaderFlags {
        pub selected_mem_clks: Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask,
        pub selected_channels:
            Ddr5TrainingOverrideEntryHeaderChannelSelectorMask,
        pub read_dq_delay_offset: B4,  // actually i4
        pub read_dq_vref_offset: B4,   // actually i4
        pub write_dq_delay_offset: B4, // actually i4
        pub write_dq_vref_offset: B4,  // actually i4
    }
    impl DummyErrorChecks for Ddr5TrainingOverrideEntryHeaderFlags {}

    /// modular-bitfield can't represent i4 (or for that matter, i8).
    /// So it uses u8 for that.
    /// So we have to manually convert it here.
    fn sign_extend_i4_to_i8(x: u8) -> i8 {
        let sign_mask = x & 0b1000;
        if sign_mask == 0 { x as i8 } else { (x | 0xF0) as i8 }
    }

    /// modular-bitfield can't represent i4 (or for that matter, i8).
    /// So it uses u8 for that.
    /// So we have to manually convert it here.
    fn i8_to_i4(value: i8) -> u8 {
        (value as u8) & 0b1111
    }

    impl From<Ddr5TrainingOverrideEntryHeaderFlags> for u32 {
        fn from(source: Ddr5TrainingOverrideEntryHeaderFlags) -> u32 {
            let bytes = source.into_bytes();
            u32::from_le_bytes(bytes)
        }
    }

    impl From<u32> for Ddr5TrainingOverrideEntryHeaderFlags {
        fn from(source: u32) -> Ddr5TrainingOverrideEntryHeaderFlags {
            Self::from_bytes(source.to_le_bytes())
        }
    }

    impl Ddr5TrainingOverrideEntryHeaderFlags {
        #[allow(dead_code)]
        pub fn builder() -> Self {
            Self::new()
        }
        #[allow(dead_code)]
        pub fn build(&self) -> Self {
            *self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_selected_mem_clks(
            &self,
        ) -> Result<Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask> {
            Ok(self.selected_mem_clks())
        }
        #[allow(dead_code)]
        pub(crate) fn serde_selected_channels(
            &self,
        ) -> Result<Ddr5TrainingOverrideEntryHeaderChannelSelectorMask>
        {
            Ok(self.selected_channels())
        }
        #[allow(dead_code)]
        pub(crate) fn serde_read_dq_delay_offset(&self) -> Result<i8> {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            Ok(sign_extend_i4_to_i8(self.read_dq_delay_offset()))
        }
        #[allow(dead_code)]
        pub(crate) fn serde_read_dq_vref_offset(&self) -> Result<i8> {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            Ok(sign_extend_i4_to_i8(self.read_dq_vref_offset()))
        }
        #[allow(dead_code)]
        pub(crate) fn serde_write_dq_delay_offset(&self) -> Result<i8> {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            Ok(sign_extend_i4_to_i8(self.write_dq_delay_offset()))
        }
        #[allow(dead_code)]
        pub(crate) fn serde_write_dq_vref_offset(&self) -> Result<i8> {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            Ok(sign_extend_i4_to_i8(self.write_dq_vref_offset()))
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_selected_mem_clks(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask,
        ) -> &mut Self {
            self.set_selected_mem_clks(value);
            self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_selected_channels(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderChannelSelectorMask,
        ) -> &mut Self {
            self.set_selected_channels(value);
            self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_read_dq_delay_offset(
            &mut self,
            value: i8,
        ) -> &mut Self {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            self.set_read_dq_delay_offset(i8_to_i4(value));
            self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_read_dq_vref_offset(
            &mut self,
            value: i8,
        ) -> &mut Self {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            self.set_read_dq_vref_offset(i8_to_i4(value));
            self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_write_dq_delay_offset(
            &mut self,
            value: i8,
        ) -> &mut Self {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            self.set_write_dq_delay_offset(i8_to_i4(value));
            self
        }
        #[allow(dead_code)]
        pub(crate) fn serde_with_write_dq_vref_offset(
            &mut self,
            value: i8,
        ) -> &mut Self {
            // modular-bitfield can't represent i4 (or for that matter, i8).
            // So it uses u8 for that.
            // So we have to manually convert it here.
            self.set_write_dq_vref_offset(i8_to_i4(value));
            self
        }
    }

    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[cfg_attr(
        feature = "serde",
        serde(rename = "Ddr5TrainingOverrideEntryHeaderFlags")
    )]
    pub struct CustomSerdeDdr5TrainingOverrideEntryHeaderFlags {
        pub selected_mem_clks: Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask,
        pub selected_channels:
            Ddr5TrainingOverrideEntryHeaderChannelSelectorMask,
        #[cfg_attr(feature = "serde", serde(default))]
        pub read_dq_delay_offset: i8,
        #[cfg_attr(feature = "serde", serde(default))]
        pub read_dq_vref_offset: i8,
        #[cfg_attr(feature = "serde", serde(default))]
        pub write_dq_delay_offset: i8,
        #[cfg_attr(feature = "serde", serde(default))]
        pub write_dq_vref_offset: i8,
    }

    impl_bitfield_primitive_conversion!(
        Ddr5TrainingOverrideEntryHeaderFlags,
        0b1111_1111_1111_1111_1111_1111_1111_1111,
        u32
    );

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr5TrainingOverride40Element {
            pub length || u32 : LU32,
            /// SPD byte 521-550
            pub dimm_module_part_number: [u8; 31] | pub get [u8; 31] : pub set [u8; 31], // TODO: extra type?
            pub _reserved_1 || #[serde(default)] u8 : u8, // for alignment
            flags || #[serde(flatten)] Ddr5TrainingOverrideEntryHeaderFlags : LU32 | pub(crate) get Ddr5TrainingOverrideEntryHeaderFlags : pub(crate) set Ddr5TrainingOverrideEntryHeaderFlags,
        }
    }

    impl Ddr5TrainingOverride40Element {
        pub fn selected_mem_clks(
            &self,
        ) -> Result<Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask> {
            Ok(self.flags()?.selected_mem_clks())
        }
        pub fn set_selected_mem_clks(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask,
        ) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            flags.set_selected_mem_clks(value);
            self.set_flags(flags)
        }
        pub fn with_selected_mem_clks(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask,
        ) -> &mut Self {
            self.set_selected_mem_clks(value);
            self
        }
        pub fn selected_channels(
            &self,
        ) -> Result<Ddr5TrainingOverrideEntryHeaderChannelSelectorMask>
        {
            Ok(self.flags()?.selected_channels())
        }
        pub fn set_selected_channels(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderChannelSelectorMask,
        ) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            flags.set_selected_channels(value);
            self.set_flags(flags)
        }
        pub fn with_selected_channels(
            &mut self,
            value: Ddr5TrainingOverrideEntryHeaderChannelSelectorMask,
        ) -> &mut Self {
            self.set_selected_channels(value);
            self
        }
        pub fn read_dq_delay_offset(&self) -> Result<i8> {
            // Because u4 doesn't exist in modular-bitfield, it returns an u8.
            // We need an i8 (or really, an i4).
            let raw_value: u8 = self.flags()?.read_dq_delay_offset();
            Ok(sign_extend_i4_to_i8(raw_value))
        }
        pub fn set_read_dq_delay_offset(&mut self, value: i8) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            // Because modular-bitfield doesn't have i4.
            flags.set_read_dq_delay_offset(i8_to_i4(value));
            self.set_flags(flags)
        }
        pub fn with_read_dq_delay_offset(&mut self, value: i8) -> &mut Self {
            self.set_read_dq_delay_offset(value);
            self
        }
        pub fn read_dq_vref_offset(&self) -> Result<i8> {
            // Because u4 doesn't exist in modular-bitfield, it returns an u8.
            // We need an i8 (or really, an i4).
            let raw_value: u8 = self.flags()?.read_dq_vref_offset();
            Ok(sign_extend_i4_to_i8(raw_value))
        }
        pub fn set_read_dq_vref_offset(&mut self, value: i8) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            // Because modular-bitfield doesn't have i4.
            flags.set_read_dq_vref_offset(i8_to_i4(value));
            self.set_flags(flags)
        }
        pub fn with_read_dq_vref_offset(&mut self, value: i8) -> &mut Self {
            self.set_read_dq_vref_offset(value);
            self
        }
        pub fn write_dq_delay_offset(&self) -> Result<i8> {
            // Because u4 doesn't exist in modular-bitfield, it returns an u8.
            // We need an i8 (or really, an i4).
            let raw_value: u8 = self.flags()?.write_dq_delay_offset();
            Ok(sign_extend_i4_to_i8(raw_value))
        }
        pub fn set_write_dq_delay_offset(&mut self, value: i8) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            // Cast because modular-bitfield doesn't have i4.
            flags.set_write_dq_delay_offset(i8_to_i4(value));
            self.set_flags(flags)
        }
        pub fn with_write_dq_delay_offset(&mut self, value: i8) -> &mut Self {
            self.set_write_dq_delay_offset(value);
            self
        }
        pub fn write_dq_vref_offset(&self) -> Result<i8> {
            // Because u4 doesn't exist in modular-bitfield, it returns an u8.
            // We need an i8 (or really, an i4).
            let raw_value: u8 = self.flags()?.write_dq_vref_offset();
            Ok(sign_extend_i4_to_i8(raw_value))
        }
        pub fn set_write_dq_vref_offset(&mut self, value: i8) {
            let mut flags =
                Ddr5TrainingOverrideEntryHeaderFlags::from(self.flags.get());
            // Cast because modular-bitfield doesn't have i4.
            flags.set_write_dq_vref_offset(i8_to_i4(value));
            self.set_flags(flags)
        }
        pub fn with_write_dq_vref_offset(&mut self, value: i8) -> &mut Self {
            self.set_write_dq_vref_offset(value);
            self
        }
    }

    impl EntryCompatible for Ddr5TrainingOverride40Element {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::Ddr5TrainingOverride)
            )
        }
    }

    impl HeaderWithTail for Ddr5TrainingOverride40Element {
        type TailArrayItemType<'de> = ();
    }

    pub mod platform_specific_override {
        use super::{EntryId, Error, MemoryEntryId};
        crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum! {
                        // See AMD #44065

                        use core::mem::size_of;
                        use static_assertions::const_assert;
                        use zerocopy::{IntoBytes, FromBytes, Unaligned};
                        use super::super::*;
                        use crate::struct_accessors::{Getter, Setter, make_accessors};
                        use crate::types::Result;

                        make_bitfield_serde! {
                            #[bitfield(filled = true, bits = 8)]
                            #[repr(u8)]
                            #[derive(Clone, Copy, PartialEq)]
                            pub struct ChannelIdsSelection {
                                pub a || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub b || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub c || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub d || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub e || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub f || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub g || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub h || #[serde(default)] bool : bool | pub get bool : pub set bool,
                            }
                        }
                        impl_bitfield_primitive_conversion!(ChannelIdsSelection, 0b1111_1111, u8);

                        #[derive(PartialEq)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum ChannelIds {
                            Any, // 0xff
                            Specific(ChannelIdsSelection),
                        }

                        impl FromPrimitive for ChannelIds {
                            #[inline]
                            fn from_u64(raw_value: u64) -> Option<Self> {
                                match raw_value.cmp(&0xff) {
                                    Ordering::Equal => Some(Self::Any),
                                    Ordering::Less => Some(Self::Specific(ChannelIdsSelection::from_u8(raw_value as u8)?)),
                                    _ => None,
                                }
                            }
                            #[inline]
                            fn from_i64(raw_value: i64) -> Option<Self> {
                                if raw_value >= 0 {
                                    Self::from_u64(raw_value as u64)
                                } else {
                                    None
                                }
                            }
                        }

                        impl ToPrimitive for ChannelIds {
                            #[inline]
                            fn to_i64(&self) -> Option<i64> {
                                match self {
                                    Self::Any => Some(0xff),
                                    Self::Specific(ids) => {
                                        let value = ids.to_i64()?;
                                        assert!(value != 0xff);
                                        Some(value)
                                    },
                                }
                            }
                            #[inline]
                            fn to_u64(&self) -> Option<u64> {
                                match self {
                                    Self::Any => Some(0xff),
                                    Self::Specific(ids) => {
                                        let value = ids.to_u64()?;
                                        assert!(value != 0xff);
                                        Some(value)
                                    },
                                }
                            }
                        }

                        make_bitfield_serde! {
                            #[bitfield(filled = true, bits = 8)]
                            #[repr(u8)]
                            #[derive(Clone, Copy, PartialEq)]
                            pub struct SocketIds {
                                pub socket_0 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_1 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_2 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_3 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_4 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_5 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_6 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub socket_7 || #[serde(default)] bool : bool | pub get bool : pub set bool,
                            }
                        }
                        impl_bitfield_primitive_conversion!(SocketIds, 0b1111_1111, u8);
                        impl SocketIds {
                            pub const ALL: Self = Self::from_bytes([0xff]);
                        }

                        make_bitfield_serde! {
                            #[bitfield(bits = 8)]
                            #[repr(u8)]
                            #[derive(Clone, Copy)]
                            pub struct DimmSlotsSelection {
                                pub dimm_slot_0 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @0
                                pub dimm_slot_1 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @1
                                pub dimm_slot_2 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @2
                                pub dimm_slot_3 || #[serde(default)] bool : bool | pub get bool : pub set bool, // @3
                                pub _reserved_1 || #[serde(default)] SerdeHex8 : B4,
                            }
                        }
                        impl_bitfield_primitive_conversion!(DimmSlotsSelection, 0b1111, u8);
                        #[derive(Clone, Copy)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum DimmSlots {
                            Any, // 0xff
                            Specific(DimmSlotsSelection),
                        }

                        impl FromPrimitive for DimmSlots {
                            #[inline]
                            fn from_u64(raw_value: u64) -> Option<Self> {
                                if raw_value == 0xff { // valid
                                    Some(Self::Any)
                                } else if raw_value < 16 { // valid
                                    Some(Self::Specific(DimmSlotsSelection::from_u8(raw_value as u8)?))
                                } else {
                                    None
                                }
                            }
                            #[inline]
                            fn from_i64(raw_value: i64) -> Option<Self> {
                                if raw_value >= 0 {
                                    Self::from_u64(raw_value as u64)
                                } else {
                                    None
                                }
                            }
                        }

                        impl ToPrimitive for DimmSlots {
                            #[inline]
                            fn to_i64(&self) -> Option<i64> {
                                match self {
                                    Self::Any => Some(0xff),
                                    Self::Specific(value) => {
                                        let value = value.to_i64()?;
                                        assert!(value != 0xff);
                                        Some(value)
                                    },
                                }
                            }
                            #[inline]
                            fn to_u64(&self) -> Option<u64> {
                                match self {
                                    Self::Any => Some(0xff),
                                    Self::Specific(value) => {
                                        let value = value.to_u64()?;
                                        assert!(value != 0xff);
                                        Some(value)
                                    },
                                }
                            }
                        }

                        macro_rules! impl_EntryCompatible {($struct_:ty, $type_:expr, $payload_size:expr) => (
                            const_assert!($payload_size as usize + 2usize == size_of::<$struct_>());

                            impl $struct_ {
                                const TAG: u16 = $type_;
                                #[allow(dead_code)]
                                fn serde_default_tag() -> SerdeHex8 {
                                    (Self::TAG as u8).into()
                                }
                                #[allow(dead_code)]
                                fn serde_default_payload_size() -> SerdeHex8 {
                                    $payload_size.try_into().unwrap()
                                }
                            }

                            impl EntryCompatible for $struct_ {
                                fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
                                    match entry_id {
                                        EntryId::Memory(MemoryEntryId::PlatformSpecificOverride) => {
                                            prefix.len() >= 2 && prefix[0] == $type_ && prefix[1] as usize + 2usize == size_of::<Self>()
                                        },
                                        _ => false,
                                    }
                                }
                                fn skip_step(entry_id: EntryId, prefix: &[u8]) -> Option<(u16, usize)> {
                                    match entry_id {
                                        EntryId::Memory(MemoryEntryId::PlatformSpecificOverride) => {
                                            if prefix.len() >= 2 {
                                                let type_ = prefix[0] as u16;
                                                let size = (prefix[1] as usize).checked_add(2)?;
                                                Some((type_, size))
                                            } else {
                                                None
                                            }
                                        },
                                        _ => {
                                            None
                                        }
                                    }
                                }
                            }

                            impl SequenceElementAsBytes for $struct_ {
                                fn checked_as_bytes(&self, entry_id: EntryId) -> Option<&[u8]> {
                                    let blob = IntoBytes::as_bytes(self);
                                    if <$struct_>::is_entry_compatible(entry_id, blob) {
                                        Some(blob)
                                    } else {
                                        None
                                    }
                                }
                            }
                            impl ElementAsBytes for $struct_ {
                                fn element_as_bytes(&self) -> &[u8] {
                                    IntoBytes::as_bytes(self)
                                }
                            }

                        //            impl HeaderWithTail for $struct_ {
                        //                type TailArrayItemType = ();
                        //            }
                        )}

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Clone)]
                            #[repr(C, packed)]
                            pub struct CkeTristateMap {
                                type_ || #[serde(default = "CkeTristateMap::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "CkeTristateMap::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package's clock enable (CKE) pin, value = memory rank's CKE pin mask
                                pub connections || [SerdeHex8; 4] : [u8; 4],
                            }
                        }
                        impl_EntryCompatible!(CkeTristateMap, 1, 7);
                        impl Default for CkeTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    connections: [0; 4], // probably invalid
                                }
                            }
                        }
                        impl CkeTristateMap {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, connections: [u8; 4]) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    connections,
                                    .. Self::default()
                                })
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct OdtTristateMap {
                                type_ || #[serde(default = "OdtTristateMap::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "OdtTristateMap::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package's ODT pin (MA_ODT\[i\]), value = memory rank's ODT pin mask
                                pub connections || [SerdeHex8; 4] : [u8; 4],
                            }
                        }
                        impl_EntryCompatible!(OdtTristateMap, 2, 7);
                        impl Default for OdtTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    connections: [0; 4], // probably invalid
                                }
                            }
                        }
                        impl OdtTristateMap {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, connections: [u8; 4]) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    connections,
                                    .. Self::default()
                                })
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct CsTristateMap {
                                type_ || #[serde(default = "CsTristateMap::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "CsTristateMap::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package CS pin (MA_CS_L\[i\]), value = memory rank's CS pin
                                pub connections || [SerdeHex8; 8] : [u8; 8],
                            }
                        }
                        impl_EntryCompatible!(CsTristateMap, 3, 11);
                        impl Default for CsTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    connections: [0; 8], // probably invalid
                                }
                            }
                        }
                        impl CsTristateMap {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, connections: [u8; 8]) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    connections,
                                    .. Self::default()
                                })
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxDimmsPerChannel {
                                type_ || #[serde(default = "MaxDimmsPerChannel::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MaxDimmsPerChannel::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "any"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(MaxDimmsPerChannel, 4, 4);
                        impl Default for MaxDimmsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 2,
                                }
                            }
                        }
                        impl MaxDimmsPerChannel {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, value: u8) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value,
                                    .. Self::default()
                                })
                            }
                        }
                        make_bitfield_serde! {
                            #[bitfield(filled = true, bits = 16)]
                            #[repr(u16)]
                            #[derive(Clone, Copy, PartialEq)]
                            pub struct ChannelIdsSelection12 {
                                pub a || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub b || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub c || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub d || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub e || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub f || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub g || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub h || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub i || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub j || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub k || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub l || #[serde(default)] bool : bool | pub get bool : pub set bool,
                                pub _reserved_1 || #[serde(default)] SerdeHex8 : B4,
                            }
                        }
                        impl_bitfield_primitive_conversion!(ChannelIdsSelection12, 0b1111_1111_1111, u16);

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxDimmsPerChannel6 {
                                type_ || #[serde(default = "MaxDimmsPerChannel6::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MaxDimmsPerChannel6::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIdsSelection12 : LU16 | pub get ChannelIdsSelection12 : pub set ChannelIdsSelection12,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "any"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8,
                                _padding_0 || #[serde(default)] SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(MaxDimmsPerChannel6, 4, 6);
                        impl Default for MaxDimmsPerChannel6 {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u16().unwrap().into(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 2,
                                    _padding_0: 0,
                                }
                            }
                        }
                        impl MaxDimmsPerChannel6 {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, value: u8) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u16().unwrap().into(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value,
                                    .. Self::default()
                                })
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemclkMap {
                                type_ || #[serde(default = "MemclkMap::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MemclkMap::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "all"
                                pub connections || [SerdeHex8; 8] : [u8; 8],
                            }
                        }
                        impl_EntryCompatible!(MemclkMap, 7, 11);
                        impl Default for MemclkMap {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    connections: [0; 8], // all disabled
                                }
                            }
                        }
                        impl MemclkMap {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, connections: [u8; 8]) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    connections,
                                    ..Self::default()
                                })
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxChannelsPerSocket {
                                type_ || #[serde(default = "MaxChannelsPerSocket::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MaxChannelsPerSocket::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,  // Note: must always be "any"
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "any" here
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(MaxChannelsPerSocket, 8, 4);
                        impl Default for MaxChannelsPerSocket {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 2,
                                }
                            }
                        }
                        impl MaxChannelsPerSocket {
                            pub fn new(sockets: SocketIds, value: u8) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value,
                                    ..Self::default()
                                })
                            }
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum TimingMode {
                            Auto = 0,
                            Limit = 1,
                            Specific = 2,
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum MemBusSpeedType { // in MHz
                            Ddr400 = 200,
                            Ddr533 = 266,
                            Ddr667 = 333,
                            Ddr800 = 400,
                            Ddr1066 = 533,
                            Ddr1333 = 667,
                            Ddr1600 = 800,
                            Ddr1866 = 933,
                            Ddr2100 = 1050,
                            Ddr2133 = 1067,
                            Ddr2400 = 1200,
                            Ddr2667 = 1333,
                            Ddr2800 = 1400,
                            Ddr2933 = 1467,
                            Ddr3066 = 1533,
                            Ddr3200 = 1600,
                            Ddr3333 = 1667,
                            Ddr3466 = 1733,
                            Ddr3600 = 1800,
                            Ddr3733 = 1867,
                            Ddr3866 = 1933,
                            Ddr4000 = 2000,
                            Ddr4200 = 2100,
                            Ddr4267 = 2133,
                            Ddr4333 = 2167,
                            Ddr4400 = 2200,
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemBusSpeed {
                                type_ || #[serde(default = "MemBusSpeed::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MemBusSpeed::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "all"
                                timing_mode || TimingMode : LU32 | pub get TimingMode : pub set TimingMode,
                                bus_speed || MemBusSpeedType : LU32 | pub get MemBusSpeedType : pub set MemBusSpeedType,
                            }
                        }
                        impl_EntryCompatible!(MemBusSpeed, 9, 11);
                        impl Default for MemBusSpeed {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    timing_mode: TimingMode::Auto.to_u32().unwrap().into(),
                                    bus_speed: MemBusSpeedType::Ddr1600.to_u32().unwrap().into(), // User probably wants to change this
                                }
                            }
                        }
                        impl MemBusSpeed {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, timing_mode: TimingMode, bus_speed: MemBusSpeedType) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    timing_mode: (timing_mode as u32).into(),
                                    bus_speed: (bus_speed as u32).into(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            /// Max. Chip Selects per channel
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxCsPerChannel {
                                type_ || #[serde(default = "MaxCsPerChannel::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MaxCsPerChannel::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "Any"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(MaxCsPerChannel, 10, 4);
                        impl Default for MaxCsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 0, // probably invalid
                                }
                            }
                        }
                        impl MaxCsPerChannel {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, value: u8) -> Result<Self> {
                                Ok(Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms:  DimmSlots::Any.to_u8().unwrap(),
                                    value,
                                    ..Self::default()
                                })
                            }
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy,
                Clone)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum MemTechnologyType {
                            Ddr2 = 0,
                            Ddr3 = 1,
                            Gddr5 = 2,
                            Ddr4 = 3,
                            Lpddr3 = 4,
                            Lpddr4 = 5,
                            Hbm = 6,
                            Gddr6 = 7,
                            Ddr5 = 8,
                            Lpddr5 = 9,
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemTechnology {
                                type_ || #[serde(default = "MemTechnology::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MemTechnology::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds, // Note: must always be "any" here
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: must always be "any" here
                                technology_type || MemTechnologyType : LU32 | pub get MemTechnologyType : pub set MemTechnologyType,
                            }
                        }
                        impl_EntryCompatible!(MemTechnology, 11, 7);
                        impl Default for MemTechnology {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    technology_type: 0.into(), // probably invalid
                                }
                            }
                        }
                        impl MemTechnology {
                            pub fn new(sockets: SocketIds, technology_type: MemTechnologyType) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    technology_type: (technology_type as u32).into(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct WriteLevellingSeedDelay {
                                type_ || #[serde(default = "WriteLevellingSeedDelay::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "WriteLevellingSeedDelay::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                seed || [SerdeHex8; 8] : [u8; 8],
                                ecc_seed || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(WriteLevellingSeedDelay, 12, 12);
                        impl Default for WriteLevellingSeedDelay {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    seed: [0; 8], // probably invalid
                                    ecc_seed: 0, // probably invalid
                                }
                            }
                        }
                        impl WriteLevellingSeedDelay {
                            // TODO: Add fn new.
                        }

                        make_accessors! {
                            /// See <https://www.amd.com/system/files/TechDocs/43170_14h_Mod_00h-0Fh_BKDG.pdf> section 2.9.3.7.2.1
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct RxEnSeed {
                                type_ || #[serde(default = "RxEnSeed::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "RxEnSeed::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                seed || [SerdeHex16; 8] : [LU16; 8],
                                ecc_seed || SerdeHex16 : LU16 | pub get u16 : pub set u16,
                            }
                        }
                        impl_EntryCompatible!(RxEnSeed, 13, 21);
                        impl Default for RxEnSeed {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    seed: [0.into(); 8], // probably invalid
                                    ecc_seed: 0.into(), // probably invalid
                                }
                            }
                        }
                        impl RxEnSeed {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, seed: [u16; 8], ecc_seed: u16) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    seed: [seed[0].into(), seed[1].into(), seed[2].into(), seed[3].into(), seed[4].into(), seed[5].into(), seed[6].into(), seed[7].into()],
                                    ecc_seed: ecc_seed.into(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct LrDimmNoCs6Cs7Routing {
                                type_ || #[serde(default = "LrDimmNoCs6Cs7Routing::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "LrDimmNoCs6Cs7Routing::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots,
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(LrDimmNoCs6Cs7Routing, 14, 4);
                        impl Default for LrDimmNoCs6Cs7Routing {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 1,
                                }
                            }
                        }
                        impl LrDimmNoCs6Cs7Routing {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct SolderedDownSodimm {
                                type_ || #[serde(default = "SolderedDownSodimm::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "SolderedDownSodimm::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(SolderedDownSodimm, 15, 4);
                        impl Default for SolderedDownSodimm {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 1,
                                }
                            }
                        }
                        impl SolderedDownSodimm {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct LvDimmForce1V5 {
                                type_ || #[serde(default = "LvDimmForce1V5::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "LvDimmForce1V5::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(LvDimmForce1V5, 16, 4);
                        impl Default for LvDimmForce1V5 {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 1,
                                }
                            }
                        }
                        impl LvDimmForce1V5 {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MinimumRwDataEyeWidth {
                                type_ || #[serde(default = "MinimumRwDataEyeWidth::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MinimumRwDataEyeWidth::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                min_read_data_eye_width || SerdeHex8 : u8 | pub get u8 : pub set u8,
                                min_write_data_eye_width || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(MinimumRwDataEyeWidth, 17, 5);
                        impl Default for MinimumRwDataEyeWidth {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    min_read_data_eye_width: 0, // probably invalid
                                    min_write_data_eye_width: 0, // probably invalid
                                }
                            }
                        }
                        impl MinimumRwDataEyeWidth {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, min_read_data_eye_width: u8, min_write_data_eye_width: u8) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    min_read_data_eye_width,
                                    min_write_data_eye_width,
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct CpuFamilyFilter {
                                type_ || #[serde(default = "CpuFamilyFilter::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "CpuFamilyFilter::serde_default_payload_size")] SerdeHex8 : u8,
                                cpu_family_revision || SerdeHex32 : LU32 | pub get u32 : pub set u32,
                            }
                        }
                        impl_EntryCompatible!(CpuFamilyFilter, 18, 4);
                        impl Default for CpuFamilyFilter {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    cpu_family_revision: 0.into(), // probably invalid
                                }
                            }
                        }
                        impl CpuFamilyFilter {
                            pub fn new(cpu_family_revision: u32) -> Self {
                                Self {
                                    cpu_family_revision: cpu_family_revision.into(),
                                    ..Self::default()
                                }
                            }
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct SolderedDownDimmsPerChannel {
                                type_ || #[serde(default = "SolderedDownDimmsPerChannel::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "SolderedDownDimmsPerChannel::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds,
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds,
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value || SerdeHex8 : u8 | pub get u8 : pub set u8,
                            }
                        }
                        impl_EntryCompatible!(SolderedDownDimmsPerChannel, 19, 4);
                        impl Default for SolderedDownDimmsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 0, // probably invalid
                                }
                            }
                        }
                        impl SolderedDownDimmsPerChannel {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, value: u8) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    value,
                                    ..Self::default()
                                }
                            }
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum MemPowerPolicyType {
                            Performance = 0,
                            BatteryLife = 1,
                            Auto = 2,
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemPowerPolicy {
                                type_ || #[serde(default = "MemPowerPolicy::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MemPowerPolicy::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value || MemPowerPolicyType : u8 | pub get MemPowerPolicyType : pub set MemPowerPolicyType,
                            }
                        }
                        impl_EntryCompatible!(MemPowerPolicy, 20, 4);
                        impl Default for MemPowerPolicy {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 0, // probably invalid
                                }
                            }
                        }
                        impl MemPowerPolicy {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, value: u8) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    value,
                                    ..Self::default()
                                }
                            }
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
                        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
                        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
                        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
                        pub enum MotherboardLayerCount {
                            #[cfg_attr(feature = "serde", serde(rename="4"))]
                            _4 = 0,
                            #[cfg_attr(feature = "serde", serde(rename="6"))]
                            _6 = 1,
                        }

                        make_accessors! {
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MotherboardLayers {
                                type_ || #[serde(default = "MotherboardLayers::serde_default_tag")] SerdeHex8 : u8 | pub get u8 : pub set u8,
                                payload_size || #[serde(default = "MotherboardLayers::serde_default_payload_size")] SerdeHex8 : u8,
                                sockets || SocketIds : u8 | pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels || ChannelIds : u8 | pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms || DimmSlots : u8 | pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value || MotherboardLayerCount : u8 | pub get MotherboardLayerCount : pub set MotherboardLayerCount,
                            }
                        }
                        impl_EntryCompatible!(MotherboardLayers, 21, 4);
                        impl Default for MotherboardLayers {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG as u8,
                                    payload_size: (size_of::<Self>() - 2) as u8,
                                    sockets: SocketIds::ALL.to_u8().unwrap(),
                                    channels: ChannelIds::Any.to_u8().unwrap(),
                                    dimms: DimmSlots::Any.to_u8().unwrap(),
                                    value: 0, // probably invalid
                                }
                            }
                        }
                        impl MotherboardLayers {
                            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, value: u8) -> Self {
                                Self {
                                    sockets: sockets.to_u8().unwrap(),
                                    channels: channels.to_u8().unwrap(),
                                    dimms: dimms.to_u8().unwrap(),
                                    value,
                                    ..Self::default()
                                }
                            }
                        }

                        // TODO: conditional overrides, actions.
                }

        impl EntryCompatible for ElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
                // Also supports empty chunks, so don't check prefix.
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::PlatformSpecificOverride)
                )
            }
            fn skip_step(
                entry_id: EntryId,
                prefix: &[u8],
            ) -> Option<(u16, usize)> {
                match entry_id {
                    EntryId::Memory(
                        MemoryEntryId::PlatformSpecificOverride,
                    ) => {
                        if !prefix.is_empty() && prefix[0] == 0 {
                            // work around AMD padding all the Entrys with 0s
                            return Some((0, 1));
                        }
                        if prefix.len() >= 2 {
                            let type_ = prefix[0] as u16;
                            let size = (prefix[1] as usize).checked_add(2)?;
                            Some((type_, size))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
        impl EntryCompatible for MutElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
                // Also supports empty chunks, so don't check prefix.
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::PlatformSpecificOverride)
                )
            }
            fn skip_step(
                entry_id: EntryId,
                prefix: &[u8],
            ) -> Option<(u16, usize)> {
                match entry_id {
                    EntryId::Memory(
                        MemoryEntryId::PlatformSpecificOverride,
                    ) => {
                        if !prefix.is_empty() && prefix[0] == 0 {
                            // work around AMD padding all the Entrys with 0s
                            return Some((0, 1));
                        }
                        if prefix.len() >= 2 {
                            let type_ = prefix[0] as u16;
                            let size = (prefix[1] as usize).checked_add(2)?;
                            Some((type_, size))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
        /* We don't want Unknown to be serializable, so this is not implemented on purpose.
        impl SequenceElementAsBytes for MutElementRef {
            fn checked_as_bytes(&mut self, entry_id: EntryId) -> Option<&[u8]> {
                match &mut self {
                    Self::Unknown(ref item) => ,
                }.checked_as_bytes(entry_id)
            }
        }
        */
    }

    pub mod platform_tuning {
        use super::{EntryId, Error, MemoryEntryId};
        crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum! {
                        use core::mem::size_of;
                        use static_assertions::const_assert;
                        use zerocopy::{IntoBytes, FromBytes, Unaligned};
                        use super::super::*;
                        //use crate::struct_accessors::{Getter, Setter, make_accessors};
                        //use crate::types::Result;

                        macro_rules! impl_EntryCompatible {($struct_:ty, $type_:expr, $total_size:expr) => (
                            const_assert!($total_size as usize == size_of::<$struct_>());
                            impl $struct_ {
                                const TAG: u16 = $type_;
                                #[allow(dead_code)]
                                fn serde_default_tag() -> SerdeHex16 {
                                    Self::TAG.into()
                                }
                            }

                            impl EntryCompatible for $struct_ {
                                fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
                                    match entry_id {
                                        EntryId::Memory(MemoryEntryId::PlatformTuning) => {
                                            const TYPE_: u16 = $type_;
                                            const TYPE_LO: u8 = (TYPE_ & 0xff) as u8;
                                            const TYPE_HI: u8 = (TYPE_ >> 8) as u8;
                                            if prefix.len() >= 2 && prefix[0] == TYPE_LO && prefix[1] == TYPE_HI {
                                                const SHOULD_HAVE_LEN_FIELD: bool = TYPE_ != 0xfeef;
                                                !SHOULD_HAVE_LEN_FIELD || (prefix.len() >= 3 && prefix[2] == $total_size)
                                            } else {
                                                false
                                            }
                                        },
                                        _ => false,
                                    }
                                }

                                fn skip_step(entry_id: EntryId, prefix: &[u8]) -> Option<(u16, usize)> {
                                    match entry_id {
                                        EntryId::Memory(MemoryEntryId::PlatformTuning) => {
                                            if prefix.len() >= 2 {
                                                let type_lo = prefix[0];
                                                let type_hi = prefix[1];
                                                let type_ = ((type_hi as u16) << 8) | (type_lo as u16);
                                                if type_ == 0xfeef { // no len available
                                                    Some((type_, 2))
                                                } else if prefix.len() >= 3 {
                                                    let size = (prefix[2] as usize).checked_add(2)?;
                                                    Some((type_, size))
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        },
                                        _ => {
                                            None
                                        },
                                    }
                                }
                            }

                            impl SequenceElementAsBytes for $struct_ {
                                fn checked_as_bytes(&self, entry_id: EntryId) -> Option<&[u8]> {
                                    let blob = IntoBytes::as_bytes(self);
                                    if <$struct_>::is_entry_compatible(entry_id, blob) {
                                        Some(blob)
                                    } else {
                                        None
                                    }
                                }
                            }

                            //            impl HeaderWithTail for $struct_ {
                            //                type TailArrayItemType = ();
                            //            }
                        )}

                        make_accessors!{
                            #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug,
        Clone, Copy)]
                            #[repr(C, packed)]
                            pub struct Terminator {
                                // The value of this field will automatically be set on deserialization.
                                type_ || #[serde(default = "Terminator::serde_default_tag")] SerdeHex16 : LU16 | pub get u16 : pub set u16,
                            }
                        }
                        impl_EntryCompatible!(Terminator, 0xfeef, 2);

                        impl Default for Terminator {
                            fn default() -> Self {
                                Self {
                                    type_: Self::TAG.into(),
                                }
                            }
                        }

                        impl Terminator {
                            pub fn new() -> Self {
                                Self::default()
                            }
                        }

                //        impl HeaderWithTail for PlatformTuningElementRef<'_> {
                //            type TailArrayItemType = ();
                //        }
                }

        impl EntryCompatible for ElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
                // Also supports empty chunks; so don't check for prefix.len()
                // >= 2,
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::PlatformTuning)
                )
            }
            fn skip_step(
                entry_id: EntryId,
                prefix: &[u8],
            ) -> Option<(u16, usize)> {
                match entry_id {
                    EntryId::Memory(MemoryEntryId::PlatformTuning) => {
                        if !prefix.is_empty() && prefix[0] == 0 {
                            // work around AMD padding all the Entrys with 0s
                            return Some((0, 1));
                        }
                        if prefix.len() >= 2 {
                            let type_lo = prefix[0];
                            let type_hi = prefix[1];
                            let type_ =
                                ((type_hi as u16) << 8) | (type_lo as u16);
                            if type_ == 0xfeef {
                                // no len available
                                Some((type_, 2))
                            } else if prefix.len() >= 3 {
                                let size =
                                    (prefix[2] as usize).checked_add(2)?;
                                Some((type_, size))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
        impl EntryCompatible for MutElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
                // Also supports empty chunks; so don't check for prefix.len()
                // >= 2,
                matches!(
                    entry_id,
                    EntryId::Memory(MemoryEntryId::PlatformTuning)
                )
            }
            fn skip_step(
                entry_id: EntryId,
                prefix: &[u8],
            ) -> Option<(u16, usize)> {
                match entry_id {
                    EntryId::Memory(MemoryEntryId::PlatformTuning) => {
                        if !prefix.is_empty() && prefix[0] == 0 {
                            // work around AMD padding all the Entrys with 0s
                            return Some((0, 1));
                        }
                        if prefix.len() >= 2 {
                            let type_lo = prefix[0];
                            let type_hi = prefix[1];
                            let type_ =
                                ((type_hi as u16) << 8) | (type_lo as u16);
                            if type_ == 0xfeef {
                                // no len available
                                Some((type_, 2))
                            } else if prefix.len() >= 3 {
                                let size =
                                    (prefix[2] as usize).checked_add(2)?;
                                Some((type_, size))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use static_assertions::const_assert;

        #[test]
        fn test_memory_structs() {
            const_assert!(size_of::<DimmInfoSmbusElement>() == 8);
            const_assert!(size_of::<AblConsoleOutControl>() == 16);
            const_assert!(size_of::<ConsoleOutControl>() == 20);
            const_assert!(size_of::<NaplesAblConsoleOutControl>() == 16);
            const_assert!(size_of::<NaplesConsoleOutControl>() == 20);
            const_assert!(size_of::<ExtVoltageControl>() == 32);
            const_assert!(size_of::<RdimmDdr4CadBusElement>() == 36);
            const_assert!(size_of::<UdimmDdr4CadBusElement>() == 36);
            const_assert!(size_of::<LrdimmDdr4CadBusElement>() == 36);
            const_assert!(size_of::<Ddr4DataBusElement>() == 52);
            const_assert!(size_of::<MaxFreqElement>() == 16);
            const_assert!(size_of::<LrMaxFreqElement>() == 16);
            const_assert!(size_of::<Gpio>() == 3);
            const_assert!(size_of::<ErrorOutControlBeepCode>() == 8);
            const_assert!(size_of::<ErrorOutControlBeepCodePeakAttr>() == 4);
            assert!(offset_of!(ErrorOutControl116, beep_code_table) == 44);
            assert!(offset_of!(ErrorOutControl116, enable_heart_beat) == 108);
            assert!(offset_of!(ErrorOutControl116, power_good_gpio) == 110);
            const_assert!(size_of::<ErrorOutControl116>() == 116);
            assert!(offset_of!(ErrorOutControl112, beep_code_table) == 40);
            assert!(offset_of!(ErrorOutControl112, enable_heart_beat) == 104);
            assert!(offset_of!(ErrorOutControl112, power_good_gpio) == 106);
            const_assert!(size_of::<ErrorOutControl112>() == 112);
            const_assert!(size_of::<DdrDqPinMapElement>() == 64);
            const_assert!(size_of::<Ddr5CaPinMapElement>() == 28);
            const_assert!(size_of::<RdimmDdr5BusElementHeader>() == 12);
            const_assert!(size_of::<RdimmDdr5BusElementPayload>() == 124);
            const_assert!(size_of::<RdimmDdr5BusElement>() == 12 + 124);
            const_assert!(size_of::<MemDfeSearchElementHeader12>() == 12);
            const_assert!(size_of::<MemDfeSearchElementPayload12>() == 12);
            const_assert!(size_of::<MemDfeSearchElementPayloadExt12>() == 12);
            const_assert!(size_of::<MemDfeSearchElement36>() == 36);
            assert!(offset_of!(MemDfeSearchElement36, payload) == 12);
            assert!(offset_of!(MemDfeSearchElement36, payload_ext) == 24);
            assert!(offset_of!(MemDfeSearchElement32, payload) == 8);
            const_assert!(size_of::<MemDfeSearchElement32>() == 32);
        }

        #[test]
        fn test_memory_struct_accessors() {
            let dimm_info = DimmInfoSmbusElement::new_slot(
                1,
                2,
                3,
                4,
                Some(5),
                Some(6),
                Some(7),
            )
            .unwrap();
            assert!(dimm_info.dimm_slot_present().unwrap());
            assert!(dimm_info.socket_id().unwrap() == 1);
            assert!(dimm_info.channel_id().unwrap() == 2);
            assert!(dimm_info.dimm_id().unwrap() == 3);
            assert!(dimm_info.dimm_smbus_address() == Some(4));
            assert!(dimm_info.i2c_mux_address() == Some(5));
            assert!(dimm_info.mux_control_address() == Some(6));
            assert!(dimm_info.mux_channel() == Some(7));
            let dimm_info = DimmInfoSmbusElement::new_slot(
                1,
                2,
                3,
                4,
                Some(5),
                Some(6),
                Some(7),
            )
            .unwrap();
            assert!(dimm_info.dimm_slot_present().unwrap());
            let dimm_info_2 = DimmInfoSmbusElement::new_soldered_down(
                1,
                2,
                3,
                4,
                Some(5),
                Some(6),
                Some(7),
            )
            .unwrap();
            assert!(!dimm_info_2.dimm_slot_present().unwrap());

            let mut abl_console_out_control = AblConsoleOutControl::default();
            abl_console_out_control.set_enable_mem_setreg_logging(true);
            assert!(
                abl_console_out_control.enable_mem_setreg_logging().unwrap()
            );
            abl_console_out_control.set_enable_mem_setreg_logging(false);
            assert!(
                !abl_console_out_control.enable_mem_setreg_logging().unwrap()
            );
            let console_out_control = ConsoleOutControl::new(
                abl_console_out_control,
                AblBreakpointControl::default(),
            );
            assert!(
                console_out_control
                    .abl_console_out_control
                    .abl_console_port()
                    .unwrap()
                    == 0x80
            );
        }

        #[test]
        fn test_platform_specific_overrides() {
            use platform_specific_override::{
                ChannelIds, DimmSlots, ElementRef, LvDimmForce1V5, SocketIds,
            };
            let lvdimm = LvDimmForce1V5::new(
                SocketIds::ALL,
                ChannelIds::Any,
                DimmSlots::Any,
            );
            let tag: Option<ElementRef<'_>> = Some((&lvdimm).into());
            match tag {
                Some(ElementRef::LvDimmForce1V5(_item)) => {}
                _ => {
                    assert!(false);
                }
            }
        }

        #[test]
        fn test_platform_specific_overrides_mut() {
            use platform_specific_override::{
                ChannelIds, DimmSlots, LvDimmForce1V5, MutElementRef, SocketIds,
            };
            let mut lvdimm = LvDimmForce1V5::new(
                SocketIds::ALL,
                ChannelIds::Any,
                DimmSlots::Any,
            );
            let tag: Option<MutElementRef<'_>> = Some((&mut lvdimm).into());
            match tag {
                Some(MutElementRef::LvDimmForce1V5(ref _item)) => {}
                _ => {
                    assert!(false);
                }
            }
        }

        #[test]
        fn test_getters_with_invalid_data() {
            let mut data = [2u8; size_of::<AblConsoleOutControl>()];
            let mut buf = &mut data[..];
            let header =
                take_header_from_collection_mut::<AblConsoleOutControl>(
                    &mut buf,
                )
                .unwrap();
            match header.enable_console_logging() {
                Err(Error::EntryTypeMismatch) => {}
                _ => {
                    panic!("Should not be reached");
                }
            }
            header.set_enable_console_logging(true);
            assert!(header.enable_console_logging().unwrap());
            header.set_enable_console_logging(false);
            assert!(!header.enable_console_logging().unwrap());
        }
        /// Since modular-bitfield doesn't support i4, we have implemented it ourselves.
        /// Let's test it.
        #[test]
        pub fn test_i4() {
            let mut entry = Ddr5TrainingOverride40Element::builder()
                .with_selected_mem_clks(
                    Ddr5TrainingOverrideEntryHeaderFlagsMemClkMask::builder().with_ddr4800(true).build(),
                )
                .with_selected_channels(
                    Ddr5TrainingOverrideEntryHeaderChannelSelectorMask::builder().with_c_0(true).with_c_11(true).build(),
                )
                .with_write_dq_delay_offset(5)
                .build();
            assert!(entry.selected_mem_clks().unwrap().ddr4800());
            assert!(!entry.selected_mem_clks().unwrap().ddr5600());
            assert_eq!(entry.read_dq_delay_offset().unwrap(), 0);
            assert_eq!(entry.read_dq_vref_offset().unwrap(), 0);
            assert_eq!(entry.write_dq_delay_offset().unwrap(), 5);
            assert_eq!(entry.write_dq_vref_offset().unwrap(), 0);
            entry.set_write_dq_vref_offset(1);
            assert_eq!(entry.write_dq_vref_offset().unwrap(), 1);
            entry.set_write_dq_vref_offset(7);
            assert_eq!(entry.write_dq_vref_offset().unwrap(), 7);
            entry.set_write_dq_vref_offset(-1);
            assert_eq!(entry.write_dq_vref_offset().unwrap(), -1);
            entry.set_write_dq_vref_offset(-8);
            assert_eq!(entry.write_dq_vref_offset().unwrap(), -8);
        }
    }
}

pub mod fch {
    use super::*;
    use crate::struct_accessors::{BU8, Getter, Setter, make_accessors};
    use crate::types::Result;

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitDataBusSelect {
        OutputLow = 0,
        OutputHigh = 1,
    }

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitClockPinSelect {
        Spi = 0,
        Gpio86 = 1,
    }

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitCsPinSelect {
        Gpio30 = 0,
        Gpio31 = 1,
    }

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitClockFrequency {
        #[cfg_attr(feature = "serde", serde(rename = "16.66 MHz"))]
        _16_66MHz = 0,
        #[cfg_attr(feature = "serde", serde(rename = "33.33 MHz"))]
        _33_33MHz = 1,
        #[cfg_attr(feature = "serde", serde(rename = "66.66 MHz"))]
        _66_66MHz = 2,
    }

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitIoMode {
        Single = 0,
        Dual = 1,
        Quad = 2,
    }

    #[repr(u8)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Clone, Copy)]
    #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum EspiInitAlertMode {
        NoDedicatedAlertPin = 0,
        DedicatedAlertPin = 1,
    }

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct EspiInit {
            espi_enabled || bool : BU8 | pub get bool : pub set bool,

            data_bus_select || EspiInitDataBusSelect : u8 | pub get EspiInitDataBusSelect : pub set EspiInitDataBusSelect,
            clock_pin_select || EspiInitClockPinSelect : u8 | pub get EspiInitClockPinSelect : pub set EspiInitClockPinSelect,
            cs_pin_select || EspiInitCsPinSelect : u8 | pub get EspiInitCsPinSelect : pub set EspiInitCsPinSelect,
            clock_frequency || EspiInitClockFrequency : u8 | pub get EspiInitClockFrequency : pub set EspiInitClockFrequency,
            io_mode || EspiInitIoMode : u8 | pub get EspiInitIoMode : pub set EspiInitIoMode,
            alert_mode || EspiInitAlertMode : u8 | pub get EspiInitAlertMode : pub set EspiInitAlertMode,

            pltrst_deassert || bool : BU8 | pub get bool : pub set bool,
            io80_decoding_enabled || bool : BU8 | pub get bool : pub set bool,
            io6064_decoding_enabled || bool : BU8 | pub get bool : pub set bool,

            /// The first entry is usually for IPMI.
            /// The last two entries != 0 are the serial ports.
            /// Use values 3 (32 bit) or 7 (64 bit).
            io_range_sizes_minus_one || [SerdeHex8; 16] : [u8; 16],
            /// The first entry is usually for IPMI.
            /// The last two entries != 0 are the serial ports.
            io_range_bases || [SerdeHex16; 16] : [LU16; 16],

            mmio_range_sizes_minus_one || [SerdeHex16; 5] : [LU16; 5],
            mmio_range_bases || [SerdeHex32; 5] : [LU32; 5],

            /// bitmap
            irq_mask || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            /// bitmap
            irq_polarity || SerdeHex32 : LU32 | pub get u32 : pub set u32,

            cputemp_rtctime_vw_enabled || bool : BU8 | pub get bool : pub set bool,
            cputemp_rtctime_vw_index_select || SerdeHex8 : u8 | pub get u8 : pub set u8, // FIXME what's that?

            _reserved_1 || #[serde(default)] u8 : u8,
            _reserved_2 || #[serde(default)] u8 : u8,

            cpu_temp_mmio_base || SerdeHex32 : LU32, // 0: none
            rtc_time_mmio_base || SerdeHex32 : LU32, // 0: none

            bus_master_enabled || bool : BU8 | pub get bool : pub set bool,

            _reserved_3 || #[serde(default)] u8 : u8,
            _reserved_4 || #[serde(default)] u8 : u8,
            _reserved_5 || #[serde(default)] u8 : u8,
        }
    }

    pub struct EspiInitIoRange {
        pub base: u16,
        /// Real size in bytes.
        pub size: u8,
    }

    pub struct EspiInitMmioRange {
        pub base: u32,
        /// Real size in bytes.
        pub size: u16,
    }

    impl EspiInit {
        pub fn io_range(
            &self,
            index: usize,
        ) -> Result<Option<EspiInitIoRange>> {
            if index < self.io_range_sizes_minus_one.len() {
                Ok(if self.io_range_bases[index].get() == 0 {
                    None
                } else {
                    Some(EspiInitIoRange {
                        base: self.io_range_bases[index].get(),
                        size: self.io_range_sizes_minus_one[index] + 1,
                    })
                })
            } else {
                Err(Error::EntryRange)
            }
        }
        pub fn set_io_range(
            &mut self,
            index: usize,
            value: Option<EspiInitIoRange>,
        ) {
            if index < self.io_range_sizes_minus_one.len() {
                match value {
                    None => {
                        self.io_range_sizes_minus_one[index] = 0;
                        self.io_range_bases[index] = 0.into();
                    }
                    Some(x) => {
                        assert!(x.size > 0);
                        self.io_range_sizes_minus_one[index] = x.size - 1;
                        self.io_range_bases[index] = x.base.into();
                    }
                }
            }
        }
        pub fn io_range_count(&self) -> usize {
            self.io_range_sizes_minus_one.len()
        }

        pub fn mmio_range(
            &self,
            index: usize,
        ) -> Result<Option<EspiInitMmioRange>> {
            if index < self.mmio_range_sizes_minus_one.len() {
                Ok(if self.mmio_range_bases[index].get() == 0 {
                    None
                } else {
                    Some(EspiInitMmioRange {
                        base: self.mmio_range_bases[index].get(),
                        size: self.mmio_range_sizes_minus_one[index].get() + 1,
                    })
                })
            } else {
                Err(Error::EntryRange)
            }
        }
        pub fn set_mmio_range(
            &mut self,
            index: usize,
            value: Option<EspiInitMmioRange>,
        ) {
            if index < self.mmio_range_sizes_minus_one.len() {
                match value {
                    None => {
                        self.mmio_range_sizes_minus_one[index] = 0.into();
                        self.mmio_range_bases[index] = 0.into();
                    }
                    Some(x) => {
                        assert!(x.size > 0);
                        self.mmio_range_sizes_minus_one[index] =
                            (x.size - 1).into();
                        self.mmio_range_bases[index] = x.base.into();
                    }
                }
            }
        }
        pub fn mmio_range_count(&self) -> usize {
            self.mmio_range_sizes_minus_one.len()
        }

        pub fn cpu_temp_mmio_base(&self) -> Result<Option<u32>> {
            match self.cpu_temp_mmio_base.get() {
                0 => Ok(None),
                x => Ok(Some(x)),
            }
        }
        pub fn set_cpu_temp_mmio_base(&mut self, value: Option<u32>) {
            self.cpu_temp_mmio_base.set(value.unwrap_or(0));
        }
        pub fn rtc_time_mmio_base(&self) -> Result<Option<u32>> {
            match self.rtc_time_mmio_base.get() {
                0 => Ok(None),
                x => Ok(Some(x)),
            }
        }
        pub fn set_rtc_time_mmio_base(&mut self, value: Option<u32>) {
            self.rtc_time_mmio_base.set(value.unwrap_or(0));
        }
    }

    impl EntryCompatible for EspiInit {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Fch(FchEntryId::EspiInit))
        }
    }

    impl HeaderWithTail for EspiInit {
        type TailArrayItemType<'de> = ();
    }

    #[derive(
        Debug, Default, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone,
    )]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum EspiSioAccessWidth {
        Terminator = 0, // XXX
        #[cfg_attr(feature = "serde", serde(rename = "8 Bit"))]
        _8Bit = 1,
        #[cfg_attr(feature = "serde", serde(rename = "16 Bit"))]
        _16Bit = 2,
        #[cfg_attr(feature = "serde", serde(rename = "32 Bit"))]
        #[default]
        _32Bit = 4,
    }

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct EspiSioInitElement {
            io_port || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            access_width || EspiSioAccessWidth : LU16 | pub get EspiSioAccessWidth : pub set EspiSioAccessWidth,
            data_mask || SerdeHex32 : LU32 | pub get u32 : pub set u32,
            data_or || SerdeHex32 : LU32 | pub get u32 : pub set u32,
        }
    }

    impl EntryCompatible for EspiSioInitElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(entry_id, EntryId::Fch(FchEntryId::EspiSioInit))
        }
    }

    impl HeaderWithTail for EspiSioInitElement {
        type TailArrayItemType<'de> = ();
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn test_struct_sizes() {
            assert!(offset_of!(EspiInit, espi_enabled) == 0);
            assert!(offset_of!(EspiInit, data_bus_select) == 1);
            assert!(offset_of!(EspiInit, clock_pin_select) == 2);
            assert!(offset_of!(EspiInit, cs_pin_select) == 3);
            assert!(offset_of!(EspiInit, clock_frequency) == 4);
            assert!(offset_of!(EspiInit, io_mode) == 5);
            assert!(offset_of!(EspiInit, alert_mode) == 6);
            assert!(offset_of!(EspiInit, pltrst_deassert) == 7);
            assert!(offset_of!(EspiInit, io80_decoding_enabled) == 8);
            assert!(offset_of!(EspiInit, io6064_decoding_enabled) == 9);
            assert!(offset_of!(EspiInit, io_range_sizes_minus_one) == 10);
            assert!(offset_of!(EspiInit, io_range_bases) == 26);
            assert!(offset_of!(EspiInit, mmio_range_sizes_minus_one) == 58);
            assert!(offset_of!(EspiInit, mmio_range_bases) == 68);
            assert!(offset_of!(EspiInit, irq_mask) == 88);
            assert!(offset_of!(EspiInit, irq_polarity) == 92);
            assert!(offset_of!(EspiInit, cputemp_rtctime_vw_enabled) == 96);
            assert!(
                offset_of!(EspiInit, cputemp_rtctime_vw_index_select) == 97
            );
            assert!(offset_of!(EspiInit, cpu_temp_mmio_base) == 100);
            assert!(offset_of!(EspiInit, rtc_time_mmio_base) == 104);
            assert!(offset_of!(EspiInit, bus_master_enabled) == 108);
            assert!(size_of::<EspiInit>() == 112); // 109
        }
    }
}

pub mod psp {
    use super::memory::Gpio;
    use super::*;
    use crate::struct_accessors::{Getter, Setter, make_accessors};

    make_accessors! {
        #[derive(Default, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct IdApcbMapping {
            id_and_feature_mask || SerdeHex8 : u8 | pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value || SerdeHex8 : u8 | pub get u8 : pub set u8,
            board_instance_index || SerdeHex8 : u8 | pub get u8 : pub set u8,
        }
    }
    impl IdApcbMapping {
        pub fn new(
            id_and_feature_mask: u8,
            id_and_feature_value: u8,
            board_instance_index: u8,
        ) -> Self {
            Self {
                id_and_feature_mask,
                id_and_feature_value,
                board_instance_index,
            }
        }
        pub fn board_instance_mask(&self) -> Result<u16> {
            if self.board_instance_index <= 15 {
                Ok(1u16 << self.board_instance_index)
            } else {
                Err(Error::EntryTypeMismatch)
            }
        }
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum RevAndFeatureValue {
        Value(u8),
        NotApplicable,
    }
    impl FromPrimitive for RevAndFeatureValue {
        fn from_u64(value: u64) -> Option<Self> {
            if value < 0x100 {
                match value {
                    0xff => Some(Self::NotApplicable),
                    x => Some(Self::Value(x as u8)),
                }
            } else {
                None
            }
        }
        fn from_i64(value: i64) -> Option<Self> {
            if value >= 0 {
                let value: u64 = value.try_into().ok()?;
                Self::from_u64(value)
            } else {
                None
            }
        }
    }
    impl ToPrimitive for RevAndFeatureValue {
        fn to_i64(&self) -> Option<i64> {
            match self {
                Self::Value(x) => {
                    if *x == 0xff {
                        None
                    } else {
                        Some((*x).into())
                    }
                }
                Self::NotApplicable => Some(0xff),
            }
        }
        fn to_u64(&self) -> Option<u64> {
            Some(self.to_i64()? as u64)
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct IdRevApcbMapping {
            id_and_rev_and_feature_mask || SerdeHex8 : u8 | pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value || SerdeHex8 : u8 | pub get u8 : pub set u8,
            rev_and_feature_value || RevAndFeatureValue : u8 | pub get RevAndFeatureValue : pub set RevAndFeatureValue,
            board_instance_index || SerdeHex8 : u8 | pub get u8 : pub set u8,
        }
    }

    impl IdRevApcbMapping {
        pub fn new(
            id_and_rev_and_feature_mask: u8,
            id_and_feature_value: u8,
            rev_and_feature_value: RevAndFeatureValue,
            board_instance_index: u8,
        ) -> Result<Self> {
            Ok(Self {
                id_and_rev_and_feature_mask,
                id_and_feature_value,
                rev_and_feature_value: rev_and_feature_value
                    .to_u8()
                    .ok_or(Error::EntryTypeMismatch)?,
                board_instance_index,
            })
        }
        pub fn board_instance_mask(&self) -> Result<u16> {
            if self.board_instance_index <= 15 {
                Ok(1u16 << self.board_instance_index)
            } else {
                Err(Error::EntryTypeMismatch)
            }
        }
    }

    impl Default for IdRevApcbMapping {
        fn default() -> Self {
            Self {
                id_and_rev_and_feature_mask: 0x80,
                id_and_feature_value: 0,
                rev_and_feature_value: 0,
                board_instance_index: 0,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodCustom {
            access_method || SerdeHex16 : LU16 | pub get u16 : pub set u16, // 0xF for BoardIdGettingMethodCustom
            feature_mask || SerdeHex16 : LU16 | pub get u16 : pub set u16,
        }
    }

    impl Default for BoardIdGettingMethodCustom {
        fn default() -> Self {
            Self { access_method: 0xF.into(), feature_mask: 0.into() }
        }
    }

    impl BoardIdGettingMethodCustom {
        pub fn new(feature_mask: u16) -> Self {
            Self {
                access_method: 0xF.into(),
                feature_mask: feature_mask.into(),
            }
        }
    }

    impl EntryCompatible for BoardIdGettingMethodCustom {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            if prefix.len() >= 2 && prefix[0] == 0xF && prefix[1] == 0 {
                matches!(
                    entry_id,
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod)
                )
            } else {
                false
            }
        }
    }
    impl HeaderWithTail for BoardIdGettingMethodCustom {
        type TailArrayItemType<'de> = IdApcbMapping;
    }

    make_array_accessors!(Gpio, Gpio);
    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodGpio {
            access_method || SerdeHex16 : LU16 | pub get u16 : pub set u16, // 3 for BoardIdGettingMethodGpio
            pub bit_locations: [Gpio; 4] | pub get [Gpio; 4] : pub set [Gpio; 4], // for the board id
        }
    }

    impl Default for BoardIdGettingMethodGpio {
        fn default() -> Self {
            Self {
                access_method: 3.into(),
                bit_locations: [
                    Gpio::new(0, 0, 0), // probably invalid
                    Gpio::new(0, 0, 0), // probably invalid
                    Gpio::new(0, 0, 0), // probably invalid
                    Gpio::new(0, 0, 0), // probably invalid
                ],
            }
        }
    }

    impl BoardIdGettingMethodGpio {
        pub fn new(bit_locations: [Gpio; 4]) -> Self {
            Self { access_method: 3.into(), bit_locations }
        }
    }

    impl EntryCompatible for BoardIdGettingMethodGpio {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            if prefix.len() >= 2 && prefix[0] == 3 && prefix[1] == 0 {
                matches!(
                    entry_id,
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod)
                )
            } else {
                false
            }
        }
    }
    impl HeaderWithTail for BoardIdGettingMethodGpio {
        type TailArrayItemType<'de> = IdApcbMapping;
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodEeprom {
            access_method || SerdeHex16 : LU16 | pub get u16 : pub set u16, // 2 for BoardIdGettingMethodEeprom
            i2c_controller_index || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            device_address || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            board_id_offset || SerdeHex16 : LU16 | pub get u16 : pub set u16, // Byte offset
            board_rev_offset || SerdeHex16 : LU16 | pub get u16 : pub set u16, // Byte offset
        }
    }

    impl Default for BoardIdGettingMethodEeprom {
        fn default() -> Self {
            Self {
                access_method: 2.into(),
                i2c_controller_index: 0.into(), // maybe invalid
                device_address: 0.into(),
                board_id_offset: 0.into(),
                board_rev_offset: 0.into(),
            }
        }
    }
    impl BoardIdGettingMethodEeprom {
        pub fn new(
            i2c_controller_index: u16,
            device_address: u16,
            board_id_offset: u16,
            board_rev_offset: u16,
        ) -> Self {
            Self {
                access_method: 2.into(),
                i2c_controller_index: i2c_controller_index.into(),
                device_address: device_address.into(),
                board_id_offset: board_id_offset.into(),
                board_rev_offset: board_rev_offset.into(),
            }
        }
    }

    impl EntryCompatible for BoardIdGettingMethodEeprom {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            if prefix.len() >= 2 && prefix[0] == 2 && prefix[1] == 0 {
                matches!(
                    entry_id,
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod)
                )
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for BoardIdGettingMethodEeprom {
        type TailArrayItemType<'de> = IdRevApcbMapping;
    }

    make_accessors! {
        #[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodSmbus {
            access_method || SerdeHex16 : LU16 | pub get u16 : pub set u16, // 1 for BoardIdGettingMethodSmbus
            i2c_controller_index || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            i2c_mux_address || SerdeHex8 : u8 | pub get u8 : pub set u8,
            mux_control_address || SerdeHex8 : u8 | pub get u8 : pub set u8,
            mux_channel || SerdeHex8 : u8 | pub get u8 : pub set u8,
            smbus_address || SerdeHex16 : LU16 | pub get u16 : pub set u16,
            register_index || SerdeHex16 : LU16 | pub get u16 : pub set u16,
        }
    }

    impl Default for BoardIdGettingMethodSmbus {
        fn default() -> Self {
            Self {
                access_method: 1.into(),
                i2c_controller_index: 0.into(), // maybe invalid
                i2c_mux_address: 0,             // maybe invalid
                mux_control_address: 0,         // maybe invalid
                mux_channel: 0,                 // maybe invalid
                smbus_address: 0.into(),        // maybe invalid
                register_index: 0.into(),
            }
        }
    }

    impl BoardIdGettingMethodSmbus {
        pub fn new(
            i2c_controller_index: u16,
            i2c_mux_address: u8,
            mux_control_address: u8,
            mux_channel: u8,
            smbus_address: u16,
            register_index: u16,
        ) -> Self {
            Self {
                access_method: 1.into(),
                i2c_controller_index: i2c_controller_index.into(),
                i2c_mux_address,
                mux_control_address,
                mux_channel,
                smbus_address: smbus_address.into(),
                register_index: register_index.into(),
            }
        }
    }

    impl EntryCompatible for BoardIdGettingMethodSmbus {
        fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
            if prefix.len() >= 2 && prefix[0] == 1 && prefix[1] == 0 {
                matches!(
                    entry_id,
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod)
                )
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for BoardIdGettingMethodSmbus {
        type TailArrayItemType<'de> = IdApcbMapping;
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use static_assertions::const_assert;

        #[test]
        fn test_psp_structs() {
            const_assert!(size_of::<IdApcbMapping>() == 3);
            const_assert!(size_of::<BoardIdGettingMethodGpio>() == 14);
            const_assert!(size_of::<IdRevApcbMapping>() == 4);
            const_assert!(size_of::<BoardIdGettingMethodEeprom>() == 10);
        }
    }
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BaudRate {
    #[cfg_attr(feature = "serde", serde(rename = "2400 Baud"))]
    _2400Baud = 0,
    #[cfg_attr(feature = "serde", serde(rename = "3600 Baud"))]
    _3600Baud = 1,
    #[cfg_attr(feature = "serde", serde(rename = "4800 Baud"))]
    _4800Baud = 2,
    #[cfg_attr(feature = "serde", serde(rename = "7200 Baud"))]
    _7200Baud = 3,
    #[cfg_attr(feature = "serde", serde(rename = "9600 Baud"))]
    _9600Baud = 4,
    #[cfg_attr(feature = "serde", serde(rename = "19200 Baud"))]
    _19200Baud = 5,
    #[cfg_attr(feature = "serde", serde(rename = "38400 Baud"))]
    _38400Baud = 6,
    #[cfg_attr(feature = "serde", serde(rename = "57600 Baud"))]
    _57600Baud = 7,
    #[cfg_attr(feature = "serde", serde(rename = "115200 Baud"))]
    _115200Baud = 8,
    /// Note: This variant needs a custom AgesaBootloader.
    #[cfg_attr(feature = "serde", serde(rename = "3000000 Baud"))]
    _3000000Baud = 9,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemActionOnBistFailure {
    DoNothing = 0,
    DisableProblematicCcds = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemDataPoison {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemSpdVerifyCrc {
    Skip = 0,
    Verify = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemOdtsMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemEcsModeDdr {
    EcsAuto = 0,
    EcsManual = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemBootTimePostPackageRepair {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMaxActivityCount {
    Untested = 0,
    #[cfg_attr(feature = "serde", serde(rename = "700000"))]
    _700000 = 1,
    #[cfg_attr(feature = "serde", serde(rename = "600000"))]
    _600000 = 2,
    #[cfg_attr(feature = "serde", serde(rename = "500000"))]
    _500000 = 3,
    #[cfg_attr(feature = "serde", serde(rename = "400000"))]
    _400000 = 4,
    #[cfg_attr(feature = "serde", serde(rename = "300000"))]
    _300000 = 5,
    #[cfg_attr(feature = "serde", serde(rename = "200000"))]
    _200000 = 6,
    Unlimited = 8,
    Auto = 0xff,
}

impl MemMaxActivityCount {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_700000'")]
    pub const _700K: Self = Self::_700000;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_600000'")]
    pub const _600K: Self = Self::_600000;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_500000'")]
    pub const _500K: Self = Self::_500000;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_400000'")]
    pub const _400K: Self = Self::_400000;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_300000'")]
    pub const _300K: Self = Self::_300000;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_200000'")]
    pub const _200K: Self = Self::_200000;
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemRcwWeakDriveDisable {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemSelfRefreshExitStaggering {
    Disabled = 0,
    OneThird = 3,  // Trfc/3
    OneFourth = 4, // Trfc/4
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CbsMemAddrCmdParityRetryDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxSevAsidCount {
    #[cfg_attr(feature = "serde", serde(rename = "253"))]
    _253 = 0,
    #[cfg_attr(feature = "serde", serde(rename = "509"))]
    _509 = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxApicMode {
    /// Don't use anymore.
    Compatibility = 0,
    #[cfg_attr(feature = "serde", serde(rename = "xAPIC"))]
    XApic = 1,
    #[cfg_attr(feature = "serde", serde(rename = "x2APIC"))]
    X2Apic = 2,
    Auto = 0xFF,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxSmtControl {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xf,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxCoreControl {
    Auto = 0,
    #[cfg_attr(feature = "serde", serde(rename = "1 + 0"))]
    _1Plus0 = 1,
    #[cfg_attr(feature = "serde", serde(rename = "2 + 0"))]
    _2Plus0 = 2,
    #[cfg_attr(feature = "serde", serde(rename = "3 + 0"))]
    _3Plus0 = 3,
    #[cfg_attr(feature = "serde", serde(rename = "4 + 0"))]
    _4Plus0 = 4,
    #[cfg_attr(feature = "serde", serde(rename = "5 + 0"))]
    _5Plus0 = 5,
    #[cfg_attr(feature = "serde", serde(rename = "6 + 0"))]
    _6Plus0 = 6,
    #[cfg_attr(feature = "serde", serde(rename = "7 + 0"))]
    _7Plus0 = 7,
    #[cfg_attr(feature = "serde", serde(rename = "8 + 0"))]
    _8Plus0 = 8,
    #[cfg_attr(feature = "serde", serde(rename = "9 + 0"))]
    _9Plus0 = 9,
    #[cfg_attr(feature = "serde", serde(rename = "10 + 0"))]
    _10Plus0 = 10,
    #[cfg_attr(feature = "serde", serde(rename = "11 + 0"))]
    _11Plus0 = 11,
    #[cfg_attr(feature = "serde", serde(rename = "12 + 0"))]
    _12Plus0 = 12,
    #[cfg_attr(feature = "serde", serde(rename = "13 + 0"))]
    _13Plus0 = 13,
    #[cfg_attr(feature = "serde", serde(rename = "14 + 0"))]
    _14Plus0 = 14,
    #[cfg_attr(feature = "serde", serde(rename = "15 + 0"))]
    _15Plus0 = 15,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxL3XiPrefetchReqThrottleEnable {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CcxCcdControl {
    Auto = 0,
    #[cfg_attr(feature = "serde", serde(rename = "2"))]
    _2 = 2,
    #[cfg_attr(feature = "serde", serde(rename = "4"))]
    _4 = 4,
    #[cfg_attr(feature = "serde", serde(rename = "6"))]
    _6 = 6,
    #[cfg_attr(feature = "serde", serde(rename = "8"))]
    _8 = 8,
    #[cfg_attr(feature = "serde", serde(rename = "10"))]
    _10 = 10,
    #[cfg_attr(feature = "serde", serde(rename = "12"))]
    _12 = 12,
    #[cfg_attr(feature = "serde", serde(rename = "14"))]
    _14 = 14,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchConsoleOutMode {
    Disabled = 0,
    Enabled = 1,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FchConsoleOutMode {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct ModeVisitor;
        impl<'de> serde::de::Visitor<'de> for ModeVisitor {
            type Value = FchConsoleOutMode;
            fn expecting(
                &self,
                formatter: &mut core::fmt::Formatter<'_>,
            ) -> core::fmt::Result {
                formatter.write_str("'Disabled', 'Enabled', 0 or 1")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> core::result::Result<Self::Value, E> {
                match v {
                    "Disabled" => Ok(FchConsoleOutMode::Disabled),
                    "Enabled" => Ok(FchConsoleOutMode::Enabled),
                    _ => Err(serde::de::Error::custom(
                        "'Disabled', 'Enabled', 0 or 1 was expected",
                    )),
                }
            }
            fn visit_i64<E: serde::de::Error>(
                self,
                value: i64,
            ) -> core::result::Result<Self::Value, E> {
                match value {
                    0 => Ok(FchConsoleOutMode::Disabled),
                    1 => Ok(FchConsoleOutMode::Enabled),
                    _ => Err(serde::de::Error::custom(
                        "'Disabled', 'Enabled', 0 or 1 was expected",
                    )),
                }
            }
            fn visit_u64<E: serde::de::Error>(
                self,
                value: u64,
            ) -> core::result::Result<Self::Value, E> {
                match value {
                    0 => Ok(FchConsoleOutMode::Disabled),
                    1 => Ok(FchConsoleOutMode::Enabled),
                    _ => Err(serde::de::Error::custom(
                        "'Disabled', 'Enabled', 0 or 1 was expected",
                    )),
                }
            }
        }
        deserializer.deserialize_any(ModeVisitor)
    }
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchConsoleOutSuperIoType {
    Auto = 0,
    Type1 = 1,
    Type2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchConsoleSerialPort {
    SuperIo = 0,
    Uart0Mmio = 1,
    Uart1Mmio = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchConsoleOutSerialPortIoBase {
    #[cfg_attr(feature = "serde", serde(rename = "3f8"))]
    _3f8 = 0,
    #[cfg_attr(feature = "serde", serde(rename = "2f8"))]
    _2f8 = 1,
    #[cfg_attr(feature = "serde", serde(rename = "3e8"))]
    _3e8 = 2,
    #[cfg_attr(feature = "serde", serde(rename = "2e8"))]
    _2e8 = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchIc3TransferSpeed {
    #[cfg_attr(feature = "serde", serde(alias = "12.5 MHz"))]
    Sdr0 = 0,
    #[cfg_attr(feature = "serde", serde(alias = "6 MHz"))]
    Sdr2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfToggle {
    Disabled = 0,
    Enabled = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemTsmeMode {
    Disabled = 0,
    Enabled = 1,
    // Auto = 0xff, // TODO: Test.
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemNvdimmPowerSource {
    DeviceManaged = 1,
    HostManaged = 2,
}

// See JESD82-31A Table 48.
#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemRdimmTimingCmdParLatency {
    #[cfg_attr(feature = "serde", serde(rename = "1 nCK"))]
    _1nCK = 0, // not valid in gear-down mode
    #[cfg_attr(feature = "serde", serde(rename = "2 nCK"))]
    _2nCK = 1,
    #[cfg_attr(feature = "serde", serde(rename = "3 nCK"))]
    _3nCK = 2, // not valid in gear-down mode
    #[cfg_attr(feature = "serde", serde(rename = "4 nCK"))]
    _4nCK = 3,
    #[cfg_attr(feature = "serde", serde(rename = "0 nCK"))]
    _0nCK = 4, // only valid if parity checking and CAL modes are disabled
    Auto = 0xff,
}

impl MemRdimmTimingCmdParLatency {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1nCK'")]
    pub const _1_nCK: Self = Self::_1nCK;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_2nCK'")]
    pub const _2_nCK: Self = Self::_2nCK;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_3nCK'")]
    pub const _3_nCK: Self = Self::_3nCK;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_4nCK'")]
    pub const _4_nCK: Self = Self::_4nCK;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_0nCK'")]
    pub const _0_nCK: Self = Self::_0nCK;
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemThrottleCtrlRollWindowDepth<T> {
    Memclks(T),
    // 0: _reserved_
}

/// See UMC::SpazCtrl: AutoRefFineGranMode.
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemAutoRefreshFineGranMode {
    Fixed1Times = 0,
    Fixed2Times = 1,
    Fixed4Times = 2,
    Otf2Times = 5,
    Otf4Times = 6,
}

/// See UMC::CH::ThrottleCtrl: DisRefCmdThrotCnt.
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemAutoRefreshsCountForThrottling {
    Enabled = 0,
    Disabled = 1,
}

impl FromPrimitive for MemThrottleCtrlRollWindowDepth<NonZeroU8> {
    fn from_u64(value: u64) -> Option<Self> {
        if value > 0 && value <= 0xff {
            let value = NonZeroU8::new(value as u8)?;
            Some(Self::Memclks(value))
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for MemThrottleCtrlRollWindowDepth<NonZeroU8> {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Memclks(x) => Some((*x).get().into()),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}
impl FromPrimitive for MemThrottleCtrlRollWindowDepth<NonZeroU16> {
    fn from_u64(value: u64) -> Option<Self> {
        if value > 0 && value <= 0xffff {
            let value = NonZeroU16::new(value as u16)?;
            Some(Self::Memclks(value))
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for MemThrottleCtrlRollWindowDepth<NonZeroU16> {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Memclks(x) => Some((*x).get().into()),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemControllerWritingCrcMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemHealPprType {
    SoftRepair = 0,
    HardRepair = 1,
    NoRepair = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemHealTestSelect {
    Normal = 0,
    NoVendorTests = 1,
    ForceAllVendorTests = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemPmuBistAlgorithmSelectDdr {
    User = 0,
    Vendor = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfExtIpSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfMemInterleaving {
    None = 0,
    Channel = 1,
    Die = 2,
    Socket = 3,
    Auto = 7,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfMemInterleavingSize {
    #[cfg_attr(feature = "serde", serde(rename = "256 B"))]
    _256_Byte = 0,
    #[cfg_attr(feature = "serde", serde(rename = "512 B"))]
    _512_Byte = 1,
    #[cfg_attr(feature = "serde", serde(rename = "1024 B"))]
    _1024_Byte = 2,
    #[cfg_attr(feature = "serde", serde(rename = "2048 B"))]
    _2048_Byte = 3,
    #[cfg_attr(feature = "serde", serde(rename = "4096 B"))]
    _4096_Byte = 4,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfCxlToggle {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfCcdBwThrottleLevel {
    #[cfg_attr(feature = "serde", serde(rename = "Level 0"))]
    _0 = 0,
    #[cfg_attr(feature = "serde", serde(rename = "Level 1"))]
    _1 = 1,
    #[cfg_attr(feature = "serde", serde(rename = "Level 2"))]
    _2 = 2,
    #[cfg_attr(feature = "serde", serde(rename = "Level 3"))]
    _3 = 3,
    #[cfg_attr(feature = "serde", serde(rename = "Level 4"))]
    _4 = 4,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfDramNumaPerSocket {
    None = 0,
    One = 1,
    Two = 2,
    Four = 3,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfRemapAt1TiB {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiLinkConfig {
    #[cfg_attr(feature = "serde", serde(rename = "2 links connected"))]
    _2links_connected = 0,
    #[cfg_attr(feature = "serde", serde(rename = "3 links connected"))]
    _3links_connected = 1,
    #[cfg_attr(feature = "serde", serde(rename = "4 links connected"))]
    _4links_connected = 2,
    Auto = 3,
}

impl DfXgmiLinkConfig {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_2links_connected'")]
    pub const _2_links_connected: Self = Self::_2links_connected;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_3links_connected'")]
    pub const _3_links_connected: Self = Self::_3links_connected;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_4links_connected'")]
    pub const _4_links_connected: Self = Self::_4links_connected;
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfPstateModeSelect {
    Normal = 0,
    LimitHighest = 1,
    LimitAll = 2,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfPfOrganization {
    Dedicated = 0,
    Shared = 2,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum GnbSmuDfPstateFclkLimit {
    #[cfg_attr(feature = "serde", serde(rename = "1600 MHz"))]
    _1600MHz = 0,
    #[cfg_attr(feature = "serde", serde(rename = "1467 MHz"))]
    _1467MHz = 1,
    #[cfg_attr(feature = "serde", serde(rename = "1333 MHz"))]
    _1333MHz = 2,
    #[cfg_attr(feature = "serde", serde(rename = "1200 MHz"))]
    _1200MHz = 3,
    #[cfg_attr(feature = "serde", serde(rename = "1067 MHz"))]
    _1067MHz = 4,
    #[cfg_attr(feature = "serde", serde(rename = "933 MHz"))]
    _933MHz = 5,
    #[cfg_attr(feature = "serde", serde(rename = "800 MHz"))]
    _800MHz = 6,
    Auto = 0xff,
}

impl GnbSmuDfPstateFclkLimit {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1600MHz'")]
    const _1600_Mhz: Self = Self::_1600MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1467MHz'")]
    const _1467_Mhz: Self = Self::_1467MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1333MHz'")]
    const _1333_Mhz: Self = Self::_1333MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1200MHz'")]
    const _1200_Mhz: Self = Self::_1200MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_1067MHz'")]
    const _1067_Mhz: Self = Self::_1067MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_933MHz'")]
    const _933_Mhz: Self = Self::_933MHz;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_800MHz'")]
    const _800_Mhz: Self = Self::_800MHz;
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AdditionalPcieLinkSpeed {
    Keep = 0,
    Gen1 = 1,
    Gen2 = 2,
    Gen3 = 3,
}

pub type SecondPcieLinkSpeed = AdditionalPcieLinkSpeed;
pub type ThirdPcieLinkSpeed = AdditionalPcieLinkSpeed;
pub type FourthPcieLinkSpeed = AdditionalPcieLinkSpeed;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BmcLinkSpeed {
    PcieMax = 0,
    PcieGen1 = 1,
    PcieGen2 = 2,
    PcieGen3 = 3,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum SecondPcieLinkMaxPayload {
    #[cfg_attr(feature = "serde", serde(rename = "128 B"))]
    _128_Byte = 0,
    #[cfg_attr(feature = "serde", serde(rename = "256 B"))]
    _256_Byte = 1,
    #[cfg_attr(feature = "serde", serde(rename = "512 B"))]
    _512_Byte = 2,
    #[cfg_attr(feature = "serde", serde(rename = "1024 B"))]
    _1024_Byte = 3,
    #[cfg_attr(feature = "serde", serde(rename = "2048 B"))]
    _2048_Byte = 4,
    #[cfg_attr(feature = "serde", serde(rename = "4096 B"))]
    _4096_Byte = 5,
    HardwareDefault = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum WorkloadProfile {
    Disabled = 0,
    CpuIntensive = 1,
    JavaThroughput = 2,
    JavaLatency = 3,
    PowerEfficiency = 4,
    MemoryThroughputIntensive = 5,
    StorageIoIntensive = 6,
    NicThroughputIntensive = 7,
    NicLatencySensitive = 8,
    AcceleratorThroughput = 9,
    VmwareVsphereOptimized = 10,
    LinuxKvmOptimized = 11,
    ContainerOptimized = 12,
    RdbmsOptimized = 13,
    BigDataAnalyticsOptimized = 14,
    IotGateway = 15,
    HpcOptimized = 16,
    OpenStackNfv = 17,
    OpenStackForRealTimeKernel = 18,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemControllerPmuTrainFfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemControllerPmuTrainDfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemControllerPmuTrainingMode {
    #[cfg_attr(feature = "serde", serde(rename = "1D"))]
    _1D = 0,
    #[cfg_attr(feature = "serde", serde(rename = "1D,2D,Read-Only"))]
    _1D_2D_Read_Only = 1,
    #[cfg_attr(feature = "serde", serde(rename = "1D,2D,Write-Only"))]
    _1D_2D_Write_Only = 2,
    #[cfg_attr(feature = "serde", serde(rename = "1D,2D"))]
    _1D_2D = 3,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum UmaMode {
    None = 0,
    Specified = 1,
    Auto = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistTest {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistPatternSelect {
    Prbs = 0,
    Sso = 1,
    Both = 2,
    Auto = 0xFF,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistAggressorsChannels {
    Disabled = 0,
    #[cfg_attr(feature = "serde", serde(rename = "1 aggressor/(2 ch)"))]
    _1AggressorsPer2Channels = 1,
    #[cfg_attr(feature = "serde", serde(rename = "3 aggressor/(4 ch)"))]
    _3AggressorsPer4Channels = 3,
    #[cfg_attr(feature = "serde", serde(rename = "7 aggressor/(8 ch)"))]
    _7AggressorsPer8Channels = 7,
}

impl MemMbistAggressorsChannels {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_700000'")]
    pub const _1_AggressorsPer2Channels: Self = Self::_1AggressorsPer2Channels;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_700000'")]
    pub const _3_AggressorsPer4Channels: Self = Self::_3AggressorsPer4Channels;
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to '_700000'")]
    pub const _7_AggressorsPer8Channels: Self = Self::_7AggressorsPer8Channels;
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistTestMode {
    PhysicalInterface = 0,
    DataEye = 1,
    Both = 2,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemHealingBistRepairTypeDdr {
    Soft = 0,
    Hard = 1,
    None = 2,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistDataEyeType {
    #[cfg_attr(feature = "serde", serde(rename = "1D Voltage"))]
    _1dVoltage = 0,
    #[cfg_attr(feature = "serde", serde(rename = "1D Timing"))]
    _1dTiming = 1,
    #[cfg_attr(feature = "serde", serde(rename = "2D Full Data Eye"))]
    _2dFullDataEye = 2,
    WorstCaseMarginOnly = 3,
}

impl MemMbistDataEyeType {
    #[allow(non_upper_case_globals)]
    #[deprecated(
        note = "This variant had a typo and has since been fixed to '_1dVoltage'"
    )]
    pub const _1dVolate: Self = Self::_1dVoltage;
}

#[test]
#[allow(deprecated)]
fn test_volate() {
    assert!(MemMbistDataEyeType::_1dVolate == MemMbistDataEyeType::_1dVoltage);
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiTxEqMode {
    Disabled = 0,
    EnabledByLane = 1,
    EnabledByLink = 2,
    EnabledByLinkAndRxVetting = 3,
    Auto = 0xff,
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiLinkMaxSpeed {
    #[cfg_attr(feature = "serde", serde(rename = "6.4 Gbit/s"))]
    _6_40Gbps = 0,
    #[cfg_attr(feature = "serde", serde(rename = "7.467 Gbit/s"))]
    _7_467Gbps = 1,
    #[cfg_attr(feature = "serde", serde(rename = "8.533 Gbit/s"))]
    _8_533Gbps = 2,
    #[cfg_attr(feature = "serde", serde(rename = "9.6 Gbit/s"))]
    _9_6Gbps = 3,
    #[cfg_attr(feature = "serde", serde(rename = "10.667 Gbit/s"))]
    _10_667Gbps = 4,
    #[cfg_attr(feature = "serde", serde(rename = "11 Gbit/s"))]
    _11Gbps = 5,
    #[cfg_attr(feature = "serde", serde(rename = "12 Gbit/s"))]
    _12Gbps = 6,
    #[cfg_attr(feature = "serde", serde(rename = "13 Gbit/s"))]
    _13Gbps = 7,
    #[cfg_attr(feature = "serde", serde(rename = "14 Gbit/s"))]
    _14Gbps = 8,
    #[cfg_attr(feature = "serde", serde(rename = "15 Gbit/s"))]
    _15Gbps = 9,
    #[cfg_attr(feature = "serde", serde(rename = "16 Gbit/s"))]
    _16Gbps = 10,
    #[cfg_attr(feature = "serde", serde(rename = "17 Gbit/s"))]
    _17Gbps = 11,
    #[cfg_attr(feature = "serde", serde(rename = "18 Gbit/s"))]
    _18Gbps = 12,
    #[cfg_attr(feature = "serde", serde(rename = "19 Gbit/s"))]
    _19Gbps = 13,
    #[cfg_attr(feature = "serde", serde(rename = "20 Gbit/s"))]
    _20Gbps = 14,
    #[cfg_attr(feature = "serde", serde(rename = "21 Gbit/s"))]
    _21Gbps = 15,
    #[cfg_attr(feature = "serde", serde(rename = "22 Gbit/s"))]
    _22Gbps = 16,
    #[cfg_attr(feature = "serde", serde(rename = "23 Gbit/s"))]
    _23Gbps = 17,
    #[cfg_attr(feature = "serde", serde(rename = "24 Gbit/s"))]
    _24Gbps = 18,
    #[cfg_attr(feature = "serde", serde(rename = "25 Gbit/s"))]
    _25Gbps = 19,
    #[cfg_attr(feature = "serde", serde(rename = "26 Gbit/s"))]
    _26Gbps = 20,
    #[cfg_attr(feature = "serde", serde(rename = "27 Gbit/s"))]
    _27Gbps = 21,
    #[cfg_attr(feature = "serde", serde(rename = "28 Gbit/s"))]
    _28Gbps = 22,
    #[cfg_attr(feature = "serde", serde(rename = "29 Gbit/s"))]
    _29Gbps = 23,
    #[cfg_attr(feature = "serde", serde(rename = "30 Gbit/s"))]
    _30Gbps = 24,
    #[cfg_attr(feature = "serde", serde(rename = "31 Gbit/s"))]
    _31Gbps = 25,
    #[cfg_attr(feature = "serde", serde(rename = "32 Gbit/s"))]
    _32Gbps = 26,
    Auto = 0xff,
}

pub type DfXgmi2LinkMaxSpeed = DfXgmiLinkMaxSpeed;
pub type DfXgmi3LinkMaxSpeed = DfXgmiLinkMaxSpeed;
pub type DfXgmi4LinkMaxSpeed = DfXgmiLinkMaxSpeed;

/// Placement of private memory regions (PSP, SMU, CC6)
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfSysStorageAtTopOfMem {
    /// CCD0 and CCD1 at the top of specific memory region (default)
    Distributed = 0,
    /// Consolidate the privileged region and put at the top of the last memory
    /// region (recommended)
    ConsolidatedInLastDramPair = 1,
    /// Consolidate and put at the top of the first memory region
    ConsolidatedInFirstDramPair = 2,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BmcGen2TxDeemphasis {
    Csr = 0,
    Upstream = 1,
    Minus_6_dB = 2,
    Minus_3_5_dB = 3,
    Disabled = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BmcRcbCheckingMode {
    EnableRcbChecking = 0,
    DisableRcbChecking = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfCxlStronglyOrderedWrites {
    Enabled = 1,
    Disabled = 0,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CxlResetPin {
    None = 0,
    Gpio26 = 1,
    Gpio266 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfUmcCxlMixedInterleavedMode {
    Enabled = 1,
    Disabled = 0,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfTgtReqGo {
    Enabled = 1,
    Disabled = 0,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfGmiSubTxRxMode {
    Enabled = 1,
    Disabled = 0,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemChannelDisableFloatPowerGoodDdr {
    Enabled = 1,
    Disabled = 0,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum EccSymbolSize {
    x4 = 0,
    x8 = 1,
    x16 = 2,
}

// This trait exists so we can impl it for bool; the macro MAKE_TOKEN_ACCESSORS
// will call the function by name without specifying the trait anyway.
trait ToPrimitive1 {
    fn to_u32(&self) -> Option<u32>;
}
// This trait exists so we can impl it for bool; the macro MAKE_TOKEN_ACCESSORS
// will call the function by name without specifying the trait anyway.
trait FromPrimitive1: Sized {
    fn from_u32(value: u32) -> Option<Self>;
}

impl ToPrimitive1 for bool {
    fn to_u32(&self) -> Option<u32> {
        match self {
            true => Some(1),
            false => Some(0),
        }
    }
}

impl FromPrimitive1 for bool {
    fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DxioPhyParamVga {
    Value(u32), // not 0xffff_ffff
    Skip,
}
impl FromPrimitive for DxioPhyParamVga {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000_0000 {
            match value {
                0xffff_ffff => Some(Self::Skip),
                x => Some(Self::Value(x as u32)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for DxioPhyParamVga {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Value(x) => {
                if *x == 0xffff_ffff {
                    None
                } else {
                    Some((*x).into())
                }
            }
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DxioPhyParamPole {
    Value(u32), // not 0xffff_ffff
    Skip,
}
impl FromPrimitive for DxioPhyParamPole {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            0xffff_ffff => Some(Self::Skip),
            0..=0xFFFFFFFE => Some(Self::Value(value as u32)),
            _ => None,
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for DxioPhyParamPole {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Value(x) => {
                if *x == 0xffff_ffff {
                    None
                } else {
                    Some((*x).into())
                }
            }
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DxioPhyParamDc {
    Value(u32), // not 0xffff_ffff
    Skip,
}
impl FromPrimitive for DxioPhyParamDc {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000_0000 {
            match value {
                0xffff_ffff => Some(Self::Skip),
                x => Some(Self::Value(x as u32)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for DxioPhyParamDc {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Value(x) => {
                if *x == 0xffff_ffff {
                    None
                } else {
                    Some((*x).into())
                }
            }
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DxioPhyParamIqofc {
    Value(i32),
    Skip,
}
impl FromPrimitive for DxioPhyParamIqofc {
    fn from_i64(value: i64) -> Option<Self> {
        match value {
            0x7FFFFFFF => Some(Self::Skip),
            -4..=4 => Some(Self::Value(value as i32)),
            _ => None,
        }
    }
    fn from_u64(value: u64) -> Option<Self> {
        Self::from_i64(value as i64)
    }
}
impl ToPrimitive for DxioPhyParamIqofc {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Value(x) => Some((*x).into()),
            Self::Skip => Some(0x7FFFFFFF),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemTargetSpeed {
    #[cfg_attr(feature = "serde", serde(rename = "3200 MT/s"))]
    _3200 = 3200,
    #[cfg_attr(feature = "serde", serde(rename = "3600 MT/s"))]
    _3600 = 3600,
    #[cfg_attr(feature = "serde", serde(rename = "4000 MT/s"))]
    _4000 = 4000,
    #[cfg_attr(feature = "serde", serde(rename = "4400 MT/s"))]
    _4400 = 4400,
    #[cfg_attr(feature = "serde", serde(rename = "4800 MT/s"))]
    _4800 = 4800,
    #[cfg_attr(feature = "serde", serde(rename = "5200 MT/s"))]
    _5200 = 5200,
    #[cfg_attr(feature = "serde", serde(rename = "5600 MT/s"))]
    _5600 = 5600,
    #[cfg_attr(feature = "serde", serde(rename = "6400 MT/s"))]
    _6400 = 6400,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[repr(u32)]
pub enum MemClockValue {
    // in MHz
    Ddr400 = 200,
    Ddr533 = 266,
    Ddr667 = 333,
    Ddr800 = 400,
    Ddr1066 = 533,
    Ddr1333 = 667,
    Ddr1600 = 800,
    Ddr1866 = 933,
    Ddr2100 = 1050,
    Ddr2133 = 1067,
    Ddr2400 = 1200,
    Ddr2667 = 1333,
    Ddr2800 = 1400,
    Ddr2933 = 1467,
    Ddr3066 = 1533,
    Ddr3200 = 1600,
    Ddr3333 = 1667,
    Ddr3466 = 1733,
    Ddr3600 = 1800,
    Ddr3733 = 1867,
    Ddr3866 = 1933,
    Ddr4000 = 2000,
    Ddr4200 = 2100,
    Ddr4267 = 2133,
    Ddr4333 = 2167,
    Ddr4400 = 2200,
    Ddr4467 = 2233,
    Ddr4533 = 2267,
    Ddr4600 = 2300,
    Ddr4667 = 2333,
    Ddr4733 = 2367,
    Ddr4800 = 2400,
    Ddr4867 = 2433,
    Ddr4933 = 2467,
    Ddr5000 = 2500,
    Ddr5100 = 2550,
    Ddr5200 = 2600,
    Ddr5300 = 2650,
    Ddr5400 = 2700,
    Ddr5500 = 2750,
    Ddr5600 = 2800,
    Ddr5700 = 2850,
    Ddr5800 = 2900,
    Ddr5900 = 2950,
    Ddr6000 = 3000,
    Ddr6100 = 3050,
    Ddr6200 = 3100,
    Ddr6300 = 3150,
    Ddr6400 = 3200,
    Ddr6500 = 3250,
    Ddr6600 = 3300,
    Ddr6700 = 3350,
    Ddr6800 = 3400,
    Ddr6900 = 3450,
    Ddr7000 = 3500,
    Ddr7100 = 3550,
    Ddr7200 = 3600,
    Ddr7300 = 3650,
    Ddr7400 = 3700,
    Ddr7500 = 3750,
    Ddr7600 = 3800,
    Ddr7700 = 3850,
    Ddr7800 = 3900,
    Ddr7900 = 3950,
    Ddr8000 = 4000,
    Ddr8100 = 4050,
    Ddr8200 = 4100,
    Ddr8300 = 4150,
    Ddr8400 = 4200,
    Ddr8500 = 4250,
    Ddr8533 = 4267,
    Ddr8600 = 4300,
    Ddr8700 = 4350,
    Ddr8800 = 4400,
    Auto = 0xffff_ffff, // FIXME: verify
}

type MemBusFrequencyLimit = MemClockValue;

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CbsMemPowerDownDelay {
    Value(u16), // not 0, not 0xffff
    Auto,
}

impl FromPrimitive for CbsMemPowerDownDelay {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x1_0000 {
            match value {
                0xffff => Some(Self::Auto),
                x => Some(Self::Value(x as u16)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for CbsMemPowerDownDelay {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Value(x) => {
                if *x == 0xffff {
                    None
                } else {
                    Some((*x).into())
                }
            }
            Self::Auto => Some(0xffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

pub type MemUserTimingMode = memory::platform_specific_override::TimingMode;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemHealBistEnable {
    Disabled = 0,
    TestAndRepairAllMemory = 1,
    JedecSelfHealing = 2,
    TestAndRepairAllMemoryAndJedecSelfHealing = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CbsMemSpeedDdr4 {
    Ddr333 = 4,
    Ddr400 = 6,
    Ddr533 = 0xa,
    Ddr667 = 0x14,
    Ddr800 = 0x18,
    Ddr933 = 0x1c,
    Ddr1050 = 0x19,
    Ddr1066 = 0x1a,
    Ddr1067 = 0x20,
    Ddr1200 = 0x24,
    Ddr1333 = 0x28,
    Ddr1400 = 0x2a,
    Ddr1467 = 0x2c,
    Ddr1533 = 0x2e,
    Ddr1600 = 0x30,
    Ddr1667 = 0x32,
    Ddr1733 = 0x34,
    Ddr1800 = 0x36,
    Ddr1867 = 0x38,
    Ddr1933 = 0x3a,
    Ddr2000 = 0x3c,

    Auto = 0xff,
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchSmbusSpeed {
    Value(u8), /* x in 66 MHz / (4 x) */
               // FIXME: Auto ?
}
impl FromPrimitive for FchSmbusSpeed {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x100 { Some(Self::Value(value as u8)) } else { None }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 { Self::from_u64(value as u64) } else { None }
    }
}
impl ToPrimitive for FchSmbusSpeed {
    fn to_u64(&self) -> Option<u64> {
        match self {
            Self::Value(x) => Some((*x) as u64),
        }
    }
    fn to_i64(&self) -> Option<i64> {
        let result = self.to_u64()?;
        Some(result as i64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfCakeCrcThresholdBounds {
    Value(u32), // x: 0...1_000_000d; Percentage is 0.00001% * x
}
impl FromPrimitive for DfCakeCrcThresholdBounds {
    fn from_u64(value: u64) -> Option<Self> {
        if value <= 1_000_000 { Some(Self::Value(value as u32)) } else { None }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 { Self::from_u64(value as u64) } else { None }
    }
}
impl ToPrimitive for DfCakeCrcThresholdBounds {
    fn to_u64(&self) -> Option<u64> {
        match self {
            Self::Value(x) => Some((*x) as u64),
        }
    }
    fn to_i64(&self) -> Option<i64> {
        let result = self.to_u64()?;
        Some(result as i64)
    }
}

#[derive(Debug, Default, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiChannelType {
    #[default]
    Disabled,
    LongReach,
}

impl FromPrimitive for DfXgmiChannelType {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::LongReach),
            _ => None,
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 { Self::from_u64(value as u64) } else { None }
    }
}
impl ToPrimitive for DfXgmiChannelType {
    fn to_u64(&self) -> Option<u64> {
        match self {
            Self::Disabled => Some(0),
            Self::LongReach => Some(1),
        }
    }
    fn to_i64(&self) -> Option<i64> {
        let result = self.to_u64()?;
        Some(result as i64)
    }
}

impl From<DfXgmiChannelType> for u8 {
    fn from(value: DfXgmiChannelType) -> Self {
        match value {
            DfXgmiChannelType::Disabled => 0,
            DfXgmiChannelType::LongReach => 1,
        }
    }
}
impl From<u8> for DfXgmiChannelType {
    fn from(value: u8) -> Self {
        match value {
            0 => DfXgmiChannelType::Disabled,
            1 => DfXgmiChannelType::LongReach,
            _ => panic!("Invalid value for DfXgmiChannelType: {}", value),
        }
    }
}

make_bitfield_serde! {
    #[bitfield(bits = 32)]
    #[derive(PartialEq, Debug, Copy, Clone)]
    #[repr(u32)]
    pub struct DfXgmiChannelTypeSelect {
        #[bits = 4]
        pub s0l0 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s0l1 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s0l2 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s0l3 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s1l0 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s1l1 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s1l2 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
        #[bits = 4]
        pub s1l3 || DfXgmiChannelType : B4 | pub get DfXgmiChannelType : pub set DfXgmiChannelType,
    }
}
impl_bitfield_primitive_conversion!(
    DfXgmiChannelTypeSelect,
    0b1111_1111_1111_1111,
    u32
);

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemTrainingHdtControl {
    DetailedDebugMessages = 0x04,
    CoarseDebugMessages = 0x0a,
    StageCompletionMessages = 0xc8,
    #[cfg_attr(feature = "serde", serde(alias = "StageCompletionMessages1"))]
    AssertionMessages = 0xc9,
    FirmwareCompletionMessagesOnly = 0xfe,
    Auto = 0xff,
}

impl MemTrainingHdtControl {
    #[allow(non_upper_case_globals)]
    #[deprecated(note = "Name has since been fixed to 'AssertionMessages'")]
    pub const StageCompletionMessages1: Self = Self::AssertionMessages;
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PspEnableDebugMode {
    Disabled = 0,
    Enabled = 1,
}

make_bitfield_serde! {
    #[bitfield(bits = 16)]
    #[repr(u16)]
    #[derive(Default, Debug, Copy, Clone, PartialEq)]
    pub struct FchGppClkMapSelection {
        #[bits = 1]
        pub s0_gpp0_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s0_gpp1_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s0_gpp4_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s0_gpp2_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s0_gpp3_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s0_gpp5_off : bool | pub get bool : pub set bool,
        #[bits = 2]
        pub _reserved_1 || #[serde(default)] SerdeHex8 : B2,

        #[bits = 1]
        pub s1_gpp0_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s1_gpp1_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s1_gpp4_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s1_gpp2_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s1_gpp3_off : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub s1_gpp5_off : bool | pub get bool : pub set bool,
        #[bits = 2]
        pub _reserved_2 || #[serde(default)] SerdeHex8 : B2,
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchGppClkMap {
    On,
    Value(FchGppClkMapSelection),
    Auto,
}
impl FromPrimitive for FchGppClkMap {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            0xffff => Some(Self::Auto),
            0x0000 => Some(Self::On),
            x if value < 0x10000 => {
                Some(Self::Value(FchGppClkMapSelection::from_bytes([
                    (x & 0xff) as u8,
                    (x >> 8) as u8,
                ])))
            }
            _ => None,
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            let value: u64 = value.try_into().ok()?;
            Self::from_u64(value)
        } else {
            None
        }
    }
}
impl ToPrimitive for FchGppClkMap {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::On => Some(0),
            Self::Value(x) => {
                let result: u16 = (*x).into();
                if result == 0xffff || result == 0x0000 {
                    None
                } else {
                    Some(result.into())
                }
            }
            Self::Auto => Some(0xffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

make_bitfield_serde! {
    #[bitfield(bits = 8)]
    #[repr(u8)]
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct MemPmuBistTestSelect {
        #[bits = 1]
        pub algorithm_1 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_2 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_3 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_4 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_5 || #[serde(default)] bool : bool | pub get bool : pub set bool,
        #[bits = 3]
        pub _reserved_0 || #[serde(default)] SerdeHex8 : B3,
    }
}
impl_bitfield_primitive_conversion!(MemPmuBistTestSelect, 0b11111, u8);

#[derive(
    Debug, Default, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemPmuTrainingRetryCount {
    #[default]
    _0 = 0,
    _1 = 1,
    _2 = 2,
    _3 = 3,
    _4 = 4,
    _5 = 5,
    _6 = 6,
    _7 = 7,
    _8 = 8,
    _9 = 9,
    _10 = 10,
}

#[derive(
    Debug, Default, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum EspiController {
    #[default]
    Controller0 = 0,
    Controller1 = 1,
}

impl From<EspiController> for u8 {
    fn from(value: EspiController) -> Self {
        match value {
            EspiController::Controller0 => 0,
            EspiController::Controller1 => 1,
        }
    }
}
impl From<u8> for EspiController {
    fn from(value: u8) -> Self {
        match value {
            0 => EspiController::Controller0,
            1 => EspiController::Controller1,
            _ => panic!("Invalid value for EspiController: {}", value),
        }
    }
}

make_bitfield_serde! {
    #[bitfield(bits = 8)]
    #[repr(u8)]
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct FchConsoleOutSerialPortEspiControllerSelect {
        #[bits = 1]
        pub espi_controller || EspiController : B1 | pub get EspiController : pub set EspiController,
        #[bits = 2]
        pub _reserved_0 || #[serde(default)] SerdeHex8 : B2,
        #[bits = 1]
        pub io_2e_2f_disabled: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub io_4e_4f_disabled: bool | pub get bool : pub set bool,
        #[bits = 3]
        pub _reserved_1 || #[serde(default)] SerdeHex8 : B3,
    }
}
impl_bitfield_primitive_conversion!(
    FchConsoleOutSerialPortEspiControllerSelect,
    0b0001_1001,
    u8
);

make_bitfield_serde! {
    #[bitfield(bits = 16)]
    #[repr(u16)]
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct MemPmuBistAlgorithmSelect {
        #[bits = 1]
        pub algorithm_1: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_2: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_3: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_4: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_5: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_6: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_7: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_8: bool | pub get bool : pub set bool,
        #[bits = 1]
        pub algorithm_9: bool | pub get bool : pub set bool,
        #[bits = 7]
        pub _reserved_0 || #[serde(default)] SerdeHex8 : B7,
    }
}
impl_bitfield_primitive_conversion!(
    MemPmuBistAlgorithmSelect,
    0b1_1111_1111,
    u16
);

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum GnbAdditionalFeatureDsm {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum GnbAdditionalFeatureDsmDetector {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff, // undoc
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PspSevMode {
    Disabled = 1,
    Enabled = 0,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PspApmlSbtsiSlaveMode {
    #[cfg_attr(feature = "serde", serde(alias = "I3C"))]
    I3c = 0,
    #[cfg_attr(feature = "serde", serde(alias = "I2C"))]
    I2c = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemTccd5ReadCommandSpacingMode {
    Disabled = 1,
    Enabled = 0,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ReservedDramModuleDrtmMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiPresetControlMode {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemPmuTrainingResultOutput {
    Disabled = 0,
    Console = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemForcePowerDownThrottleEnable {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemPopulationMsgControlMode {
    Warn = 0,
    Halt = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemOdtsCmdThrottleMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemRcdParityMode {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemThermalThrottleMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchI2cSdaHoldOverrideMode {
    IgnoreBoth = 0,
    OverrideBoth = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchI3cSdaHoldOverrideMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchI2cI3cControllerMode {
    #[cfg_attr(feature = "serde", serde(alias = "I3C"))]
    I3c = 0,
    #[cfg_attr(feature = "serde", serde(alias = "I2C"))]
    I2c = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchI2cI3cSmbusSelect {
    #[cfg_attr(feature = "serde", serde(alias = "I3C"))]
    I3c = 0,
    #[cfg_attr(feature = "serde", serde(alias = "I2C"))]
    I2c = 1,
    #[cfg_attr(feature = "serde", serde(alias = "SMBUS"))]
    Smbus = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchI2cMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchSpdControlOwnershipMode {
    Linked = 0,
    Separate = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfP2pOrderedWrites {
    Enabled = 0,
    Disabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistDdrMode {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistAggressorsChannelDdrMode {
    SubChannel = 0,
    Half = 1,
    All = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistPerBitSlaveDieReportDdr {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemMbistDataEyeSilentExecutionDdr {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MemSelfHealing {
    Disabled = 0,
    Pmu = 1,
    SelfHealing = 2,
    Both = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BdatSupport {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BdatPrintData {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfPdrTuningMode {
    MemorySensity = 0,
    CacheBound = 1,
    Neutral = 2,
    Adaptive = 3,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfPdrMode {
    All = 0,
    Selective = 1,
}

#[derive(BitfieldSpecifier, Copy, Clone, Debug, Default, PartialEq)]
#[bits = 1]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DfXgmiAcDcMode {
    AcCoupled = 0,
    #[default]
    DcCoupled = 1,
}

make_bitfield_serde! {
    #[bitfield(bits = 8)]
    #[repr(u8)]
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct DfXgmiAcDcCoupledLink {
        #[bits = 1]
        pub socket_0_link_0 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_0_link_1 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_0_link_2 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_0_link_3 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_1_link_0 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_1_link_1 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_1_link_2 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
        #[bits = 1]
        pub socket_1_link_3 || #[serde(default)] DfXgmiAcDcMode : DfXgmiAcDcMode | pub get DfXgmiAcDcMode : pub set DfXgmiAcDcMode,
    }
}
impl_bitfield_primitive_conversion!(DfXgmiAcDcCoupledLink, 0b1111_1111, u8);

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchMp1WarnRstAckMode {
    Enable = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FchPowerFailMode {
    DefaultOff = 0,
    On = 1,
    Off = 2,
    Previous = 3,
}

const UNLIMITED_VERSION: u32 = !0u32;

make_token_accessors! {
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum ByteToken: {TokenEntryId::Byte} {
        // ABL

        AblSerialBaudRate(default 8, id 0xae46_cea4) | pub get BaudRate : pub set BaudRate,

        // PSP

        PspEnableDebugMode(default 0, id 0xd109_1cd0) | pub get PspEnableDebugMode : pub set PspEnableDebugMode,

        PspApmlSbtsiSlaveMode(default 0, id 0xb666_1742) | pub get PspApmlSbtsiSlaveMode : pub set PspApmlSbtsiSlaveMode,

        // Memory Controller

        CbsMemSpeedDdr4(default 0xff, id 0xe060_4ce9) | pub get CbsMemSpeedDdr4 : pub set CbsMemSpeedDdr4,
        MemActionOnBistFailure(default 0, id 0xcbc2_c0dd) | pub get MemActionOnBistFailure : pub set MemActionOnBistFailure, // Milan
        MemRcwWeakDriveDisable(default 1, id 0xa30d_781a) | pub get MemRcwWeakDriveDisable : pub set MemRcwWeakDriveDisable, // FIXME is it u32 ?
        MemSelfRefreshExitStaggering(default 0, id 0xbc52_e5f7) | pub get MemSelfRefreshExitStaggering : pub set MemSelfRefreshExitStaggering,
        CbsMemAddrCmdParityRetryDdr4(default 0, id 0xbe8b_ebce) | pub get CbsMemAddrCmdParityRetryDdr4 : pub set CbsMemAddrCmdParityRetryDdr4, // FIXME: Is it u32 ?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CbsMemAddrCmdParityErrorMaxReplayDdr4(default 8, id 0x04e6_a482) | pub get u8 : pub set u8, // 0...0x3f
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CbsMemWriteCrcErrorMaxReplayDdr4(default 8, id 0x74a0_8bec) | pub get u8 : pub set u8,
        // Byte just like AMD
        MemRcdParity(default 1, id 0x647d_7662) | pub get bool : pub set bool,
        MemRcdParityMode(default 1, id 0xc4f7_c913) | pub get MemRcdParityMode : pub set MemRcdParityMode,
        /// How many times to try Specific RCD vendor workaround
        MemRcdSpecificVendorRetryCount(default 5, id 0x7246_00ac) | pub get u8 : pub set u8,

        // Byte just like AMD
        CbsMemUncorrectedEccRetryDdr4(default 1, id 0xbff0_0125) | pub get bool : pub set bool,
        /// UMC::CH::SpazCtrl::UrgRefLimit; value: 1...6 (as in register mentioned first)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUrgRefLimit(default 6, id 0x1333_32df) | pub get u8 : pub set u8,
        /// UMC::CH::SpazCtrl::SubUrgRefLowerBound; value: 1...6 (as in register mentioned first)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSubUrgRefLowerBound(default 4, id 0xe756_2ab6) | pub get u8 : pub set u8,
        MemControllerPmuTrainFfeDdr4(default 0xff, id 0x0d46_186d) | pub get MemControllerPmuTrainFfeDdr4 : pub set MemControllerPmuTrainFfeDdr4, // FIXME: is it bool ?
        MemControllerPmuTrainDfeDdr4(default 0xff, id 0x36a4_bb5b) | pub get MemControllerPmuTrainDfeDdr4 : pub set MemControllerPmuTrainDfeDdr4, // FIXME: is it bool ?
        /// See Transparent Secure Memory Encryption in PPR
        MemTsmeModeRome(default 1, id 0xd1fa_6660) | pub get MemTsmeMode : pub set MemTsmeMode,
        MemTrainingHdtControl(default 200, id 0xaf6d_3a6f) | pub get MemTrainingHdtControl : pub set MemTrainingHdtControl, // TODO: Before using default, fix default.  It's possibly not correct.
        MemHealBistEnable(default 0, id 0xfba2_3a28) | pub get MemHealBistEnable : pub set MemHealBistEnable,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSelfHealBistEnable(default 0, id 0x2c23_924c) | pub get u8 : pub set u8, // FIXME: is it bool ?  // TODO: Before using default, fix default.  It's possibly not correct.
        MemPmuBistAlgorithmSelectDdr(default 1, id 0xc5e7_8e21) | pub get MemPmuBistAlgorithmSelectDdr : pub set MemPmuBistAlgorithmSelectDdr,
        MemPmuBistTestSelect(default 7, id 0x7034_fbfb) | pub get MemPmuBistTestSelect : pub set MemPmuBistTestSelect,
        MemPmuTrainingRetryCount(default 0, id 0x1564_1ea0) | pub get MemPmuTrainingRetryCount : pub set MemPmuTrainingRetryCount,
        MemHealTestSelect(default 0, id 0x5908_2cf2) | pub get MemHealTestSelect : pub set MemHealTestSelect,
        MemHealPprType(default 0, id 0x5418_1a61) | pub get MemHealPprType : pub set MemHealPprType,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemHealMaxBankFails(default 3, id 0x632e_55d8) | pub get u8 : pub set u8, // per bank
        MemTccd5ReadCommandSpacingMode(default 1, id 0x96a5_ed6e) | pub get MemTccd5ReadCommandSpacingMode : pub set MemTccd5ReadCommandSpacingMode, // Milan
        MemMaxRcdParityErrorRelay(default 8, id 0x9702_04a2) | pub get u8 : pub set u8,
        MemMaxUeccErrorReplay(default 8, id 0x3096_b9a5) | pub get u8 : pub set u8,
        MemMaxReadCrcErrorReplay(default 8, id 0x29ad_c904) | pub get u8 : pub set u8,
        MemMaxWriteCrcErrorReplay(default 8, id 0x7342_fc94) | pub get u8 : pub set u8,
        MemReadCrcEnableDdr(default 0, id 0x8360_33c6) | pub get u8 : pub set u8,
        MemWriteCrcEnableDdr(default 0, id 0x7509_87bc) | pub get u8 : pub set u8,
        MemPsErrorHandling(default 0, id 0x6c4c_cf38) | pub get u8 : pub set u8,
        MemUeccRetryEnableDdr(default 0, id 0x4907_b13a) | pub get u8 : pub set u8,
        MemSelfHealing(default 0, id 0x5522_05A3) | pub get MemSelfHealing: pub set MemSelfHealing,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemHealMaxBankFailsDdr(default 3, id 0xa94a_3a01) | pub get u8 : pub set u8,
        MemDataPoisonDdr(default 0, id 0x8614_0d77) | pub get MemDataPoison : pub set MemDataPoison,
        MemSpdVerifyCrc(default 0, id 0x87b6_d2d4) | pub get MemSpdVerifyCrc : pub set MemSpdVerifyCrc,
        MemOdtsMode(default 0, id 0xaeb3_f914) | pub get MemOdtsMode : pub set MemOdtsMode,
        MemEcsModeDdr(default 0xff, id 0xb384_6ddf) | pub get MemEcsModeDdr : pub set MemEcsModeDdr,
        MemBootTimePostPackageRepair(default 0, id 0xe229_1e4a) | pub get MemBootTimePostPackageRepair : pub set MemBootTimePostPackageRepair,
        MemMbistTestModeDdr(default 0, id 0x96df_25ca) | pub get MemMbistTestMode : pub set MemMbistTestMode,
        MemPopulationMsgControl(default 0, id 0x2ce1_24dc) | pub get MemPopulationMsgControlMode : pub set MemPopulationMsgControlMode,
        // TODO(#121): BoolToken::MemOdtsCmdThrottleEnable
        MemOdtsCmdThrottleMode(default 1, id 0xc073_6395) | pub get MemOdtsCmdThrottleMode : pub set MemOdtsCmdThrottleMode,
        MemDisplayPmuTrainingResults(default 0, id 0xb8a6_3eba) | pub get MemPmuTrainingResultOutput : pub set MemPmuTrainingResultOutput,
        /// See UMC::CH::ThrottleCtrl[ForcePwrDownThrotEn].
        MemForcePowerDownThrottleEnableTurin(default 1, id 0x1084_9d6c) | pub get MemForcePowerDownThrottleEnable : pub set MemForcePowerDownThrottleEnable, // used to be bool.
        /// Whether PM should manage throttling--and measure sensor on DIMM
        MemThermalThrottleMode(default 0, id 0xbce9_0051) | pub get MemThermalThrottleMode : pub set MemThermalThrottleMode, // note: default unknown

        /// 40...100; point where memory throttling starts; in °C
        MemThermalThrottleStartInC(default 85, id 0x1449_3d4b) | pub get u8 : pub set u8,
        /// 1...50; how many °C below MemThermalThrottleStartInC until we stop throttling
        MemThermalThrottleHysteresisGapInC(default 5, id 0x2205_08e7) | pub get u8 : pub set u8, // note: default unknown
        /// Throttling as percentage of max, if temperature exceeded by 10 °C or more
        MemThermalThrottlePercentIfTempExceededBy10C(default 40, id 0x0141_8fff) | pub get u8 : pub set u8,
        /// Throttling as percentage of max, if temperature exceeded by 5 °C or more
        MemThermalThrottlePercentIfTempExceededBy5C(default 20, id 0xec5a_c113) | pub get u8 : pub set u8,
        /// Throttling as percentage of max, if temperature exceeded by 0 °C or more
        MemThermalThrottlePercentIfTempExceededBy0C(default 10, id 0x0645_8213) | pub get u8 : pub set u8,

        // Ccx

        CcxSevAsidCount(default 1, id 0x5587_6720) | pub get CcxSevAsidCount : pub set CcxSevAsidCount,
        CcxApicMode(default 0, id 0xf284_ad3f) | pub get CcxApicMode : pub set CcxApicMode,
        CcxSmtControl(default 0, id 0xb0a3_7d17) | pub get CcxSmtControl : pub set CcxSmtControl,
        CcxCoreControl(default 0, id 0x8239_b491) | pub get CcxCoreControl : pub set CcxCoreControl,
        CcxL3XiPrefetchReqThrottleEnable(default 0xff, id 0x6cb9_dd13) | pub get CcxL3XiPrefetchReqThrottleEnable : pub set CcxL3XiPrefetchReqThrottleEnable,
        CcxCcdControl(default 0, id 0x01b7_b701) | pub get CcxCcdControl : pub set CcxCcdControl,

        // Fch

        FchConsoleOutMode(default 0, id 0xddb7_59da) | pub get FchConsoleOutMode : pub set FchConsoleOutMode,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchConsoleOutBasicEnable(default 0, id 0xa0903f98) | pub get u8 : pub set u8, // Rome (Obsolete)
        FchConsoleOutSerialPort(default 0, id 0xfff9_f34d) | pub get FchConsoleSerialPort : pub set FchConsoleSerialPort,
        FchConsoleOutSerialPortIoBase(default 0, id 0x95dc_6839) | pub get FchConsoleOutSerialPortIoBase : pub set FchConsoleOutSerialPortIoBase,
        FchSmbusSpeed(default 42, id 0x2447_3329) | pub get FchSmbusSpeed : pub set FchSmbusSpeed,
        FchConsoleOutSuperIoType(default 0, id 0x5c8d_6e82) | pub get FchConsoleOutSuperIoType : pub set FchConsoleOutSuperIoType, // init mode
        FchIc3TransferSpeed(default 2, id 0x1ed8_fae5) | pub get FchIc3TransferSpeed : pub set FchIc3TransferSpeed,
        FchI2cI3cController0Mode(default 0, id 0xb663_3a33) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        FchI2cI3cController1Mode(default 0, id 0xd2c7_be74) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        FchI2cI3cController2Mode(default 0, id 0x9ec3_f26b) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        FchI2cI3cController3Mode(default 0, id 0x83a0_b3d6) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        FchI2cI3cController4Mode(default 0, id 0xd7ac_3048) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        FchI2cI3cController5Mode(default 0, id 0xebc3_3806) | pub get FchI2cI3cControllerMode : pub set FchI2cI3cControllerMode,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchIc3PushPullHighCount(default 0x08, id 0xe180_bc26) | pub get u8 : pub set u8,

        FchI2cI3cSmbusSelect0(default 1, id 0x811f_7fbd) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect1(default 1, id 0x32bd_7f95) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect2(default 1, id 0x8e27_5326) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect3(default 1, id 0xed13_9de8) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,

        FchI2cI3cSmbusSelect4(default 1, id 0x2158_efca) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect5(default 1, id 0x5caa_a37b) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect6(default 1, id 0x4d6a_dfe4) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect7(default 1, id 0xc5de_4283) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect8(default 1, id 0x6dd8_bd2e) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect9(default 1, id 0x3b11_f9c6) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect10(default 3, id 0x1da2_3853) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,
        FchI2cI3cSmbusSelect11(default 3, id 0xd1fd_a98e) | pub get FchI2cI3cSmbusSelect : pub set FchI2cI3cSmbusSelect,

        FchI2cController4(default 0, id 0xcfde_cf00) | pub get FchI2cMode : pub set FchI2cMode,
        FchI2cController5(default 0, id 0x380c_1b76) | pub get FchI2cMode : pub set FchI2cMode,

        // TODO(#121): WordToken::FchI2cSdaRxHold
        FchI2cSdaRxHold2(default 0, id 0xa4ba_c3d5) | pub get u8: pub set u8,
        // TODO(#121): WordToken::FchI2cSdaHoldOverrideMode & BoolToken::FchI2cSdaHoldOverride
        FchI2cSdaHoldOverrideMode2(default 0, id 0x545d_7662) | pub get FchI2cSdaHoldOverrideMode : pub set FchI2cSdaHoldOverrideMode,

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c0SdaRxHold(default 0, id 0xa79e_1ad2) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c1SdaRxHold(default 0, id 0xf18e_4011) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c2SdaRxHold(default 0, id 0xc5bb_1dcc) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c3SdaRxHold(default 0, id 0xfc88_c252) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c4SdaRxHold(default 0, id 0xdac3_46a1) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c5SdaRxHold(default 0, id 0x4310_d2c7) | pub get u8 : pub set u8,

        FchI3cSdaHoldOverrideMode(default 0, id 0xb418_0673) | pub get FchI3cSdaHoldOverrideMode : pub set FchI3cSdaHoldOverrideMode,

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI3c0SdaTxHold(default 2, id 0xfa62_b04b) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI3c1SdaTxHold(default 2, id 0xc2ff_df14) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI3c2SdaTxHold(default 2, id 0x5945_93c1) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI3c3SdaTxHold(default 2, id 0xd7d2_aac5) | pub get u8 : pub set u8,

        FchI3cSdaHoldSwitchDelay(default 2, id 0xccae_84e7) | pub get u8 : pub set u8,

        FchConsoleOutSerialPortEspiController(default 0, id 0xd9d2_97a6) | pub get FchConsoleOutSerialPortEspiControllerSelect : pub set FchConsoleOutSerialPortEspiControllerSelect,

        FchMp1WarnRstAckMode(default 0xff, id 0x448d_d056) | pub get FchMp1WarnRstAckMode : pub set FchMp1WarnRstAckMode,
        FchPowerFailMode(default 0, id 0x2662_e79d) | pub get FchPowerFailMode : pub set FchPowerFailMode, // default wrong

        // Df

        DfExtIpSyncFloodPropagation(default 0, id 0xfffe_0b07) | pub get DfExtIpSyncFloodPropagation : pub set DfExtIpSyncFloodPropagation,
        DfSyncFloodPropagation(default 0, id 0x4963_9134) | pub get DfSyncFloodPropagation : pub set DfSyncFloodPropagation,
        //DfMemInterleaving(default 7, id 0xce01_87ef) | pub get DfMemInterleaving : pub set DfMemInterleaving,
        DfMemInterleaving(default 7, id 0xce0176ef) | pub get DfMemInterleaving : pub set DfMemInterleaving, // Rome
        DfMemInterleavingSize(default 7, id 0x2606_c42e) | pub get DfMemInterleavingSize : pub set DfMemInterleavingSize,
        DfDramNumaPerSocket(default 1, id 0x2cf3_dac9) | pub get DfDramNumaPerSocket : pub set DfDramNumaPerSocket, // TODO: Maybe the default value here should be 7
        DfProbeFilter(default 1, id 0x6597_c573) | pub get DfToggle : pub set DfToggle,
        DfMemClear(default 3, id 0x9d17_7e57) | pub get DfToggle : pub set DfToggle,
        DfGmiEncrypt(default 0, id 0x08a4_5920) | pub get DfToggle : pub set DfToggle,
        DfXgmiEncrypt(default 0, id 0x6bd3_2f1c) | pub get DfToggle : pub set DfToggle,
        DfSaveRestoreMemEncrypt(default 1, id 0x7b3d_1f75) | pub get DfToggle : pub set DfToggle,
        /// Where the PCI MMIO hole will start (bits 31 to 24 inclusive)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfBottomIo(default 0xe0, id 0x8fb9_8529) | pub get u8 : pub set u8,
        DfRemapAt1TiB(default 0, id 0x35ee_96f3) | pub get DfRemapAt1TiB : pub set DfRemapAt1TiB,
        DfXgmiTxEqMode(default 0xff, id 0xade7_9549) | pub get DfXgmiTxEqMode : pub set DfXgmiTxEqMode,
        DfInvertDramMap(default 0, id 0x6574_b2c0) | pub get DfToggle : pub set DfToggle,
        DfXgmiCrcScale(default 5, id 0x5174_f4a0) | pub get u8 : pub set u8,
        DfCxlMemInterleaving(default 0xFF, id 0x6387_09e6) | pub get DfCxlToggle : pub set DfCxlToggle,
        DfCxlSublinkInterleaving(default 0xFF, id 0x66f_1625a) | pub get DfCxlToggle : pub set DfCxlToggle,
        DfXgmiAcDcCoupledLink(default 0xff, id 0xa7ae_5713) | pub get DfXgmiAcDcCoupledLink : pub set DfXgmiAcDcCoupledLink,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiCrcThreshold(default 0xff, id 0xc375_4da2) | pub get u8 : pub set u8,
        DfCcdBwThrottleLevel(default 0xFF, id 0xd288_6dc9) | pub get DfCcdBwThrottleLevel : pub set DfCcdBwThrottleLevel,
        /// See <https://www.amd.com/content/dam/amd/en/documents/epyc-technical-docs/tuning-guides/58467_amd-epyc-9005-tg-bios-and-workload.pdf>.
        DfPdrTuningMode(default 0xFF, id 0x0645_76ca) | pub get DfPdrTuningMode : pub set DfPdrTuningMode,
        DfPdrMode(default 1, id 0x60b1_fe08) | pub get DfPdrMode : pub set DfPdrMode,
        DfPfOrganization(default 0, id 0xede8_930b) | pub get DfPfOrganization : pub set DfPfOrganization,
        DfXgmiPresetControlMode(default 1, id 0x21aa_0c13) | pub get DfXgmiPresetControlMode : pub set DfXgmiPresetControlMode,
        DfCxlCacheForceStronglyOrderedWrites(default 0, id 0x236e_17a6) | pub get DfCxlStronglyOrderedWrites : pub set DfCxlStronglyOrderedWrites,
        CxlResetPin(default 2, id 0x9226_78f0) | pub get CxlResetPin : pub set CxlResetPin,
        DfUmcCxlMixedInterleavedMode(default 0, id 0x8e49_3e5f) | pub get DfUmcCxlMixedInterleavedMode : pub set DfUmcCxlMixedInterleavedMode,
        DfTgtReqGo(default 0xff, id 0xa2c7_7a9d) | pub get DfTgtReqGo : pub set DfTgtReqGo,
        DfGmiSubTxRxMode(default 0, id 0x6904_3ee0) | pub get DfGmiSubTxRxMode : pub set DfGmiSubTxRxMode,

        // Nbio

        NbioSataMode(default 2, id 0xe8fd_e3b2) | pub get u8 : pub set u8,

        SecondPcieLinkSpeed(default 0, id 0x8723_750f) | pub get SecondPcieLinkSpeed : pub set SecondPcieLinkSpeed,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SecondPcieLinkStartLane(default 0, id 0xb164_8033) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SecondPcieLinkEndLane(default 0, id 0x3bf3_23fd) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SecondPcieLinkDevice(default 0, id 0x05d0_8d49) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SecondPcieLinkFunction(default 0, id 0x1097_e009) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SecondPcieLinkPortPresent(default 0, id 0x973c_eadd) | pub get u8 : pub set u8,
        SecondPcieLinkMaxPayload(default 0xff, id 0xe02d_f04b) | pub get SecondPcieLinkMaxPayload : pub set SecondPcieLinkMaxPayload, // Milan

        ThirdPcieLinkSpeed(default 0, id 0x7963_3632) | pub get ThirdPcieLinkSpeed : pub set ThirdPcieLinkSpeed,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ThirdPcieLinkStartLane(default 0, id 0x8ea3_bd2a) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ThirdPcieLinkEndLane(default 0, id 0x8ec6_1b06) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ThirdPcieLinkDevice(default 0, id 0xd922_71f3) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ThirdPcieLinkFunction(default 0, id 0x2003_5400) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ThirdPcieLinkPortPresent(default 0, id 0xebdc_f0f1) | pub get u8 : pub set u8,

        FourthPcieLinkSpeed(default 0, id 0x0639_6763) | pub get FourthPcieLinkSpeed : pub set FourthPcieLinkSpeed,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FourthPcieLinkStartLane(default 0, id 0x0870_533f) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FourthPcieLinkEndLane(default 0, id 0xa876_54bf) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FourthPcieLinkDevice(default 0, id 0x391e_6398) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FourthPcieLinkFunction(default 0, id 0x09b0_0cd0) | pub get u8 : pub set u8,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FourthPcieLinkPortPresent(default 0, id 0x4736_daf9) | pub get u8 : pub set u8,

        // Misc

        WorkloadProfile(default 0, id 0x22f4_299f) | pub get WorkloadProfile : pub set WorkloadProfile, // Milan
        DvArbiterMin(default 1, id 0x640d_d003) | pub get u8 : pub set u8, // TODO: nicer type
        DvArbiterMax(default 0xa, id 0x6cad_6da9) | pub get u8 : pub set u8, // TODO: nicer type

        ProgSdxiClassCode(default 0, id 0xc43_82e8) | pub get u8 : pub set u8,

        // MBIST for Milan and Rome; defaults wrong!

        MemMbistDataEyeType(default 3, id 0x4e2e_dc1b) | pub get MemMbistDataEyeType : pub set MemMbistDataEyeType,
        // Byte just like AMD
        MemMbistDataEyeSilentExecution(default 0, id 0x3f74_c7e7) | pub get bool : pub set bool, // Milan
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWorseCasGranularity(default 0, id 0x23b0b6a1) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistReadDataEyeVoltageStep(default 0, id 0x35d6a4f8) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneVal(default 0, id 0x4474d416) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneVal(default 0, id 0x4d7e0206) | pub get u8 : pub set u8, // Rome
        MemMbistTestMode(default 0, id 0x567a1fc0) | pub get MemMbistTestMode : pub set MemMbistTestMode, // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelEcc(default 0, id 0x57122e99) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistReadDataEyeTimingStep(default 0, id 0x58ccd28a) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistDataEyeExecutionRepeatCount(default 0, id 0x8e4bdad7) | pub get u8 : pub set u8, // Rome; 0..=10
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelEcc(default 0, id 0xa6e92cee) | pub get u8 : pub set u8, // Rome
        /// in powers of ten; 3..=12
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistPatternLength(default 0, id 0xae7baedd) | pub get u8 : pub set u8, // Rome;
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistHaltOnError(default 0, id 0xb1940f25) | pub get u8 : pub set u8, // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWriteDataEyeVoltageStep(default 0, id 0xcda61022) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistPerBitSlaveDieReport(default 0, id 0xcff56411) | pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWriteDataEyeTimingStep(default 0, id 0xd9025142) | pub get u8 : pub set u8, // Rome
        MemMbistAggressorsChannels(default 0, id 0xdcd1444a) | pub get MemMbistAggressorsChannels : pub set MemMbistAggressorsChannels, // Rome
        MemMbistTest(default 0, id 0xdf5502c8) | pub get MemMbistTest : pub set MemMbistTest, // (obsolete)
        MemMbistPatternSelect(default 0, id 0xf527ebf8) | pub get MemMbistPatternSelect : pub set MemMbistPatternSelect, // Rome
        MemMbistAggressorOn(default 0, id 0x32361c4) | pub get bool : pub set bool, // Rome; obsolete

        // MBIST for Genoa, Bergamo, Turin
        MemMbistDdrMode(default 0, id 0x7dcb_2da5) | pub get MemMbistDdrMode: pub set MemMbistDdrMode,
        MemMbistAggressorsDdr(default 0, id 0xb46e_f9ab) | pub get MemMbistDdrMode : pub set MemMbistDdrMode,
        MemMbistAggressorsChannelDdrMode(default 0, id 0x2fc_8ca9) | pub get MemMbistAggressorsChannelDdrMode : pub set MemMbistAggressorsChannelDdrMode,
        MemMbistPatternSelectDdr(default 0xFF, id 0x5988_cfa6) | pub get MemMbistPatternSelect : pub set MemMbistPatternSelect,
        MemHealingBistRepairTypeDdr(default 0, id 0x9bf8_5c70) | pub get MemHealingBistRepairTypeDdr : pub set MemHealingBistRepairTypeDdr,
        /// in powers of ten; 3..=12
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistPatternLengthDdr(default 0, id 0x108b_b3e6) | pub get u8 : pub set u8,
        MemMbistPerBitSlaveDieReportDdr(default 0xff, id 0x3b78_2d55) | pub get MemMbistPerBitSlaveDieReportDdr : pub set MemMbistPerBitSlaveDieReportDdr,
        /// Enables MBIST DDR margining data to be populated in the BDAT (Schema 8 Types 6 & 7).
        MemMbistDataEyeSilentExecutionDdr(default 0, id 0x8ee6_e78f) | pub get MemMbistDataEyeSilentExecutionDdr : pub set MemMbistDataEyeSilentExecutionDdr,

        // BDAT for Genoa, Bergamo, Turin
        /// Controls whether BDAT entries are populated and present in the system memory map.
        BdatSupport(default 0, id 0x2fad_02d2) | pub get BdatSupport : pub set BdatSupport,
        /// Enables dumping the BDAT entries to the serial console by the ABL during boot.
        BdatPrintData(default 0, id 0xd232_e51c) | pub get BdatPrintData : pub set BdatPrintData,

        // Unsorted Milan; defaults wrong!

        MemOverrideDimmSpdMaxActivityCount(default 0xff, id 0x853cdaa) | pub get MemMaxActivityCount : pub set MemMaxActivityCount,
        GnbSmuDfPstateFclkLimit(default 0xff, id 0xea388ac3) | pub get GnbSmuDfPstateFclkLimit : pub set GnbSmuDfPstateFclkLimit, // Milan
        /// Note: Use this for Milan 1.0.0.4 or higher. For older Milan, use GnbAdditionalFeatureDsm
        GnbAdditionalFeatureDsm2(default 0xff, id 0x31a6afad) | pub get GnbAdditionalFeatureDsm : pub set GnbAdditionalFeatureDsm, // Milan 1.0.0.4
        /// Note: Use this for Milan 1.0.0.4 or higher. For older Milan, use GnbAdditionalFeatureDsmDetector
        GnbAdditionalFeatureDsmDetector2(minimal_version 0x1004_5012, frontier_version UNLIMITED_VERSION,
                                         default 0xff, id 0xf576_8cee) | pub get GnbAdditionalFeatureDsmDetector : pub set GnbAdditionalFeatureDsmDetector, // Milan 1.0.0.4
        ReservedDramModuleDrtmMode(default 0, id 0xb051_e421) | pub get ReservedDramModuleDrtmMode : pub set ReservedDramModuleDrtmMode, // Milan

        // Unsorted Rome; ungrouped; defaults wrong!

        /// I doubt that AMD converts those, but the 2 lowest bits usually set up the resolution. 0: 0.5 ºC; 1: 0.25 ºC; 2: 0.125 ºC; 3: 0.0625 ºC; higher resolution is slower.
        /// DIMM temperature sensor register at address 8
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorResolution(default 0, id 0x831af313) | pub get u8 : pub set u8, // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PcieResetPinSelect(default 0, id 0x8c0b2de9) | pub get u8 : pub set u8, // value 2 // Rome; 0..=4; FIXME: enum?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramAddressCommandParityRetryCount(default 0, id 0x3e7c51f8) | pub get u8 : pub set u8, // value 1 // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemParityErrorMaxReplayDdr4(default 0, id 0xc9e9a1c9) | pub get u8 : pub set u8, // value 8 // Rome // 0..=0x3f (6 bit)
        Df2LinkMaxXgmiSpeed(default 0, id 0xd19c_6e80)| pub get DfXgmi2LinkMaxSpeed : pub set DfXgmi2LinkMaxSpeed, // Genoa
        Df3LinkMaxXgmiSpeed(default 0, id 0x53ba_449b) | pub get DfXgmi3LinkMaxSpeed : pub set DfXgmi3LinkMaxSpeed, // value 0xff // Rome
        Df4LinkMaxXgmiSpeed(default 0, id 0x3f30_7cb3) | pub get DfXgmi4LinkMaxSpeed : pub set DfXgmi4LinkMaxSpeed, // value 0xff //  Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramDoubleRefreshRate(default 0, id 0x44d40026) | pub get u8 : pub set u8, // value 0 // Rome; see also MemDramDoubleRefreshRateMilan
        /// See UMC::CH::ThrottleCtrl RollWindowDepth
        MemRollWindowDepth(default 0xff, id 0x5985083a) | pub get MemThrottleCtrlRollWindowDepth<NonZeroU8> : pub set MemThrottleCtrlRollWindowDepth<NonZeroU8>, // Rome
        DfPstateModeSelect(default 0xff, id 0xaeb84b12) | pub get DfPstateModeSelect : pub set DfPstateModeSelect, // value 0xff // Rome
        DfXgmiConfig(default 3, id 0xb0b6ad3e) | pub get DfXgmiLinkConfig : pub set DfXgmiLinkConfig, // Rome
        /// See DramTiming15_UMCWPHY0_mp0_umc0 CmdParLatency (for the DDR4 Registering Clock Driver).
        /// See also JESD82-31A DDR4 REGISTERING CLOCK DRIVER.
        /// See also <https://github.com/enjoy-digital/litedram/blob/master/litedram/init.py#L460>.
        MemRdimmTimingRcdF0Rc0FAdditionalLatency(default 0xff, id 0xd155798a) | pub get MemRdimmTimingCmdParLatency : pub set MemRdimmTimingCmdParLatency, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDataScramble(default 0, id 0x98aca5b4) | pub get u8 : pub set u8, // Rome (Obsolete)
        MemAutoRefreshFineGranMode(default 0, id 0x190305df) | pub get MemAutoRefreshFineGranMode : pub set MemAutoRefreshFineGranMode, // value 0 // Rome (Obsolete)
        UmaMode(default 0, id 0x1fb35295) | pub get UmaMode : pub set UmaMode, // value 2 // Rome (Obsolete)
        MemNvdimmPowerSource(default 0, id 0x286d0075) | pub get MemNvdimmPowerSource : pub set MemNvdimmPowerSource, // value 1 // Rome (Obsolete)
        MemDataPoison(default 0, id 0x48959473) | pub get MemDataPoison : pub set MemDataPoison, // value 1 // Rome (Obsolete)
        /// See PPR SwCmdThrotCyc
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SwCmdThrotCycles(default 0, id 0xdcec8fcb) | pub get u8 : pub set u8, // value 0 // (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        OdtsCmdThrottleCycles(default 0, id 0x69318e90) | pub get u8 : pub set u8, // value 0x57 // Rome (Obsolete); TODO: Auto?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramVrefRange(default 0, id 0xa8769655) | pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemCpuVrefRange(default 0, id 0x7627cb6d) | pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        MemControllerWritingCrcMode(default 0, id 0x7d1c6e46) | pub get MemControllerWritingCrcMode : pub set MemControllerWritingCrcMode, // value 0 // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemControllerWritingCrcMaxReplay(default 0, id 0x6bb1acf9) | pub get u8 : pub set u8, // value 8 // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemControllerWritingCrcLimit(default 0, id 0xc73a7692) | pub get u8 : pub set u8, // 0..=1 // Rome
        MemChannelDisableFloatPowerGoodDdr(default 0, id 0x847c521b) | pub get MemChannelDisableFloatPowerGoodDdr : pub set MemChannelDisableFloatPowerGoodDdr, // Turin 1.0.0.0
        PmuTrainingMode(default 0xff, id 0xbd4a6afc) | pub get MemControllerPmuTrainingMode : pub set MemControllerPmuTrainingMode, // Rome (Obsolete)
        DfSysStorageAtTopOfMem(default 0xff, id 0x249e08d5) | pub get DfSysStorageAtTopOfMem : pub set DfSysStorageAtTopOfMem,

        // BMC Rome

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcSocket(default 0, id 0x846573f9) | pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcDevice(default 0, id 0xd5bc5fc9) | pub get u8 : pub set u8, // value 5 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcFunction(default 0, id 0x1de4dd61) | pub get u8 : pub set u8, // value 2 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcStartLane(default 0, id 0xb88d87df) | pub get u8 : pub set u8, // value 0x81 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcEndLane(default 0, id 0x143f3963) | pub get u8 : pub set u8, // value 0x81 // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoPortSize(default 0, id 0xfc3f2520) | pub get u8 : pub set u8, // value 0 // legacy
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoBarToReplace(default 0, id 0x2c81a37f) | pub get u8 : pub set u8, // value 0; 0 to 6 // legacy
        BmcGen2TxDeemphasis(default 0xff, id 0xf30d142d) | pub get BmcGen2TxDeemphasis : pub set BmcGen2TxDeemphasis, // value 0xff
        BmcLinkSpeed(default 0, id 0x9c790f4b) | pub get BmcLinkSpeed : pub set BmcLinkSpeed, // value 1
        /// See <https://www.techdesignforums.com/practice/technique/common-pitfalls-in-pci-express-design/>.
        BmcRcbCheckingMode(default 0, id 0xae7f0df4) | pub get BmcRcbCheckingMode : pub set BmcRcbCheckingMode, // value 0xff // Rome
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum WordToken: {TokenEntryId::Word} {
        // PSP

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PspSyshubWatchdogTimerInterval(default 2600, id 0xedb5_e4c9) | pub get u16 : pub set u16, // in ms

        // Memory Controller

        CbsMemPowerDownDelay(default 0xff, id 0x1ebe_755a) | pub get CbsMemPowerDownDelay : pub set CbsMemPowerDownDelay,

        // TODO(#121): ByteToken::MemRollWindowDepth
        MemRollWindowDepth2(default 0x1ff, id 0x5985_083a) | pub get MemThrottleCtrlRollWindowDepth<NonZeroU16> : pub set MemThrottleCtrlRollWindowDepth<NonZeroU16>,
        // TODO(#121): ByteToken::OdtsCmdThrottleCycles
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        OdtsCmdThrottleCycles2(default 0x1FF, id 0x6931_8e90) | pub get u16 : pub set u16,
        MemPmuBistAlgorithmSelect(default 0x1ff, id 0xeb1b_26d3) | pub get MemPmuBistAlgorithmSelect : pub set MemPmuBistAlgorithmSelect,
        MemTargetSpeed(default 4800, id 0xd06d_bafb) | pub get MemTargetSpeed : pub set MemTargetSpeed,

        // Ccx

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CcxCpuWatchDogTimeout(default 0xffff, id 0xcfca_e1bd) | pub get u16 : pub set u16,

        // Fch

        FchGppClkMap(default 0xffff, id 0xcd7e_6983) | pub get FchGppClkMap : pub set FchGppClkMap,
        FchSataSvid(default 0xffff, id 0x111c_de0d) | pub get u16 : pub set u16,
        FchUsbSvid(default 0xffff, id 0x75a3_9c7f) | pub get u16 : pub set u16,

        /// Whether FchI2cSdaRxHold and FchI2cSdaTxHold will be used
        FchI2cSdaHoldOverrideMode(default 0, id 0x545d_7662) | pub get FchI2cSdaHoldOverrideMode : pub set FchI2cSdaHoldOverrideMode, // Milan
        /// See FCH::I2C::IC_SDA_HOLD. Unit: number of ic_clk periods.
        FchI2cSdaRxHold(default 0, id 0xa4ba_c3d5) | pub get u16 : pub set u16, // Milan
        /// See FCH::I2C::IC_SDA_HOLD. Unit: number of ic_clk periods.
        FchI2cSdaTxHold(default 0, id 0x9518_f953) | pub get u16 : pub set u16, // Milan

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c0SdaTxHold(default 0x35, id 0xf6ac_a32e) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c1SdaTxHold(default 0x35, id 0x20ff_9fd8) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c2SdaTxHold(default 0x35, id 0x9c73_6d3c) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c3SdaTxHold(default 0x35, id 0xd75d_dfbf) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c4SdaTxHold(default 0x35, id 0xe821_4ff8) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchI2c5SdaTxHold(default 0x35, id 0xf01a_6a2c) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchSataSsid(default 0xffff, id 0xc2b4_a7e9) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchUsbSsid(default 0xffff, id 0x7451_3df3) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchConsoleOutSerialPortCustomIoBase(default 0, id 0x267e_3fb0) | pub get u16 : pub set u16,

        // Df

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS0L0(default 0x4444, id 0xf577_49d9) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS0L1(default 0x4444, id 0x6d17_233f) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS0L2(default 0x4444, id 0x2c73_3c17) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS0L3(default 0x4444, id 0xc49e_23e3) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS1L0(default 0x4444, id 0xdb75_063f) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS1L1(default 0x4444, id 0x98ba_12f7) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS1L2(default 0x4444, id 0x01a9_16bc) | pub get u16 : pub set u16,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitPresetS1L3(default 0x4444, id 0x416f_f232) | pub get u16 : pub set u16,

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfBelow4GbAreaSize(default 0x80, id 0x31f2_deef) | pub get u16 : pub set u16,


        // Unsorted Milan; obsolete and ungrouped; defaults wrong!

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        Dimm3DsSensorCritical(default 0, id 0x16b77f73) | pub get u16 : pub set u16, // value 0x50 // (Obsolete; added in Milan)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        Dimm3DsSensorUpper(default 0, id 0x2db877e4) | pub get u16 : pub set u16, // value 0x42 // (Obsolete; added in Milan)

        // Unsorted Rome; ungrouped; defaults wrong!

        EccSymbolSize(default 1, id 0x302d5c04) | pub get EccSymbolSize : pub set EccSymbolSize, // Rome (Obsolete)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubDramRate(default 0, id 0x9adddd6b) | pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16; or 0xff
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubL2Rate(default 0, id 0x2266c144) | pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubL3Rate(default 0, id 0xc0279ae0) | pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16; maybe 00h disable; maybe otherwise x: (x * 20 ns)
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubIcacheRate(default 0, id 0x99639ee4) | pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubDcacheRate(default 0, id 0xb398daa0) | pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        /// See for example MCP9843/98243
        /// DIMM temperature sensor register at address 1
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorConfig(default 0x408, id 0x51e7b610) | pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 2
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorUpper(default 80, id 0xb5af557a) | pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 3
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorLower(default 10, id 0xc5ea38a0) | pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 4
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorCritical(default 95, id 0x38e9bf5d) | pub get u16 : pub set u16, // Rome (Obsolete)

        // BMC Rome

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoPort(default 0, id 0x6e06198) | pub get u16 : pub set u16, // value 0 // legacy
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum DwordToken: {TokenEntryId::Dword} {
        // Psp

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PspOpnPlatformComptFusing(default 0x1, id 0x24b4_5ecd) | pub get u32 : pub set u32,

        // Memory Controller

        MemBusFrequencyLimit(default 1600, id 0x3497_0a3c) | pub get MemBusFrequencyLimit : pub set MemBusFrequencyLimit,
        MemClockValue(default 0xffff_ffff, id 0xcc83_f65f) | pub get MemClockValue : pub set MemClockValue,
        MemUserTimingMode(default 0xff, id 0xfc56_0d7d) | pub get MemUserTimingMode : pub set MemUserTimingMode,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSelfHealBistTimeout(default 1_0000, id 0xbe75_97d4) | pub get u32 : pub set u32, // in ms
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemRestoreValidDays(default 30, id 0x6bd7_0482) | pub get u32 : pub set u32,

        // Ccx

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CcxMinSevAsid(default 1, id 0xa7c3_3753) | pub get u32 : pub set u32,

        // Fch

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchRom3BaseHigh(default 0, id 0x3e7d_5274) | pub get u32 : pub set u32,

        // Df

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfPciMmioSize(default 0x1000_0000, id 0x3d9b_7d7b) | pub get u32 : pub set u32,
        DfCakeCrcThresholdBounds(default 100, id 0x9258_cf45) | pub get DfCakeCrcThresholdBounds : pub set DfCakeCrcThresholdBounds, // default: 0.001%

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L0P01(default 0x007a_007a, id 0xe53_519b1) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L0P23(default 0x007a_007a, id 0xc50_e790e) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L1P01(default 0x007a_007a, id 0x68c_aed33) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L1P23(default 0x007a_007a, id 0xaf5_6afaa) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L2P01(default 0x007a_007a, id 0xd6c_ad603) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L2P23(default 0x007a_007a, id 0x17e_59442) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L3P01(default 0x007a_007a, id 0x606_1edce) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS0L3P23(default 0x007a_007a, id 0x34d_bc7af) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L0P01(default 0x007a_007a, id 0xd32_408f4) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L0P23(default 0x007a_007a, id 0x524_3af4a) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L1P01(default 0x007a_007a, id 0x026_b4760) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L1P23(default 0x007a_007a, id 0x72b_f1cdf) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L2P01(default 0x007a_007a, id 0xc8b_848a9) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L2P23(default 0x007a_007a, id 0x0f3_a8f7f) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L3P01(default 0x007a_007a, id 0x656_6d661) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiTxEqS1L3P23(default 0x007a_007a, id 0x902_d0192) | pub get u32 : pub set u32,

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiPresetP11(default 0x3000, id 0x088b_9701) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiPresetP12(default 0x3000, id 0x5c22_c08a) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiPresetP13(default 0x3000, id 0x2753_0c2f) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiPresetP14(default 0x3000, id 0x76ec_f33f) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiPresetP15(default 0x3000, id 0x532e_b058) | pub get u32 : pub set u32,

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfXgmiInitialPreset(default 0x44444444, id 0xC6F86640) | pub get u32 : pub set u32,

        DfXgmiChannelTypeSelect(default 0x0, id 0x0db9_89c4) | pub get DfXgmiChannelTypeSelect : pub set DfXgmiChannelTypeSelect,

        // TODO: use better types to ensure 1MiB alignment & correct bounds. also, better defaults?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfPciMmioBaseLo(default 0xE0000000, id 0x7a4f_a9ed) | pub get u32 : pub set u32,
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfPciMmioBaseHi(default 0, id 0xf6b9_15bf) | pub get u32 : pub set u32,

        // Dxio

        DxioPhyParamVga(default 0xffff_ffff, id 0xde09_c43b) | pub get DxioPhyParamVga : pub set DxioPhyParamVga,
        DxioPhyParamPole(default 0xffff_ffff, id 0xb189_447e) | pub get DxioPhyParamPole : pub set DxioPhyParamPole,
        DxioPhyParamDc(default 0xffff_ffff, id 0x2066_7c30) | pub get DxioPhyParamDc : pub set DxioPhyParamDc,
        DxioPhyParamIqofc(default 0x7FFFFFFF, id 0x7e60_69c5) | pub get DxioPhyParamIqofc : pub set DxioPhyParamIqofc,

        // MBIST for Milan and Rome; defaults wrong!

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelLo(default 0, id 0x745218ad) | pub get u32 : pub set u32, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelHi(default 0, id 0xfac9f48f) | pub get u32 : pub set u32, // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelLo(default 0, id 0x81880d15) | pub get u32 : pub set u32, // value 0 // Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelHi(default 0, id 0xaf669f33) | pub get u32 : pub set u32, // value 0 // Rome

        // Unsorted Milan; defaults wrong!

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        GnbOffRampStall(default 0, id 0x88b3c0d4) | pub get u32 : pub set u32, // value 0xc8
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PspMeasureConfig(default 0, id 0xdd3ad029) | pub get u32 : pub set u32, // Milan; _reserved_, must be 0

        // Unsorted Rome; ungrouped; defaults wrong!

        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemPowerDownMode(default 0, id 0x23dd2705) | pub get u32 : pub set u32, // power_down_mode; Rome
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUmaSize(default 0, id 0x37b1f8cf) | pub get u32 : pub set u32, // uma_size; Rome // FIXME enum
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUmaAlignment(default 0, id 0x57ddf512) | pub get u32 : pub set u32, // value 0xffffc0 // Rome // FIXME enum?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PcieResetGpioPin(default 0, id 0x596663ac) | pub get u32 : pub set u32, // value 0xffffffff // Rome; FIXME: enum?
        #[cfg_attr(feature = "serde-hex", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CpuFetchFromSpiApBase(default 0, id 0xd403ea0e) | pub get u32 : pub set u32, // value 0xfff00000 // Rome

        GnbAdditionalFeatureOffRamp(default 40, id 0xe851_6128) | pub get u32 : pub set u32, // FIXME: nicer user type
        GnbAdditionalFeatureOnRamp(default 70, id 0x8d40_76dd) | pub get u32 : pub set u32, // FIXME: nicer user type
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum BoolToken: {TokenEntryId::Bool} {
        // PSP

        PspTpPort(default 1, id 0x0460_abe8) | pub get bool : pub set bool,
        PspErrorDisplay(default 1, id 0xdc33_ff21) | pub get bool : pub set bool,
        PspEventLogDisplay(default 0, id 0x0c47_3e1c) | pub get bool : pub set bool,
        PspStopOnError(default 0, id 0xe702_4a21) | pub get bool : pub set bool,
        PspPsbAutoFuse(default 1, id 0x2fcd_70c9) | pub get bool : pub set bool,
        PspSevMode(default 0, id 0xadc8_33e4) | pub get PspSevMode : pub set PspSevMode,
        PspSfsEnable(default 0, id 0x10b8_7c7d) | pub get bool : pub set bool,
        PspSfsCosignRequired(default 0, id 0xb364_f33f) | pub get bool : pub set bool,

        // Memory Controller

        MemEnableChipSelectInterleaving(default 1, id 0x6f81_a115) | pub get bool : pub set bool,
        MemEnableEccFeature(default 1, id 0xfa35_f040) | pub get bool : pub set bool,
        MemEnableParity(default 1, id 0x3cb8_cbd2) | pub get bool : pub set bool,
        MemEnableBankSwizzle(default 1, id 0x6414_d160) | pub get bool : pub set bool,
        MemIgnoreSpdChecksum(default 0, id 0x7d36_9dbc) | pub get bool : pub set bool,
        MemSpdReadOptimizationDdr4(default 1, id 0x6816_f949) | pub get bool : pub set bool,
        CbsMemSpdReadOptimizationDdr4(default 1, id 0x8d3a_b10e) | pub get bool : pub set bool,
        MemEnablePowerDown(default 1, id 0xbbb1_85a2) | pub get bool : pub set bool,
        MemTempControlledRefreshEnable(default 0, id 0xf051_e1c4) | pub get bool : pub set bool,
        MemOdtsCmdThrottleEnable(default 1, id 0xc073_6395) | pub get bool : pub set bool,
        MemSwCmdThrottleEnable(default 0, id 0xa29c_1cf9) | pub get bool : pub set bool,
        MemForcePowerDownThrottleEnable(default 1, id 0x1084_9d6c) | pub get bool : pub set bool,
        MemHoleRemapping(default 1, id 0x6a13_3ac5) | pub get bool : pub set bool,
        MemEnableBankGroupSwapAlt(default 1, id 0xa89d_1be8) | pub get bool : pub set bool,
        MemEnableBankGroupSwap(default 1, id 0x4692_0968) | pub get bool : pub set bool,
        MemDdrRouteBalancedTee(default 0, id 0xe68c_363d) | pub get bool : pub set bool,
        CbsMemWriteCrcRetryDdr4(default 0, id 0x25fb_6ea6) | pub get bool : pub set bool,
        CbsMemControllerWriteCrcEnableDdr4(default 0, id 0x9445_1a4b) | pub get bool : pub set bool,
        MemUncorrectedEccRetryDdr4(default 1, id 0xbff0_0125) | pub get bool : pub set bool,
        /// See Transparent Secure Memory Encryption in PPR
        MemTsmeModeMilan(default 1, id 0xd1fa_6660) | pub get bool : pub set bool,
        MemEccSyncFlood(default 0, id 0x88bd_40c2) | pub get bool : pub set bool,
        MemRestoreControl(default 0, id 0xfedb_01f8) | pub get bool : pub set bool,
        MemPostPackageRepairEnable(default 0, id 0xcdc0_3e4e) | pub get bool : pub set bool,
        MemDramDoubleRefreshRateMilan(default 0, id 0x974e_8e7c) | pub get bool : pub set bool, // Milan
        MemSmeMkEnable(default 0, id 0x07a5_db75) | pub get bool : pub set bool,
        MemTsmeEnable(default 0, id 0xf086_9eca) | pub get bool : pub set bool,

        // Ccx

        CcxPpinOptIn(default 0, id 0x6a67_00fd) | pub get bool : pub set bool,
        CcxCpuWatchdogEnable(default 1, id 0x7436_2038) | pub get bool : pub set bool,

        // Nbio

        NbioIommu(default 1, id 0x1ccc_9834) | pub get bool : pub set bool,

        // Df

        /// F17M30 needs it to be true
        DfGroupDPlatform(default 0, id 0x6831_8493) | pub get bool : pub set bool,
        DfPickerThrottleEnable(default 0, id 0x0bcb_d809) | pub get bool : pub set bool,
        DfNps1With4ChannelInterleavedRdimm4ChannelNonInterleavedNvdimm(default 0, id 0x9d6e_e05e) | pub get bool : pub set bool, // Milan
        DfCdma(default 0, id 0xd7d7_6f0c) | pub get bool : pub set bool,
        DfP2pOrderedWrites(default 0, id 0x0647_28ef) | pub get DfP2pOrderedWrites : pub set DfP2pOrderedWrites,

        // Dxio

        DxioVgaApiEnable(default 0, id 0xbd5a_a3c6) | pub get bool : pub set bool, // Milan

        // Fch

        FchEspiAblInitEnable(default 1, id 0x8795_8b5a) | pub get bool : pub set bool,
        // TODO(#121): WordToken::FchI2cSdaHoldOverrideMode & ByteToken::FchI2cSdaHoldOverrideMode2
        FchI2cSdaHoldOverride(default 0, id 0x545d_7662) | pub get bool : pub set bool,
        FchSpdControlOwnership(default 0, id 0x2329_5d81) | pub get FchSpdControlOwnershipMode : pub set FchSpdControlOwnershipMode,
        FchCmosClearEnableMemRestore(default 0, id 0x6a05_8ca4) | pub get bool : pub set bool,
        FchSataEnable(default 1, id 0x91b6_2b88) | pub get bool : pub set bool,
        FchSata0Enable(default 1, id 0xcaed_3265) | pub get bool : pub set bool,
        FchSata1Enable(default 1, id 0x87ff_12b7) | pub get bool : pub set bool,
        FchSata2Enable(default 1, id 0xa3b4_01f5) | pub get bool : pub set bool,
        FchSata3Enable(default 1, id 0x5495_4ba4) | pub get bool : pub set bool,
        FchSata4Enable(default 1, id 0xc336_f85f) | pub get bool : pub set bool,
        FchSata5Enable(default 1, id 0x81a7_9e2f) | pub get bool : pub set bool,
        FchSata6Enable(default 1, id 0x2cf9_f74d) | pub get bool : pub set bool,
        FchSata7Enable(default 1, id 0x8b79_4f2b) | pub get bool : pub set bool,
        FchSataMsi(default 1, id 0xb631_d11f) | pub get bool : pub set bool,
        FcbUsb0Enable(default 1, id 0x8804_70fa) | pub get bool : pub set bool,
        FchUsb1Enable(default 1, id 0xcbaa_700b) | pub get bool : pub set bool,
        FchUsb2Enable(default 0, id 0x784d_51d0) | pub get bool : pub set bool,
        FchUsb3Enable(default 0, id 0x3b6f_bb77) | pub get bool : pub set bool,
        FchPowerFailEarlyShadow(default 0, id 0x03da_06e0) | pub get bool : pub set bool,
        FchAblApcbBoardIdErrorHalt(default 0, id 0xb41f_1a20) | pub get bool : pub set bool,

        // Misc

        ConfigureSecondPcieLink(default 0, id 0x7142_8092) | pub get bool : pub set bool,
        PerformanceTracing(default 0, id 0xf27a_10f0) | pub get bool : pub set bool, // Milan
        DisplayPmuTrainingResults(default 0, id 0x9e36_a9d4) | pub get bool : pub set bool,
        ScanDumpDebugEnable(default 0, id 0x7595_65c9) | pub get bool : pub set bool,
        Mcax64BankEnable(default 0, id 0xedf6_737b) | pub get bool : pub set bool,

        // MBIST for Milan and Rome; defaults wrong!

        MemMbistAggressorStaticLaneControl(default 0, id 0x77e6f2c9) | pub get bool : pub set bool, // Rome
        MemMbistTgtStaticLaneControl(default 0, id 0xe1cc135e) | pub get bool : pub set bool, // Rome

        // Capabilities for Rome; defaults wrong!

        MemUdimmCapable(default 0, id 0x3cf8a8ec) | pub get bool : pub set bool, // Rome
        MemSodimmCapable(default 0, id 0x7c61c187) | pub get bool : pub set bool, // Rome
        MemRdimmCapable(default 0, id 0x81726666) | pub get bool : pub set bool, // Rome
        /// Leaving this off is very bad (does not boot anymore).
        MemLrdimmCapable(default 0, id 0x14fbf20) | pub get bool : pub set bool,
        /// Leaving this off is very bad (does not boot anymore).
        MemDimmTypeLpddr3Capable(default 0, id 0xad96aa30) | pub get bool : pub set bool, // Rome
        MemDimmTypeDdr3Capable(default 0, id 0x789210c) | pub get bool : pub set bool, // Rome
        MemQuadRankCapable(default 0, id 0xe6dfd3dc) | pub get bool : pub set bool, // Rome

        // Unsorted Milan; defaults wrong!

        /// Leaving this off is very bad (does not boot anymore).
        MemModeUnganged(default 0, id 0x3ce1180) | pub get bool : pub set bool,
        /// This is optional
        GnbAdditionalFeatures(default 0, id 0xf4c7789) | pub get bool : pub set bool, // Milan
        /// Note: Use GnbAdditionalFeatureDsm2 for Milan 1.0.0.4 or higher. For older Milan, use GnbAdditionalFeatureDsmDetector
        GnbAdditionalFeatureDsm(default 0, id 0x31a6afad) | pub get bool : pub set bool, // Milan < 1.0.0.4
        VgaProgram(default 0, id 0x6570eace) | pub get bool : pub set bool, // Milan
        MemNvdimmNDisable(default 0, id 0x941a92d4) | pub get bool : pub set bool, // Milan
        GnbAdditionalFeatureL3PerformanceBias(default 0, id 0xa003b37a) | pub get bool : pub set bool, // Milan
        /// Note: Use GnbAdditionalFeatureDsmDetector2 for Milan 1.0.0.4 or higher. For older Milan, use GnbAdditionalFeatureDsmDetector
        GnbAdditionalFeatureDsmDetector(minimal_version 0, frontier_version 0x1004_5012, default 0, id 0xf5768cee) | pub get bool : pub set bool, // Milan < 1.0.0.4

        // Unsorted Rome; ungrouped; defaults wrong!

        PcieResetControl(default 0, id 0xf7bb3451) | pub get bool : pub set bool, // Rome (Obsolete)
        MemDqsTrainingControl(default 0, id 0x3caaa3fa) | pub get bool : pub set bool, // Rome
        MemChannelInterleaving(default 0, id 0x48254f73) | pub get bool : pub set bool, // Rome
        MemPstate(default 0, id 0x56b93947) | pub get bool : pub set bool, // Rome
        /// Average the time between refresh requests
        MemAmp(default 0, id 0x592cb3ca) | pub get bool : pub set bool, // value 1 // amp_enable; Rome
        MemLimitMemoryToBelow1TiB(default 0, id 0x5e71e6d8) | pub get bool : pub set bool, // value 1 // Rome
        MemOcVddioControl(default 0, id 0x6cd36dbe) | pub get bool : pub set bool, // value 0 // Rome
        MemUmaAbove4GiB(default 0, id 0x77e41d2a) | pub get bool : pub set bool, // value 1 // Rome
        MemAutoRefreshsCountForThrottling(default 0, id 0x8f84dcb4) | pub get MemAutoRefreshsCountForThrottling : pub set MemAutoRefreshsCountForThrottling, // value 0 // Rome
        GeneralCapsuleMode(default 0, id 0x96176308) | pub get bool : pub set bool, // value 1 // Rome
        MemOnDieThermalSensor(default 0, id 0xaeb3f914) | pub get bool : pub set bool, // odts_en; Rome
        MemAllClocks(default 0, id 0xb95e0555) | pub get bool : pub set bool, // mem_all_clocks_on; Rome
        MemClear(default 0, id 0xc6acdb37) | pub get bool : pub set bool, // enable_mem_clr; Rome
        MemDdr4ForceDataMaskDisable(default 0, id 0xd68482b3) | pub get bool : pub set bool, // Rome
        MemEccRedirection(default 0, id 0xdede0e09) | pub get bool : pub set bool, // Rome
        MemTempControlledExtendedRefresh(default 0, id 0xf402f423) | pub get bool : pub set bool, // Rome (Obsolete)
        MotherBoardType0(default 0, id 0x536464b) | pub get bool : pub set bool, // value 0
        MctpRerouteEnable(default 0, id 0x79f2a8d5) | pub get bool : pub set bool, // value 0
        IohcMixedRwWorkaround(default 0, id 0xec3faf5a) | pub get bool : pub set bool, // value 0 // FIXME remove?

        // BMC Rome

        BmcVgaIoEnable(default 0, id 0x468d2cfa) | pub get bool : pub set bool, // value 0 // legacy
        BmcInitBeforeDram(default 0, id 0xfa94ee37) | pub get bool : pub set bool, // value 0

        // Other

        CmosClearTriggerApcbRecovery(default 1, id 0x854e_f3f3) | pub get bool : pub set bool,
        CmosClearEnableMemRestore(default 0, id 0x6a05_8ca4) | pub get bool : pub set bool,
        L3Bist(default 0, id 0x081c_2c0f) | pub get bool : pub set bool,
        GnbMpdmaTfEnable(default 0, id 0x35aa_5977) | pub get bool : pub set bool,
    }
}

// Compatibility shim for old token accessors (which we have a lot of
// configurations with)
impl<'a, 'b> Tokens<'a, 'b> {
    #[allow(non_snake_case)]
    pub fn mem_limit_memory_to_below_1_TiB(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x5e71e6d8)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    #[allow(non_snake_case)]
    pub fn mem_uma_above_4_GiB(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x77e41d2a)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_remap_at_1tib(&self) -> Result<DfRemapAt1TiB> {
        DfRemapAt1TiB::from_u32(self.get(TokenEntryId::Byte, 0x35ee_96f3)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_4link_max_xgmi_speed(&self) -> Result<DfXgmi4LinkMaxSpeed> {
        DfXgmi4LinkMaxSpeed::from_u32(self.get(TokenEntryId::Byte, 0x3f307cb3)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_3link_max_xgmi_speed(&self) -> Result<DfXgmi3LinkMaxSpeed> {
        DfXgmi3LinkMaxSpeed::from_u32(self.get(TokenEntryId::Byte, 0x53ba449b)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn mem_rdimm_timing_rcd_f0rc0f_additional_latency(
        &self,
    ) -> Result<MemRdimmTimingCmdParLatency> {
        MemRdimmTimingCmdParLatency::from_u32(
            self.get(TokenEntryId::Byte, 0xd155798a)?,
        )
        .ok_or(Error::EntryTypeMismatch)
    }
    pub fn dimm_3ds_sensor_critical(&self) -> Result<u16> {
        u16::from_u32(self.get(TokenEntryId::Word, 0x16b77f73)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn dimm_3ds_sensor_upper(&self) -> Result<u16> {
        u16::from_u32(self.get(TokenEntryId::Word, 0x2db877e4)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn mother_board_type_0(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x536464b)?)
            .ok_or(Error::EntryTypeMismatch)
    }
}

// Compatibility shim for old token accessors (which we have a lot of
// configurations with)
impl<'a, 'b> TokensMut<'a, 'b> {
    #[allow(non_snake_case)]
    pub fn mem_limit_memory_to_below_1_TiB(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x5e71e6d8)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    #[allow(non_snake_case)]
    pub fn mem_uma_above_4_GiB(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x77e41d2a)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_remap_at_1tib(&self) -> Result<DfRemapAt1TiB> {
        DfRemapAt1TiB::from_u32(self.get(TokenEntryId::Byte, 0x35ee_96f3)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_4link_max_xgmi_speed(&self) -> Result<DfXgmi4LinkMaxSpeed> {
        DfXgmi4LinkMaxSpeed::from_u32(self.get(TokenEntryId::Byte, 0x3f307cb3)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn df_3link_max_xgmi_speed(&self) -> Result<DfXgmi3LinkMaxSpeed> {
        DfXgmi3LinkMaxSpeed::from_u32(self.get(TokenEntryId::Byte, 0x53ba449b)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn mem_rdimm_timing_rcd_f0rc0f_additional_latency(
        &self,
    ) -> Result<MemRdimmTimingCmdParLatency> {
        MemRdimmTimingCmdParLatency::from_u32(
            self.get(TokenEntryId::Byte, 0xd155798a)?,
        )
        .ok_or(Error::EntryTypeMismatch)
    }
    pub fn dimm_3ds_sensor_critical(&self) -> Result<u16> {
        u16::from_u32(self.get(TokenEntryId::Word, 0x16b77f73)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn dimm_3ds_sensor_upper(&self) -> Result<u16> {
        u16::from_u32(self.get(TokenEntryId::Word, 0x2db877e4)?)
            .ok_or(Error::EntryTypeMismatch)
    }
    pub fn mother_board_type_0(&self) -> Result<bool> {
        bool::from_u32(self.get(TokenEntryId::Bool, 0x536464b)?)
            .ok_or(Error::EntryTypeMismatch)
    }

    #[allow(non_snake_case)]
    pub fn set_mem_limit_memory_to_below_1_TiB(
        &'_ mut self,
        value: bool,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Bool, 0x5e71e6d8, token_value)
    }
    #[allow(non_snake_case)]
    pub fn set_mem_uma_above_4_GiB(&'_ mut self, value: bool) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Bool, 0x77e41d2a, token_value)
    }
    pub fn set_df_remap_at_1tib(
        &'_ mut self,
        value: DfRemapAt1TiB,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Byte, 0x35ee_96f3, token_value)
    }
    pub fn set_df_4link_max_xgmi_speed(
        &'_ mut self,
        value: DfXgmi4LinkMaxSpeed,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Bool, 0x3f307cb3, token_value)
    }
    pub fn set_df_3link_max_xgmi_speed(
        &'_ mut self,
        value: DfXgmi3LinkMaxSpeed,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Bool, 0x53ba449b, token_value)
    }
    pub fn set_mem_rdimm_timing_rcd_f0rc0f_additional_latency(
        &'_ mut self,
        value: MemRdimmTimingCmdParLatency,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Byte, 0xd155798a, token_value)
    }
    pub fn set_dimm_3ds_sensor_critical(
        &'_ mut self,
        value: u16,
    ) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Word, 0x16b77f73, token_value)
    }
    pub fn set_dimm_3ds_sensor_upper(&'_ mut self, value: u16) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Word, 0x2db877e4, token_value)
    }
    pub fn set_mother_board_type_0(&'_ mut self, value: bool) -> Result<()> {
        let token_value = value.to_u32().unwrap();
        self.set(TokenEntryId::Bool, 0x536464b, token_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::const_assert;

    #[test]
    fn test_struct_sizes() {
        const_assert!(size_of::<V2_HEADER>() == 32);
        const_assert!(
            size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>() == 128
        );
        const_assert!(size_of::<V2_HEADER>() % ENTRY_ALIGNMENT == 0);
        const_assert!(size_of::<GROUP_HEADER>() == 16);
        const_assert!(size_of::<GROUP_HEADER>() % ENTRY_ALIGNMENT == 0);
        const_assert!(size_of::<ENTRY_HEADER>() == 16);
        const_assert!(size_of::<ENTRY_HEADER>() % ENTRY_ALIGNMENT == 0);
    }

    #[test]
    fn test_four_cc() {
        const_assert!(size_of::<FourCC>() == 4);
        assert!(FourCC(*b"APCB").0 == [0x41, 0x50, 0x43, 0x42]);
    }
}
