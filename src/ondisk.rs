// This file contains the Apcb on-disk format.  Please only change it in
// coordination with the AMD PSP team.  Even then, you probably shouldn't.

//#![feature(trace_macros)] trace_macros!(true);
#![allow(non_snake_case)]

pub use crate::naples::{ParameterTimePoint, ParameterTokenConfig};
use crate::struct_accessors::{make_accessors, Getter, Setter};
use crate::token_accessors::{make_token_accessors, Tokens, TokensMut};
use crate::types::Error;
use crate::types::PriorityLevel;
use crate::types::Result;
use byteorder::{LittleEndian, ReadBytesExt};
use core::clone::Clone;
use core::cmp::Ordering;
use core::convert::TryInto;
use core::mem::{size_of, take};
use core::num::NonZeroU8;
use modular_bitfield::prelude::*;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use paste::paste;
use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned, U16, U32, U64};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
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
    type TailArrayItemType<'de>: AsBytes
        + FromBytes
        + serde::de::Deserialize<'de>;
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. If the item
/// cannot be parsed, returns None and does not advance.
pub fn take_header_from_collection_mut<'a, T: Sized + FromBytes + AsBytes>(
    buf: &mut &'a mut [u8],
) -> Option<&'a mut T> {
    let xbuf = take(&mut *buf);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item.into_mut())
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. Assuming that
/// *BUF had been aligned to ALIGNMENT before the call, it also ensures that
/// *BUF is aligned to ALIGNMENT after the call. If the item cannot be parsed,
/// returns None and does not advance.
pub fn take_body_from_collection_mut<'a>(
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
pub fn take_header_from_collection<'a, T: Sized + FromBytes>(
    buf: &mut &'a [u8],
) -> Option<&'a T> {
    let xbuf = take(&mut *buf);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item.into_ref())
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the
/// items and returns it after advancing *BUF to the next item. Assuming that
/// *BUF had been aligned to ALIGNMENT before the call, it also ensures that
/// *BUF is aligned to ALIGNMENT after the call. If the item cannot be parsed,
/// returns None and does not advance.
pub fn take_body_from_collection<'a>(
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

#[derive(Default, Copy, Clone, FromPrimitive, ToPrimitive)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
pub struct SerdeHex8(u8);

#[cfg(feature = "std")]
impl serde::ser::Serialize for SerdeHex8 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerHex::<StrictPfx>::serialize(&self.0, serializer)
    }
}

#[cfg(feature = "std")]
impl<'de> serde::de::Deserialize<'de> for SerdeHex8 {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(SerHex::<StrictPfx>::deserialize(deserializer)?))
    }
}

#[derive(Default, Copy, Clone, FromPrimitive, ToPrimitive)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
pub struct SerdeHex16(u16);

#[cfg(feature = "std")]
impl serde::ser::Serialize for SerdeHex16 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerHex::<StrictPfx>::serialize(&self.0, serializer)
    }
}

#[cfg(feature = "std")]
impl<'de> serde::de::Deserialize<'de> for SerdeHex16 {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(SerHex::<StrictPfx>::deserialize(deserializer)?))
    }
}

#[derive(Default, Copy, Clone, FromPrimitive, ToPrimitive)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
pub struct SerdeHex32(u32);

#[cfg(feature = "std")]
impl serde::ser::Serialize for SerdeHex32 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerHex::<StrictPfx>::serialize(&self.0, serializer)
    }
}

#[cfg(feature = "std")]
impl<'de> serde::de::Deserialize<'de> for SerdeHex32 {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(SerHex::<StrictPfx>::deserialize(deserializer)?))
    }
}

#[derive(Default, Copy, Clone, FromPrimitive, ToPrimitive)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
pub struct SerdeHex64(u64);

#[cfg(feature = "std")]
impl serde::ser::Serialize for SerdeHex64 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerHex::<StrictPfx>::serialize(&self.0, serializer)
    }
}

#[cfg(feature = "std")]
impl<'de> serde::de::Deserialize<'de> for SerdeHex64 {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(SerHex::<StrictPfx>::deserialize(deserializer)?))
    }
}

impl From<u8> for SerdeHex8 {
    fn from(lu: u8) -> Self {
        Self(lu)
    }
}

impl From<SerdeHex8> for u8 {
    fn from(sh: SerdeHex8) -> Self {
        sh.0
    }
}

impl From<LU16> for SerdeHex16 {
    fn from(lu: LU16) -> Self {
        Self(lu.get())
    }
}

impl From<SerdeHex16> for LU16 {
    fn from(sh: SerdeHex16) -> Self {
        Self::new(sh.0)
    }
}

impl From<LU32> for SerdeHex32 {
    fn from(lu: LU32) -> Self {
        Self(lu.get())
    }
}

impl From<SerdeHex32> for LU32 {
    fn from(sh: SerdeHex32) -> Self {
        Self::new(sh.0)
    }
}

impl From<LU64> for SerdeHex64 {
    fn from(lu: LU64) -> Self {
        Self(lu.get())
    }
}

impl From<SerdeHex64> for LU64 {
    fn from(sh: SerdeHex64) -> Self {
        Self::new(sh.0)
    }
}

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

make_array_accessors!(SerdeHex8, u8);
make_array_accessors!(SerdeHex16, LU16);
make_array_accessors!(SerdeHex32, LU32);
make_array_accessors!(SerdeHex64, LU64);

make_accessors! {
    #[derive(FromBytes, AsBytes, Unaligned, Debug, Clone)]
    #[repr(C, packed)]
    pub struct V2_HEADER {
        pub signature: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],
        pub header_size: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // == sizeof(V2_HEADER); but 128 for V3
        pub version: LU16 : pub get SerdeHex16 : pub set SerdeHex16,     // == 0x30
        pub apcb_size: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
        pub unique_apcb_instance: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
        pub checksum_byte: u8 : pub get SerdeHex8 : pub set SerdeHex8,
        reserved1: [u8; 3],                // 0
        reserved2: [LU32; 3], // 0
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
            reserved1: [0, 0, 0],
            reserved2: [0u32.into(); 3],
        }
    }
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
        FromBytes, AsBytes, Unaligned, Clone, Copy,
    )]
    #[repr(C, packed)]
    pub struct V3_HEADER_EXT {
        pub signature: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],
        reserved_1: LU16,          // 0
        reserved_2: LU16, // 0x10 // GROUP_HEADER::header_size
        pub struct_version: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // GROUP_HEADER::version
        pub data_version: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // GROUP_HEADER::_reserved
        pub ext_header_size: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // 96 // GROUP_HEADER::group_size
        reserved_3: LU16, // 0 // ENTRY_HEADER::group_id
        reserved_4: LU16, // 0xFFFF // ENTRY_HEADER::entry_id
        // reserved_5 includes 0x10 for the entry header; that
        // gives 48 Byte payload for the entry--which is inconsistent with the
        // size of the fields following afterwards.
        reserved_5: LU16, // 0x40 // ENTRY_HEADER::entry_size
        reserved_6: LU16, // 0x00 // ENTRY_HEADER::instance_id
        // ENTRY_HEADER::{context_type, context_format, unit_size, prority_mask,
        // key_size, key_pos, board_instance_mask}.  Those are all of
        // ENTRY_HEADER's fields.
        reserved_7: [LU32; 2], // 0 0
        pub data_offset: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // 0x58
        pub header_checksum: u8: pub get SerdeHex8 : pub set SerdeHex8,
        reserved_8: u8,                     // 0
        reserved_9: [LU32; 3], // 0 0 0
        pub integrity_sign: [u8; 32] : pub get [SerdeHex8; 32] : pub set [SerdeHex8; 32],
        reserved_10: [LU32; 3], // 0 0 0
        pub signature_ending: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],       // "BCBA"
    }
}

impl Default for V3_HEADER_EXT {
    fn default() -> Self {
        Self {
            signature: *b"ECB2",
            reserved_1: 0u16.into(),
            reserved_2: 0x10u16.into(),
            struct_version: 0x12u16.into(),
            data_version: 0x100u16.into(),
            ext_header_size: (size_of::<Self>() as u32).into(),
            reserved_3: 0u16.into(),
            reserved_4: 0xFFFFu16.into(),
            reserved_5: 0x40u16.into(),
            reserved_6: 0x00u16.into(),
            reserved_7: [0u32.into(); 2],
            data_offset: 0x58u16.into(),
            header_checksum: 0, // invalid--but unused by AMD Rome
            reserved_8: 0,
            reserved_9: [0u32.into(); 3],
            integrity_sign: [0; 32], // invalid--but unused by AMD Rome
            reserved_10: [0u32.into(); 3],
            signature_ending: *b"BCBA",
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

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
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

    Ps3dsRdimmDdr4MaxFreq,
    Ps3dsRdimmDdr4StretchFreq,
    Ps3dsRdimmDdr4DataBus,

    ConsoleOutControl,
    EventControl,
    ErrorOutControl,
    ExtVoltageControl,

    PsLrdimmDdr4OdtPat,
    PsLrdimmDdr4CadBus,
    PsLrdimmDdr4DataBus,
    PsLrdimmDdr4MaxFreq,
    PsLrdimmDdr4StretchFreq,

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

            Self::Ps3dsRdimmDdr4MaxFreq => 0x4B,
            Self::Ps3dsRdimmDdr4StretchFreq => 0x4C,
            Self::Ps3dsRdimmDdr4DataBus => 0x4D,

            Self::ConsoleOutControl => 0x50,
            Self::EventControl => 0x51,
            Self::ErrorOutControl => 0x52,
            Self::ExtVoltageControl => 0x53,

            Self::PsLrdimmDdr4OdtPat => 0x54,
            Self::PsLrdimmDdr4CadBus => 0x55,
            Self::PsLrdimmDdr4DataBus => 0x56,
            Self::PsLrdimmDdr4MaxFreq => 0x57,
            Self::PsLrdimmDdr4StretchFreq => 0x58,

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
    Unknown(u16),
}

impl ToPrimitive for GnbEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x09,
            Self::Parameters => 0x0A,
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

    Unknown(u16),
}

impl ToPrimitive for FchEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::DefaultParameters => 0x0B,
            Self::Parameters => 0x0C,
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
        if value < 0x1_0000 {
            Some(match value {
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
        if value < 0x1_0000 {
            Some(match value {
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum TokenEntryId {
    Bool,
    Byte,
    Word,
    Dword,
    Unknown(u16),
}

#[cfg(feature = "std")]
use std::fmt::{Formatter, Result as FResult};
#[cfg(feature = "std")]
impl serde::de::Expected for TokenEntryId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FResult {
        write!(f, "{:?}", self)
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
        FromBytes, AsBytes, Unaligned, Clone, Debug,
    )]
    #[repr(C, packed)]
    pub(crate) struct GROUP_HEADER {
        pub(crate) signature: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],
        pub(crate) group_id: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
        pub(crate) header_size: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // == sizeof(GROUP_HEADER)
        pub(crate) version: LU16 : pub get SerdeHex16 : pub set SerdeHex16,     // == 0 << 4 | 1
        _reserved: LU16,
        pub(crate) group_size: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // including header!
    }
}

#[derive(
    FromPrimitive,
    ToPrimitive,
    Debug,
    PartialEq,
    Copy,
    Clone,
    Serialize,
    Deserialize,
)]
#[non_exhaustive]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
pub enum ContextFormat {
    Raw = 0,
    SortAscending = 1,  // (sort by unit size)
    SortDescending = 2, // don't use
}

#[derive(
    FromPrimitive,
    ToPrimitive,
    Debug,
    PartialEq,
    Copy,
    Clone,
    Serialize,
    Deserialize,
)]
#[non_exhaustive]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
            _reserved: 0u16.into(),
            group_size: (size_of::<Self>() as u32).into(), // probably invalid
        }
    }
}

macro_rules! make_bitfield_serde {(
        $(#[$struct_meta:meta])*
        $struct_vis:vis
        struct $StructName:ident {
                $(
                        $(#[$field_meta:meta])*
                        $field_vis:vis
                        $field_name:ident : $field_ty:ty $(: $getter_vis:vis get $field_user_ty:ty $(: $setter_vis:vis set $field_setter_user_ty:ty)?)?
                ),* $(,)?
        }
) => {
    $(#[$struct_meta])*
    $struct_vis
    struct $StructName {
        $(
            $(#[$field_meta])*
            $field_vis
            $field_name : $field_ty,
        )*
    }

    impl $StructName {
        pub fn build(&self) -> Self {
            self.clone()
        }
    }

    paste::paste! {
        #[derive(serde::Deserialize, serde::Serialize)]
        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
        //#[serde(remote = "" $StructName)]
        pub(crate) struct [<Serde $StructName>] {
            $(
                $(
                    $getter_vis
                    //pub(crate)
                    $field_name : <$field_ty as Specifier>::InOut, // $field_user_ty
                )?
            )*
        }
    }
}}

make_bitfield_serde! {
    #[bitfield(bits = 8)]
    #[repr(u8)]
    #[derive(Copy, Clone)]
    pub struct PriorityLevels {
        pub hard_force: bool : pub get bool : pub set bool,
        pub high: bool : pub get bool : pub set bool,
        pub medium: bool : pub get bool : pub set bool,
        pub event_logging: bool : pub get bool : pub set bool,
        pub low: bool : pub get bool : pub set bool,
        pub normal: bool : pub get bool : pub set bool,
        #[skip]
        __: B2,
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
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct BoardInstances {
        pub instance_0: bool : pub get bool : pub set bool,
        pub instance_1: bool : pub get bool : pub set bool,
        pub instance_2: bool : pub get bool : pub set bool,
        pub instance_3: bool : pub get bool : pub set bool,
        pub instance_4: bool : pub get bool : pub set bool,
        pub instance_5: bool : pub get bool : pub set bool,
        pub instance_6: bool : pub get bool : pub set bool,
        pub instance_7: bool : pub get bool : pub set bool,
        pub instance_8: bool : pub get bool : pub set bool,
        pub instance_9: bool : pub get bool : pub set bool,
        pub instance_10: bool : pub get bool : pub set bool,
        pub instance_11: bool : pub get bool : pub set bool,
        pub instance_12: bool : pub get bool : pub set bool,
        pub instance_13: bool : pub get bool : pub set bool,
        pub instance_14: bool : pub get bool : pub set bool,
        pub instance_15: bool : pub get bool : pub set bool,
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

make_accessors! {
    #[derive(FromBytes, AsBytes, Unaligned, Clone, Debug)]
    #[repr(C, packed)]
    pub(crate) struct ENTRY_HEADER {
        pub(crate) group_id: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // should be equal to the group's group_id
        pub(crate) entry_id: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // meaning depends on context_type
        pub(crate) entry_size: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // including header
        pub(crate) instance_id: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
        pub(crate) context_type: u8 : pub get ContextType : pub set ContextType,  // see ContextType enum
        pub(crate) context_format: u8 : pub get ContextFormat: pub set ContextFormat, // see ContextFormat enum
        pub(crate) unit_size: u8 : pub get SerdeHex8 : pub set SerdeHex8, // in Byte.  Applicable when ContextType == 2.  value should be 8
        pub(crate) priority_mask: u8 : pub get PriorityLevels : pub set PriorityLevels,
        pub(crate) key_size: u8 : pub get SerdeHex8 : pub set SerdeHex8, // Sorting key size; <= unit_size. Applicable when ContextFormat = 1. (or != 0)
        pub(crate) key_pos: u8 : pub get SerdeHex8 : pub set SerdeHex8, // Sorting key position of the unit specified of UnitSize
        pub(crate) board_instance_mask: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // Board-specific Apcb instance mask
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

#[derive(FromBytes, AsBytes, Clone)]
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

/// For Naples.
#[bitfield(bits = 32)]
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub struct ParameterAttributes {
    #[bits = 8]
    pub time_point: ParameterTimePoint,
    #[bits = 13]
    pub token: ParameterTokenConfig,
    #[bits = 3]
    pub size_minus_one: B3,
    #[skip]
    __: B8,
}

impl_bitfield_primitive_conversion!(
    ParameterAttributes,
    0b1111_1111_1111_1111_1111_1111,
    u32
);

impl ParameterAttributes {
    pub fn size(&self) -> usize {
        (self.size_minus_one() as usize) + 1
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
    pub(crate) fn next_attributes<'b>(
        buf: &mut &'b [u8],
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
}

impl Iterator for ParametersIter<'_> {
    type Item = (ParameterAttributes, u64);
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let attributes = Self::next_attributes(&mut self.keys).ok()?;
        if attributes.token() == ParameterTokenConfig::Limit {
            return None;
        }
        let size = attributes.size();
        match size {
            1 | 2 | 4 | 8 => {
                let raw_value =
                    take_body_from_collection(&mut self.values, size, 1)?;
                let mut raw_value = raw_value;
                match raw_value.len() {
                    1 => Some((attributes, raw_value.read_u8().ok()?.into())),
                    2 => Some((
                        attributes,
                        raw_value.read_u16::<LittleEndian>().ok()?.into(),
                    )),
                    4 => Some((
                        attributes,
                        raw_value.read_u32::<LittleEndian>().ok()?.into(),
                    )),
                    8 => Some((
                        attributes,
                        raw_value.read_u64::<LittleEndian>().ok()?.into(),
                    )),
                    _ => None, // TODO: Raise error
                }
            }
            _ => None,
        }
    }
}

/// For Naples.
#[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
#[repr(C, packed)]
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

pub mod df {
    use super::*;
    use crate::struct_accessors::{make_accessors, Getter, Setter};
    use crate::types::Result;

    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[non_exhaustive]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum SlinkRegionInterleavingSize {
        _256B = 0,
        _512B = 1,
        _1024B = 2,
        _2048B = 3,
        Auto = 7,
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct SlinkRegion {
            size: LU64 : pub get SerdeHex64 : pub set SerdeHex64,
            alignment: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            socket: u8 : pub get SerdeHex8 : pub set SerdeHex8, // 0|1
            phys_nbio_map: u8 : pub get SerdeHex8 : pub set SerdeHex8, // bitmap
            interleaving: u8 : pub get SlinkRegionInterleavingSize : pub set SlinkRegionInterleavingSize,
            _reserved: [u8; 4],
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
                _reserved: [0; 4],
            }
        }
    }

    impl SlinkRegion {
        // Not sure why AMD still uses those--but it does use them, so whatever.
        pub fn dummy(socket: u8) -> SlinkRegion {
            SlinkRegion {
                socket,
                ..Self::default()
            }
        }
    }

    // Rome only; even there, it's almost all 0s
    #[derive(
        FromBytes,
        AsBytes,
        Unaligned,
        PartialEq,
        Debug,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[repr(C, packed)]
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
        make_accessors, DummyErrorChecks, Getter, Setter, BLU16, BU8,
    };
    use crate::types::Result;

    make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone,
    Serialize, Deserialize)]
            #[repr(C, packed)]
            pub struct DimmInfoSmbusElement {
                dimm_slot_present: BU8 : pub get bool, // if false, it's soldered-down and not a slot
                #[serde(with = "SerHex::<StrictPfx>")]
                socket_id: u8 : pub get u8 : pub set u8,
                #[serde(with = "SerHex::<StrictPfx>")]
                channel_id: u8 : pub get u8 : pub set u8,
                #[serde(with = "SerHex::<StrictPfx>")]
                dimm_id: u8 : pub get u8 : pub set u8,
                // For soldered-down DIMMs, SPD data is hardcoded in APCB (in entry MemoryEntryId::SpdInfo), and DIMM_SMBUS_ADDRESS here is the index in the structure for SpdInfo.
                #[serde(with = "SerHex::<StrictPfx>")]
                dimm_smbus_address: u8,
                #[serde(with = "SerHex::<StrictPfx>")]
                i2c_mux_address: u8,
                #[serde(with = "SerHex::<StrictPfx>")]
                mux_control_address: u8,
                #[serde(with = "SerHex::<StrictPfx>")]
                mux_channel: u8,
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct AblConsoleOutControl {
            enable_console_logging: BU8 : pub get bool : pub set bool,
            enable_mem_flow_logging: BU8 : pub get bool : pub set bool,
            enable_mem_setreg_logging: BU8 : pub get bool : pub set bool,
            enable_mem_getreg_logging: BU8 : pub get bool : pub set bool,
            enable_mem_status_logging: BU8 : pub get bool : pub set bool,
            enable_mem_pmu_logging: BU8 : pub get bool : pub set bool,
            enable_mem_pmu_sram_read_logging: BU8 : pub get bool : pub set bool,
            enable_mem_pmu_sram_write_logging: BU8 : pub get bool : pub set bool,
            enable_mem_test_verbose_logging: BU8 : pub get bool : pub set bool,
            enable_mem_basic_output_logging: BU8 : pub get bool : pub set bool,
            _reserved: LU16,
            abl_console_port: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
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
                _reserved: 0u16.into(),
                abl_console_port: 0x80u32.into(),
            }
        }
    }

    impl AblConsoleOutControl {
        pub fn builder() -> Self {
            Self::default()
        }
        pub fn new() -> Self {
            Self::default()
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct AblBreakpointControl {
            enable_breakpoint: BU8 : pub get bool : pub set bool,
            break_on_all_dies: BU8 : pub get bool : pub set bool,
        }
    }
    impl Default for AblBreakpointControl {
        fn default() -> Self {
            Self {
                enable_breakpoint: BU8(1),
                break_on_all_dies: BU8(1),
            }
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

    #[derive(
        FromBytes,
        AsBytes,
        Unaligned,
        PartialEq,
        Debug,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[repr(C, packed)]
    pub struct ConsoleOutControl {
        pub abl_console_out_control: AblConsoleOutControl,
        pub abl_breakpoint_control: AblBreakpointControl,
        #[serde(skip)]
        _reserved: LU16,
    }

    impl Default for ConsoleOutControl {
        fn default() -> Self {
            Self {
                abl_console_out_control: AblConsoleOutControl::default(),
                abl_breakpoint_control: AblBreakpointControl::default(),
                _reserved: 0.into(),
            }
        }
    }

    impl EntryCompatible for ConsoleOutControl {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::ConsoleOutControl)
            )
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
                _reserved: 0.into(),
            }
        }
    }

    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum PortType {
        PcieHt0 = 0,
        PcieHt1 = 2,
        PcieMmio = 5,
        FchHtIo = 6,
        FchMmio = 7,
    }
    impl Default for PortType {
        fn default() -> Self {
            PortType::FchHtIo
        }
    }

    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum PortSize {
        _8Bit = 1,
        _16Bit = 2,
        _32Bit = 4,
    }
    impl Default for PortSize {
        fn default() -> Self {
            PortSize::_32Bit
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct ExtVoltageControl {
            enabled: BU8 : pub get bool : pub set bool,
            _reserved: [u8; 3],
            input_port: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            output_port: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            input_port_size: LU32 : pub get PortSize : pub set PortSize,
            output_port_size: LU32 : pub get PortSize : pub set PortSize,
            input_port_type: LU32 : pub get PortType : pub set PortType, // default: 6 (FCH)
            output_port_type: LU32 : pub get PortType : pub set PortType, // default: 6 (FCH)
            clear_acknowledgement: BU8 : pub get bool : pub set bool,
            _reserved_2: [u8; 3],
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
                _reserved: [0; 3],
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
            input_port: SerdeHex32,
            input_port_size: PortSize,
            output_port_type: PortType,
            output_port: SerdeHex32,
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
        #[bitfield(bits = 4)]
        #[derive(
            Default, Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct Ddr4DimmRanks {
            pub unpopulated: bool : pub get bool : pub set bool,
            pub single_rank: bool : pub get bool : pub set bool,
            pub dual_rank: bool : pub get bool : pub set bool,
            pub quad_rank: bool : pub get bool : pub set bool,
        }
    );
    impl DummyErrorChecks for Ddr4DimmRanks {}
    impl Ddr4DimmRanks {
        pub fn builder() -> Self {
            Self::new()
        }
    }

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
        #[bitfield(bits = 4)]
        #[derive(
            Clone, Copy, PartialEq, BitfieldSpecifier,
        )]
        pub struct LrdimmDdr4DimmRanks {
            pub unpopulated: bool : pub get bool : pub set bool,
            pub lr: bool : pub get bool : pub set bool,
            #[skip]
            __: B2,
        }
    );

    impl DummyErrorChecks for LrdimmDdr4DimmRanks {}
    impl Default for LrdimmDdr4DimmRanks {
        fn default() -> Self {
            Self::new()
        }
    }
    impl LrdimmDdr4DimmRanks {
        pub fn builder() -> Self {
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

    #[derive(
        Clone,
        Copy,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Serialize,
        Deserialize,
    )]
    #[non_exhaustive]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum CadBusClkDriveStrength {
        Auto = 0xFF,
        _120Ohm = 0,
        _60Ohm = 1,
        _40Ohm = 3,
        _30Ohm = 7,
        _24Ohm = 15,
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
            #[skip]
            __: bool,
            #[skip]
            __: bool,
            #[skip]
            __: bool,
            pub ddr400: bool : pub get bool : pub set bool, // @3
            pub ddr533: bool : pub get bool : pub set bool, // @4
            pub ddr667: bool : pub get bool : pub set bool, // @5
            pub ddr800: bool : pub get bool : pub set bool, // @6
            #[skip]
            __: bool,
            pub ddr1066: bool : pub get bool : pub set bool, // @8
            #[skip]
            __: bool,
            pub ddr1333: bool : pub get bool : pub set bool, // @10
            #[skip]
            __: bool,
            pub ddr1600: bool : pub get bool : pub set bool, // @12
            #[skip]
            __: bool,
            pub ddr1866: bool : pub get bool : pub set bool, // @14
            #[skip]
            __: bool, // AMD-missing pub ddr2100 // @15
            pub ddr2133: bool : pub get bool : pub set bool, // @16
            #[skip]
            __: bool,
            pub ddr2400: bool : pub get bool : pub set bool, // @18
            #[skip]
            __: bool,
            pub ddr2667: bool : pub get bool : pub set bool, // @20
            #[skip]
            __: bool, // AMD-missing pub ddr2800 // @21
            pub ddr2933: bool : pub get bool : pub set bool, // @22
            #[skip]
            __: bool, // AMD-missing pub ddr3066: // @23
            pub ddr3200: bool : pub get bool : pub set bool, // @24
            #[skip]
            __: bool, // @25
            #[skip]
            __: bool, // @26
            #[skip]
            __: bool, // @27
            #[skip]
            __: bool, // @28
            #[skip]
            __: bool, // @29
            #[skip]
            __: bool, // @30
            #[skip]
            __: bool, /* @31
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
    impl Default for DdrRates {
        fn default() -> Self {
            Self::new()
        }
    }
    impl DdrRates {
        pub fn builder() -> Self {
            Self::new()
        }
    }
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
            pub one_dimm: bool : pub get bool : pub set bool,
            pub two_dimms: bool : pub get bool : pub set bool,
            pub three_dimms: bool : pub get bool : pub set bool,
            pub four_dimms: bool : pub get bool : pub set bool,
            #[skip]
            __: B28,
        }
    }

    impl Default for DimmsPerChannelSelector {
        fn default() -> Self {
            Self::new()
        }
    }

    impl_bitfield_primitive_conversion!(DimmsPerChannelSelector, 0b1111, u32);
    impl DimmsPerChannelSelector {
        pub fn builder() -> Self {
            Self::new()
        }
    }

    #[derive(Clone, Copy, Serialize, Deserialize)]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
            if raw_value >= 0 {
                Self::from_u64(raw_value as u64)
            } else {
                None
            }
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
            pub v_1_2: bool : pub get bool : pub set bool,
            #[skip]
            __: B31,
            // all = 7
        }
    }
    impl_bitfield_primitive_conversion!(RdimmDdr4Voltages, 0b1, u32);
    impl RdimmDdr4Voltages {
        pub fn builder() -> Self {
            Self::new()
        }
        pub fn default() -> Self {
            let mut r = Self::new();
            r.set_v_1_2(true);
            r
        }
    }

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct RdimmDdr4CadBusElement {
            dimm_slots_per_channel: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            ddr_rates: LU32 : pub get DdrRates : pub set DdrRates,
            vdd_io: LU32 : pub get RdimmDdr4Voltages : pub set RdimmDdr4Voltages,
            dimm0_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            gear_down_mode: BLU16 : pub get bool : pub set bool,
            _reserved: LU16,
            slow_mode: BLU16 : pub get bool : pub set bool, // (probably) 2T is slow, 1T is fast
            _reserved_2: LU16,
            address_command_control: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
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
                _reserved: 0.into(),
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
            pub v_1_5: bool : pub get bool : pub set bool,
            pub v_1_35: bool : pub get bool : pub set bool,
            pub v_1_25: bool : pub get bool : pub set bool,
            // all = 7
            #[skip]
            __: B29,
        }
    }
    impl_bitfield_primitive_conversion!(UdimmDdr4Voltages, 0b111, u32);
    impl UdimmDdr4Voltages {
        pub fn builder() -> Self {
            Self::new()
        }
        pub fn default() -> Self {
            Self::new()
        }
    }

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct UdimmDdr4CadBusElement {
            dimm_slots_per_channel: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            ddr_rates: LU32 : pub get DdrRates : pub set DdrRates,
            vdd_io: LU32 : pub get UdimmDdr4Voltages : pub set UdimmDdr4Voltages,
            dimm0_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            gear_down_mode: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            _reserved: LU16,
            slow_mode: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            _reserved_2: LU16,
            address_command_control: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
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
                _reserved: 0.into(),
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

    impl UdimmDdr4CadBusElement {
        pub fn builder() -> Self {
            Self::default()
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
            pub v_1_2: bool: pub get bool : pub set bool,
            // all = 7
            #[skip]
            __: B31,
        }
    }
    impl_bitfield_primitive_conversion!(LrdimmDdr4Voltages, 0b1, u32);
    impl LrdimmDdr4Voltages {
        pub fn builder() -> Self {
            Self::new()
        }
        pub fn default() -> Self {
            let mut lr = Self::new();
            lr.set_v_1_2(true);
            lr
        }
    }

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4CadBusElement {
            dimm_slots_per_channel: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            ddr_rates: LU32 : pub get DdrRates : pub set DdrRates,
            vdd_io: LU32 : pub get LrdimmDdr4Voltages : pub set LrdimmDdr4Voltages,
            dimm0_ranks: LU32 : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks: LU32 : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,

            gear_down_mode: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            _reserved: LU16,
            slow_mode: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            _reserved_2: LU16,
            address_command_control: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get CadBusCkeDriveStrength : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get CadBusCsOdtDriveStrength : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get CadBusAddressCommandDriveStrength : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get CadBusClkDriveStrength : pub set CadBusClkDriveStrength,
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
                _reserved: 0.into(),
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
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4CadBus) => true,
                _ => false,
            }
        }
    }

    // Those are all divisors of 240
    // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 3
    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum RttNom {
        Off = 0,
        _60Ohm = 1,
        _120Ohm = 2,
        _40Ohm = 3,
        _240Ohm = 4,
        _48Ohm = 5,
        _80Ohm = 6,
        _34Ohm = 7,
    }
    // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 11
    pub type RttPark = RttNom;
    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum RttWr {
        Off = 0,
        _120Ohm = 1,
        _240Ohm = 2,
        Floating = 3,
        _80Ohm = 4,
    }

    #[derive(
        FromPrimitive, ToPrimitive, Clone, Copy, Serialize, Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum VrefDqRange1 {
        _60_00P = 0b00_0000,
        _60_65P = 0b00_0001,
        _61_30P = 0b00_0010,
        _61_95P = 0b00_0011,
        _62_60P = 0b00_0100,
        _63_25P = 0b00_0101,
        _63_90P = 0b00_0110,
        _64_55P = 0b00_0111,
        _65_20P = 0b00_1000,
        _65_85P = 0b00_1001,
        _66_50P = 0b00_1010,
        _67_15P = 0b00_1011,
        _67_80P = 0b00_1100,
        _68_45P = 0b00_1101,
        _69_10P = 0b00_1110,
        _69_75P = 0b00_1111,
        _70_40P = 0b01_0000,
        _71_05P = 0b01_0001,
        _71_70P = 0b01_0010,
        _72_35P = 0b01_0011,
        _73_00P = 0b01_0100,
        _73_65P = 0b01_0101,
        _74_30P = 0b01_0110,
        _74_95P = 0b01_0111,
        _75_60P = 0b01_1000,
        _76_25P = 0b01_1001,
        _76_90P = 0b01_1010,
        _77_55P = 0b01_1011,
        _78_20P = 0b01_1100,
        _78_85P = 0b01_1101,
        _79_50P = 0b01_1110,
        _80_15P = 0b01_1111,
        _80_80P = 0b10_0000,
        _81_45P = 0b10_0001,
        _82_10P = 0b10_0010,
        _82_75P = 0b10_0011,
        _83_40P = 0b10_0100,
        _84_05P = 0b10_0101,
        _84_70P = 0b10_0110,
        _85_35P = 0b10_0111,
        _86_00P = 0b10_1000,
        _86_65P = 0b10_1001,
        _87_30P = 0b10_1010,
        _87_95P = 0b10_1011,
        _88_60P = 0b10_1100,
        _89_25P = 0b10_1101,
        _89_90P = 0b10_1110,
        _90_55P = 0b10_1111,
        _91_20P = 0b11_0000,
        _91_85P = 0b11_0001,
        _92_50P = 0b11_0010,
    }

    #[derive(
        FromPrimitive, ToPrimitive, Clone, Copy, Serialize, Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub enum VrefDqRange2 {
        _45_00P = 0b00_0000,
        _45_65P = 0b00_0001,
        _46_30P = 0b00_0010,
        _46_95P = 0b00_0011,
        _47_60P = 0b00_0100,
        _48_25P = 0b00_0101,
        _48_90P = 0b00_0110,
        _49_55P = 0b00_0111,
        _50_20P = 0b00_1000,
        _50_85P = 0b00_1001,
        _51_50P = 0b00_1010,
        _52_15P = 0b00_1011,
        _52_80P = 0b00_1100,
        _53_45P = 0b00_1101,
        _54_10P = 0b00_1110,
        _54_75P = 0b00_1111,
        _55_40P = 0b01_0000,
        _56_05P = 0b01_0001,
        _56_70P = 0b01_0010,
        _57_35P = 0b01_0011,
        _58_00P = 0b01_0100,
        _58_65P = 0b01_0101,
        _59_30P = 0b01_0110,
        _59_95P = 0b01_0111,
        _60_60P = 0b01_1000,
        _61_25P = 0b01_1001,
        _61_90P = 0b01_1010,
        _62_55P = 0b01_1011,
        _63_20P = 0b01_1100,
        _63_85P = 0b01_1101,
        _64_50P = 0b01_1110,
        _65_15P = 0b01_1111,
        _65_80P = 0b10_0000,
        _66_45P = 0b10_0001,
        _67_10P = 0b10_0010,
        _67_75P = 0b10_0011,
        _68_40P = 0b10_0100,
        _69_05P = 0b10_0101,
        _69_70P = 0b10_0110,
        _70_35P = 0b10_0111,
        _71_00P = 0b10_1000,
        _71_65P = 0b10_1001,
        _72_30P = 0b10_1010,
        _72_95P = 0b10_1011,
        _73_60P = 0b10_1100,
        _74_25P = 0b10_1101,
        _74_90P = 0b10_1110,
        _75_55P = 0b10_1111,
        _76_20P = 0b11_0000,
        _76_85P = 0b11_0001,
        _77_50P = 0b11_0010,
    }

    /// VrefDq can be set to either Range1 (between 60% and 92.5% of VDDQ) or
    /// Range2 (between 45% and 77.5% of VDDQ). Range1 is intended for
    /// module-based systems, while Range2 is intended for point-to-point-based
    /// systems. In each range, Vref can be adjusted in steps of 0.65% VDDQ.
    #[derive(Serialize, Deserialize)]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr4DataBusElement {
            dimm_slots_per_channel: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            ddr_rates: LU32 : pub get DdrRates : pub set DdrRates,
            vdd_io: LU32 : pub get RdimmDdr4Voltages : pub set RdimmDdr4Voltages,
            dimm0_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,
            dimm1_ranks: LU32 : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks,

            rtt_nom: LU32 : pub get RttNom : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr: LU32 : pub get RttWr : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park: LU32 : pub get RttPark : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for data
            dqs_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for data strobe (bit clock)
            odt_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for on-die termination
            pmu_phy_vref: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            // See <https://github.com/LongJohnCoder/ddr-doc/blob/gh-pages/jedec/JESD79-4.pdf> Table 15
            pub(crate) vref_dq: LU32 : pub get VrefDq : pub set VrefDq, // MR6 vref calibration value; 23|30|32
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4DataBusElement {
            dimm_slots_per_channel: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            ddr_rates: LU32 : pub get DdrRates : pub set DdrRates,
            vdd_io: LU32 : pub get LrdimmDdr4Voltages : pub set LrdimmDdr4Voltages,
            dimm0_ranks: LU32 : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks: LU32 : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks,

            rtt_nom: LU32 : pub get RttNom : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr: LU32 : pub get RttWr : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park: LU32 : pub get RttPark : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for data
            dqs_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for data strobe (bit clock)
            odt_drive_strength: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for on-die termination
            pmu_phy_vref: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            vref_dq: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // MR6 vref calibration value; 23|30|32
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

    // ACTUAL 1/T, where T is one period.  For DDR, that means DDR400 has
    // frequency 200.
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
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
        UnsupportedRome = 2201,
        UnsupportedMilan = 4401,
    }

    // Usually an array of those is used
    // Note: This structure is not used for LR DRAM
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct MaxFreqElement {
            dimm_slots_per_channel: u8 : pub get DimmsPerChannel : pub set DimmsPerChannel,
            _reserved: u8,
            conditions: [LU16; 4] : pub get [SerdeHex16; 4] : pub set [SerdeHex16; 4], // number of dimm on a channel, number of single-rank dimm, number of dual-rank dimm, number of quad-rank dimm
            speeds: [LU16; 3] : pub get [SerdeHex16; 3] : pub set [SerdeHex16; 3], // speed limit with voltage 1.5 V, 1.35 V, 1.25 V // FIXME make accessible
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
                _reserved: 0,
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
                // Definitely not: EntryId::Memory(MemoryEntryId::PsLrdimmDdr4)
                // => true TODO: Check
                // EntryId::Memory(PsSodimmDdr4MaxFreq) => true
                // Definitely not: EntryId::PsDramdownDdr4MaxFreq => true
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4StretchFreq) => {
                    true
                }
                _ => false,
            }
        }
    }

    // Usually an array of those is used
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrMaxFreqElement {
            dimm_slots_per_channel: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            _reserved: u8,
            pub conditions: [LU16; 4] : pub get [SerdeHex16; 4] : pub set [SerdeHex16; 4], // maybe: number of dimm on a channel, 0, number of lr dimm, 0 // FIXME: Make accessible
            pub speeds: [LU16; 3] : pub get [SerdeHex16; 3] : pub set [SerdeHex16; 3], // maybe: speed limit with voltage 1.5 V, 1.35 V, 1.25 V; FIXME: Make accessible
        }
    }

    impl Default for LrMaxFreqElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1,
                _reserved: 0,
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
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4MaxFreq) => true,
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(Default, FromBytes, AsBytes, Unaligned, PartialEq, Debug, Clone, Copy)]
        #[repr(C, packed)]
        pub struct Gpio {
            pin: u8 : pub get SerdeHex8 : pub set SerdeHex8, // in FCH
            iomux_control: u8 : pub get SerdeHex8 : pub set SerdeHex8, // how to configure that pin
            bank_control: u8 : pub get SerdeHex8 : pub set SerdeHex8, // how to configure bank control
        }
    }

    impl Gpio {
        pub fn new(pin: u8, iomux_control: u8, bank_control: u8) -> Self {
            Self {
                pin,
                iomux_control,
                bank_control,
            }
        }
        pub fn default() -> Self {
            Self {
                pin: 0,
                iomux_control: 0,
                bank_control: 0,
            }
        }
    }

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[derive(PartialEq, Debug, Copy, Clone)]
        #[repr(u32)]
        pub struct ErrorOutControlBeepCodePeakAttr {
            pub peak_count: B5 : pub get u8 : pub set u8,
            /// PULSE_WIDTH: in units of 0.1 s
            pub pulse_width: B3 : pub get u8 : pub set u8,
            pub repeat_count: B4 : pub get u8 : pub set u8,
            #[skip]
            __: B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        ErrorOutControlBeepCodePeakAttr,
        0b1111_1111_1111,
        u32
    );

    impl Default for ErrorOutControlBeepCodePeakAttr {
        fn default() -> Self {
            ErrorOutControlBeepCodePeakAttr {
                bytes: [0, 0, 0, 0],
            }
        }
    }

    #[derive(
        Debug,
        PartialEq,
        FromPrimitive,
        ToPrimitive,
        Copy,
        Clone,
        Serialize,
        Deserialize,
    )]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
    impl Default for ErrorOutControlBeepCodeErrorType {
        fn default() -> Self {
            ErrorOutControlBeepCodeErrorType::General
        }
    }
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct ErrorOutControlBeepCode {
            error_type: LU16,
            peak_map: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            peak_attr: LU32 : pub get ErrorOutControlBeepCodePeakAttr : pub set ErrorOutControlBeepCodePeakAttr,
        }
    }
    #[derive(Serialize, Deserialize)]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    pub struct CustomSerdeErrorOutControlBeepCode {
        pub error_type: ErrorOutControlBeepCodeErrorType,
        pub peak_map: SerdeHex16,
        pub peak_attr: ErrorOutControlBeepCodePeakAttr,
    }
    impl Default for ErrorOutControlBeepCode {
        fn default() -> Self {
            ErrorOutControlBeepCode::new(
                ErrorOutControlBeepCodeErrorType::default(),
                0,
                ErrorOutControlBeepCodePeakAttr::default(),
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
            self,
            value: ErrorOutControlBeepCodeErrorType,
        ) -> Self {
            let mut result = self;
            result.set_error_type(value);
            result
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy,
Clone)]
            #[repr(C, packed)]
            pub struct $struct_name {
                enable_error_reporting: BU8 : pub get bool : pub set bool,
                enable_error_reporting_gpio: BU8 : pub get bool : pub set bool, // FIXME: Remove
                enable_error_reporting_beep_codes: BU8 : pub get bool : pub set bool,
                /// Note: Receiver of the error log: Send 0xDEAD5555 to the INPUT_PORT to acknowledge.
                enable_using_handshake: BU8 : pub get bool : pub set bool, // otherwise see output_delay
                input_port: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // for handshake
                output_delay: LU32 : pub get SerdeHex32 : pub set SerdeHex32, // if no handshake; in units of 10 ns.
                output_port: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
                stop_on_first_fatal_error: BU8: pub get bool : pub set bool,
                _reserved: [u8; 3],
                input_port_size: LU32 : pub get PortSize : pub set PortSize,
                output_port_size: LU32 : pub get PortSize : pub set PortSize,
                input_port_type: LU32 : pub get PortType : pub set PortType, // PortType; default: 6
                output_port_type: LU32 : pub get PortType : pub set PortType, // PortType; default: 6
                clear_acknowledgement: BU8 : pub get bool : pub set bool,
                _reserved_before_gpio: [u8; $padding_before_gpio],
                pub error_reporting_gpio: Gpio,
                _reserved_after_gpio: [u8; $padding_after_gpio],
                beep_code_table: [ErrorOutControlBeepCode; 8], // FIXME: Make accessible
                //beep_code_table: [ErrorOutControlBeepCode; 8] : pub get
                //    [ErrorOutControlBeepCode; 8] : pub set [ErrorOutControlBeepCode; 8], // FIXME: Make accessible
                enable_heart_beat: BU8 : pub get bool : pub set bool,
                enable_power_good_gpio: BU8 : pub get bool : pub set bool,
                pub power_good_gpio: Gpio,
                _reserved_end: [u8; 3], // pad
            }
        }
        paste::paste!{
            #[doc(hidden)]
            #[derive(Serialize, Deserialize)]
            #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
            #[repr(C, packed)]
            pub struct [<CustomSerde $struct_name>] {
                pub enable_error_reporting: bool,
                pub enable_error_reporting_gpio: bool,
                pub enable_error_reporting_beep_codes: bool,
                /// Note: Receiver of the error log: Send 0xDEAD5555 to the INPUT_PORT to acknowledge.
                pub enable_using_handshake: bool,
                pub input_port: SerdeHex32,
                pub output_delay: SerdeHex32,
                pub output_port: SerdeHex32,
                pub stop_on_first_fatal_error: bool,
                pub input_port_size: PortSize,
                pub output_port_size: PortSize,
                pub input_port_type: PortType,
                pub output_port_type: PortType,
                pub clear_acknowledgement: bool,
                pub error_reporting_gpio: Option<Gpio>,
                pub beep_code_table: [ErrorOutControlBeepCode; 8],
                pub enable_heart_beat: bool,
                pub enable_power_good_gpio: bool,
                pub power_good_gpio: Option<Gpio>,
            }
        }

        impl $struct_name {
            pub fn builder() -> Self {
                Self::new()
            }
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
            pub fn with_error_reporting_gpio(self, value: Option<Gpio>) -> Self {
                let mut result = self;
                result.set_error_reporting_gpio(value);
                result
            }
            pub fn beep_code_table(&self) -> Result<[ErrorOutControlBeepCode; 8]> {
                self.beep_code_table.get1()
            }
            pub fn set_beep_code_table(&mut self, value: [ErrorOutControlBeepCode; 8]) {
                self.beep_code_table.set1(value);
            }
            pub fn with_beep_code_table(self, value: [ErrorOutControlBeepCode; 8]) -> Self {
                let mut result = self;
                result.set_beep_code_table(value);
                result
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
            pub fn with_power_good_gpio(self, value: Option<Gpio>) -> Self {
                let mut result = self;
                result.set_power_good_gpio(value);
                result
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
                    _reserved: [0; 3],
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
            pub dimm0: Ddr4DimmRanks : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @0
            #[bits = 4]
            pub dimm1: Ddr4DimmRanks : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @4
            #[bits = 4]
            pub dimm2: Ddr4DimmRanks : pub get Ddr4DimmRanks : pub set Ddr4DimmRanks, // @8
            #[skip]
            __: B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        Ddr4OdtPatDimmRankBitmaps,
        0b0111_0111_0111,
        u32
    );
    impl Ddr4OdtPatDimmRankBitmaps {
        pub fn builder() -> Self {
            Self::new()
        }
    }
    type OdtPatPattern = B4; // TODO: Meaning

    make_bitfield_serde! {
        #[bitfield(bits = 32)]
        #[repr(u32)]
        #[derive(Clone, Copy)]
        pub struct OdtPatPatterns {
            pub reading_pattern: OdtPatPattern : pub get OdtPatPattern : pub set OdtPatPattern, // @bit 0
            #[skip]
            __: B4,             // @bit 4
            pub writing_pattern: OdtPatPattern : pub get OdtPatPattern : pub set OdtPatPattern, // @bit 8
            #[skip]
            __: B20,
        }
    }
    impl_bitfield_primitive_conversion!(OdtPatPatterns, 0b1111_0000_1111, u32);
    impl OdtPatPatterns {
        pub fn builder() -> Self {
            Self::new()
        }
    }
    impl Default for OdtPatPatterns {
        fn default() -> Self {
            Self::new()
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct Ddr4OdtPatElement {
            dimm_rank_bitmaps: LU32 : pub get Ddr4OdtPatDimmRankBitmaps : pub set Ddr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs1_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs2_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs3_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
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
            pub dimm0: LrdimmDdr4DimmRanks : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 0
            pub dimm1: LrdimmDdr4DimmRanks : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 4
            pub dimm2: LrdimmDdr4DimmRanks : pub get LrdimmDdr4DimmRanks : pub set LrdimmDdr4DimmRanks, // @bit 8
            #[skip]
            __: B20,
        }
    }
    impl_bitfield_primitive_conversion!(
        LrdimmDdr4OdtPatDimmRankBitmaps,
        0b0011_0011_0011,
        u32
    );
    impl Default for LrdimmDdr4OdtPatDimmRankBitmaps {
        fn default() -> Self {
            Self::new()
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4OdtPatElement {
            dimm_rank_bitmaps: LU32 : pub get LrdimmDdr4OdtPatDimmRankBitmaps : pub set LrdimmDdr4OdtPatDimmRankBitmaps,
            cs0_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs1_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs2_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
            cs3_odt_patterns: LU32 : pub get OdtPatPatterns : pub set OdtPatPatterns,
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
    #[derive(Clone, Copy)]
    pub struct DdrPostPackageRepairBody {
        pub bank: B5,
        pub rank_multiplier: B3,
        xdevice_width: B5, /* device width of DIMMs to repair; or 0x1F for
                            * heeding target_device instead */
        pub chip_select: B2,
        pub column: B10,
        pub hard_repair: bool,
        pub valid: bool,
        pub target_device: B5,
        pub row: B18,
        pub socket: B3,
        pub channel: B3,
        #[skip]
        __: B8,
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
    }

    impl_bitfield_primitive_conversion!(
        DdrPostPackageRepairBody,
        0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111,
        u64
    );

    impl DdrPostPackageRepairBody {
        pub fn builder() -> Self {
            Self::new()
        }
    }

    make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, Debug, Copy, Clone, Serialize,
    Deserialize)]
            #[repr(C, packed)]
            pub struct DdrPostPackageRepairElement {
                #[serde(with = "SerHex::<StrictPfx>")]
                body: [u8; 8], // no: pub get DdrPostPackageRepairBody : pub set DdrPostPackageRepairBody,
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
            Self {
                body: [0, 0, 0, 0, 0, 0, 0, 0xff],
            }
        }
        pub fn builder() -> Self {
            Self::invalid()
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

    impl EntryCompatible for DdrPostPackageRepairElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            matches!(
                entry_id,
                EntryId::Memory(MemoryEntryId::DdrPostPackageRepair)
            )
        }
    }

    pub mod platform_specific_override {
        use super::{EntryId, Error, MemoryEntryId};
        crate::struct_variants_enum::collect_EntryCompatible_impl_into_enum! {
                        // See AMD #44065

                        use core::mem::size_of;
                        use static_assertions::const_assert;
                        use zerocopy::{AsBytes, FromBytes, Unaligned};
                        use super::super::*;
                        use crate::struct_accessors::{Getter, Setter, make_accessors};
                        use crate::types::Result;

                        make_bitfield_serde! {
                            #[bitfield(filled = true, bits = 8)]
                            #[repr(u8)]
                            #[derive(Clone, Copy, PartialEq)]
                            pub struct ChannelIdsSelection {
                                pub a: bool : pub get bool : pub set bool,
                                pub b: bool : pub get bool : pub set bool,
                                pub c: bool : pub get bool : pub set bool,
                                pub d: bool : pub get bool : pub set bool,
                                pub e: bool : pub get bool : pub set bool,
                                pub f: bool : pub get bool : pub set bool,
                                pub g: bool : pub get bool : pub set bool,
                                pub h: bool : pub get bool : pub set bool,
                            }
                        }
                        impl_bitfield_primitive_conversion!(ChannelIdsSelection, 0b1111_1111, u8);
                        impl ChannelIdsSelection {
                            pub fn builder() -> Self {
                                Self::new()
                            }
                        }
                        impl Default for ChannelIdsSelection {
                            fn default() -> Self {
                                Self::new()
                            }
                        }

                        #[derive(PartialEq, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
                                pub socket_0: bool : pub get bool : pub set bool,
                                pub socket_1: bool : pub get bool : pub set bool,
                                pub socket_2: bool : pub get bool : pub set bool,
                                pub socket_3: bool : pub get bool : pub set bool,
                                pub socket_4: bool : pub get bool : pub set bool,
                                pub socket_5: bool : pub get bool : pub set bool,
                                pub socket_6: bool : pub get bool : pub set bool,
                                pub socket_7: bool : pub get bool : pub set bool,
                            }
                        }
                        impl_bitfield_primitive_conversion!(SocketIds, 0b1111_1111, u8);
                        impl SocketIds {
                            pub const ALL: Self = Self::from_bytes([0xff]);
                            pub fn builder() -> Self {
                                Self::new()
                            }
                        }
                        impl Default for SocketIds {
                            fn default() -> Self {
                                Self::new()
                            }
                        }

                        make_bitfield_serde! {
                            #[bitfield(bits = 8)]
                            #[repr(u8)]
                            #[derive(Clone, Copy)]
                            pub struct DimmSlotsSelection {
                                pub dimm_slot_0: bool : pub get bool : pub set bool, // @0
                                pub dimm_slot_1: bool : pub get bool : pub set bool, // @1
                                pub dimm_slot_2: bool : pub get bool : pub set bool, // @2
                                pub dimm_slot_3: bool : pub get bool : pub set bool, // @3
                                #[skip] __: B4,
                            }
                        }
                        impl_bitfield_primitive_conversion!(DimmSlotsSelection, 0b1111, u8);
                        impl DimmSlotsSelection {
                            pub fn builder() -> Self {
                                Self::new()
                            }
                        }
                        impl Default for DimmSlotsSelection {
                            fn default() -> Self {
                                Self::new()
                            }
                        }
                        #[derive(Clone, Copy, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
                                    let blob = AsBytes::as_bytes(self);
                                    if <$struct_>::is_entry_compatible(entry_id, blob) {
                                        Some(blob)
                                    } else {
                                        None
                                    }
                                }
                            }
                            impl ElementAsBytes for $struct_ {
                                fn element_as_bytes(&self) -> &[u8] {
                                    AsBytes::as_bytes(self)
                                }
                            }

                        //            impl HeaderWithTail for $struct_ {
                        //                type TailArrayItemType = ();
                        //            }
                        )}

                        make_accessors! {
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Clone)]
                            #[repr(C, packed)]
                            pub struct CkeTristateMap {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package's clock enable (CKE) pin, value = memory rank's CKE pin mask
                                pub connections: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],
                            }
                        }
                        impl_EntryCompatible!(CkeTristateMap, 1, 7);
                        impl Default for CkeTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: 1,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct OdtTristateMap {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package's ODT pin (MA_ODT\[i\]), value = memory rank's ODT pin mask
                                pub connections: [u8; 4] : pub get [SerdeHex8; 4] : pub set [SerdeHex8; 4],
                            }
                        }
                        impl_EntryCompatible!(OdtTristateMap, 2, 7);
                        impl Default for OdtTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: 2,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct CsTristateMap {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                /// index i = CPU package CS pin (MA_CS_L\[i\]), value = memory rank's CS pin
                                pub connections: [u8; 8] : pub get [SerdeHex8; 8] : pub set [SerdeHex8; 8],
                            }
                        }
                        impl_EntryCompatible!(CsTristateMap, 3, 11);
                        impl Default for CsTristateMap {
                            fn default() -> Self {
                                Self {
                                    type_: 3,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxDimmsPerChannel {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "any"
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(MaxDimmsPerChannel, 4, 4);
                        impl Default for MaxDimmsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: 4,
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

                        make_accessors! {
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemclkMap {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "all"
                                pub connections: [u8; 8] : pub get [SerdeHex8; 8] : pub set [SerdeHex8; 8],
                            }
                        }
                        impl_EntryCompatible!(MemclkMap, 7, 11);
                        impl Default for MemclkMap {
                            fn default() -> Self {
                                Self {
                                    type_: 7,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxChannelsPerSocket {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,  // Note: must always be "any"
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "any" here
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(MaxChannelsPerSocket, 8, 4);
                        impl Default for MaxChannelsPerSocket {
                            fn default() -> Self {
                                Self {
                                    type_: 8,
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

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
                        pub enum TimingMode {
                            Auto = 0,
                            Limit = 1,
                            Specific = 2,
                        }

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemBusSpeed {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "all"
                                timing_mode: LU32 : pub get TimingMode : pub set TimingMode,
                                bus_speed: LU32 : pub get MemBusSpeedType : pub set MemBusSpeedType,
                            }
                        }
                        impl_EntryCompatible!(MemBusSpeed, 9, 11);
                        impl Default for MemBusSpeed {
                            fn default() -> Self {
                                Self {
                                    type_: 9,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MaxCsPerChannel {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "Any"
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(MaxCsPerChannel, 10, 4);
                        impl Default for MaxCsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: 10,
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
                Clone, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemTechnology {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds, // Note: must always be "any" here
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: must always be "any" here
                                technology_type: LU32 : pub get MemTechnologyType : pub set MemTechnologyType,
                            }
                        }
                        impl_EntryCompatible!(MemTechnology, 11, 7);
                        impl Default for MemTechnology {
                            fn default() -> Self {
                                Self {
                                    type_: 11,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct WriteLevellingSeedDelay {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                seed: [u8; 8] : pub get [SerdeHex8; 8] : pub set [SerdeHex8; 8],
                                ecc_seed: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(WriteLevellingSeedDelay, 12, 12);
                        impl Default for WriteLevellingSeedDelay {
                            fn default() -> Self {
                                Self {
                                    type_: 12,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct RxEnSeed {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                seed: [LU16; 8] : pub get [SerdeHex16; 8] : pub set [SerdeHex16; 8],
                                ecc_seed: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
                            }
                        }
                        impl_EntryCompatible!(RxEnSeed, 13, 21);
                        impl Default for RxEnSeed {
                            fn default() -> Self {
                                Self {
                                    type_: 13,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct LrDimmNoCs6Cs7Routing {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots,
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(LrDimmNoCs6Cs7Routing, 14, 4);
                        impl Default for LrDimmNoCs6Cs7Routing {
                            fn default() -> Self {
                                Self {
                                    type_: 14,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct SolderedDownSodimm {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(SolderedDownSodimm, 15, 4);
                        impl Default for SolderedDownSodimm {
                            fn default() -> Self {
                                Self {
                                    type_: 15,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct LvDimmForce1V5 {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels: u8 : pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8, // Note: always 1
                            }
                        }
                        impl_EntryCompatible!(LvDimmForce1V5, 16, 4);
                        impl Default for LvDimmForce1V5 {
                            fn default() -> Self {
                                Self {
                                    type_: 16,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MinimumRwDataEyeWidth {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                min_read_data_eye_width: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                min_write_data_eye_width: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(MinimumRwDataEyeWidth, 17, 5);
                        impl Default for MinimumRwDataEyeWidth {
                            fn default() -> Self {
                                Self {
                                    type_: 17,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct CpuFamilyFilter {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                cpu_family_revision: LU32 : pub get SerdeHex32 : pub set SerdeHex32,
                            }
                        }
                        impl_EntryCompatible!(CpuFamilyFilter, 18, 4);
                        impl Default for CpuFamilyFilter {
                            fn default() -> Self {
                                Self {
                                    type_: 18,
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct SolderedDownDimmsPerChannel {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds,
                                channels: u8 : pub get ChannelIds : pub set ChannelIds,
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                            }
                        }
                        impl_EntryCompatible!(SolderedDownDimmsPerChannel, 19, 4);
                        impl Default for SolderedDownDimmsPerChannel {
                            fn default() -> Self {
                                Self {
                                    type_: 19,
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

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
                        pub enum MemPowerPolicyType {
                            Performance = 0,
                            BatteryLife = 1,
                            Auto = 2,
                        }

                        make_accessors! {
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MemPowerPolicy {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels: u8 : pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value: u8 : pub get MemPowerPolicyType : pub set MemPowerPolicyType,
                            }
                        }
                        impl_EntryCompatible!(MemPowerPolicy, 20, 4);
                        impl Default for MemPowerPolicy {
                            fn default() -> Self {
                                Self {
                                    type_: 20,
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

                        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone, Serialize, Deserialize)]
                        #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
                        pub enum MotherboardLayerCount {
                            _4 = 0,
                            _6 = 1,
                        }

                        make_accessors! {
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Copy, Clone)]
                            #[repr(C, packed)]
                            pub struct MotherboardLayers {
                                type_: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                payload_size: u8 : pub get SerdeHex8 : pub set SerdeHex8,
                                sockets: u8 : pub get SocketIds : pub set SocketIds, // Note: always "all"
                                channels: u8 : pub get ChannelIds : pub set ChannelIds, // Note: always "all"
                                dimms: u8 : pub get DimmSlots : pub set DimmSlots, // Note: always "all"
                                value: u8 : pub get MotherboardLayerCount : pub set MotherboardLayerCount,
                            }
                        }
                        impl_EntryCompatible!(MotherboardLayers, 21, 4);
                        impl Default for MotherboardLayers {
                            fn default() -> Self {
                                Self {
                                    type_: 21,
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
                        use zerocopy::{AsBytes, FromBytes, Unaligned};
                        use super::super::*;
                        //use crate::struct_accessors::{Getter, Setter, make_accessors};
                        //use crate::types::Result;

                        macro_rules! impl_EntryCompatible {($struct_:ty, $type_:expr, $total_size:expr) => (
                            const_assert!($total_size as usize == size_of::<$struct_>());
                            impl $struct_ {
                                const TAG: u16 = $type_;
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
                                    let blob = AsBytes::as_bytes(self);
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
                            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug,
        Clone, Copy)]
                            #[repr(C, packed)]
                            pub struct Terminator {
                                type_: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
                            }
                        }
                        impl_EntryCompatible!(Terminator, 0xfeef, 2);

                        impl Default for Terminator {
                            fn default() -> Self {
                                Self {
                                    type_: 0xfeef.into(),
                                }
                            }
                        }


                        impl Terminator {
                            pub fn new() -> Self {
                                Self::default()
                            }
                            pub fn builder() -> Self {
                                Self::new()
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
            assert!(abl_console_out_control
                .enable_mem_setreg_logging()
                .unwrap());
            abl_console_out_control.set_enable_mem_setreg_logging(false);
            assert!(!abl_console_out_control
                .enable_mem_setreg_logging()
                .unwrap());
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
                Some(ElementRef::LvDimmForce1V5(ref _item)) => {}
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
    }
}

pub mod psp {
    use super::memory::Gpio;
    use super::*;
    use crate::struct_accessors::{make_accessors, Getter, Setter};

    make_accessors! {
        #[derive(Default, FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct IdApcbMapping {
            id_and_feature_mask: u8 : pub get SerdeHex8 : pub set SerdeHex8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            board_instance_index: u8 : pub get SerdeHex8 : pub set SerdeHex8,
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

    #[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct IdRevApcbMapping {
            id_and_rev_and_feature_mask: u8 : pub get SerdeHex8 : pub set SerdeHex8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            rev_and_feature_value: u8 : pub get RevAndFeatureValue : pub set RevAndFeatureValue,
            board_instance_index: u8 : pub get SerdeHex8 : pub set SerdeHex8,
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
        pub fn default() -> Self {
            Self {
                id_and_rev_and_feature_mask: 0x80,
                id_and_feature_value: 0,
                rev_and_feature_value: 0,
                board_instance_index: 0,
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodCustom {
            access_method: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // 0xF for BoardIdGettingMethodCustom
            feature_mask: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
        }
    }

    impl Default for BoardIdGettingMethodCustom {
        fn default() -> Self {
            Self {
                access_method: 0xF.into(),
                feature_mask: 0.into(),
            }
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodGpio {
            access_method: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // 3 for BoardIdGettingMethodGpio
            pub bit_locations: [Gpio; 4] : pub get [Gpio; 4] : pub set [Gpio; 4], // for the board id
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
            Self {
                access_method: 3.into(),
                bit_locations,
            }
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodEeprom {
            access_method: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // 2 for BoardIdGettingMethodEeprom
            i2c_controller_index: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            device_address: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            board_id_offset: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // Byte offset
            board_rev_offset: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // Byte offset
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Copy, Clone)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodSmbus {
            access_method: LU16 : pub get SerdeHex16 : pub set SerdeHex16, // 1 for BoardIdGettingMethodSmbus
            i2c_controller_index: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            i2c_mux_address: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            mux_control_address: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            mux_channel: u8 : pub get SerdeHex8 : pub set SerdeHex8,
            smbus_address: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
            register_index: LU16 : pub get SerdeHex16 : pub set SerdeHex16,
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum BaudRate {
    _2400Baud = 0,
    _3600Baud = 1,
    _4800Baud = 2,
    _7200Baud = 3,
    _9600Baud = 4,
    _19200Baud = 5,
    _38400Baud = 6,
    _57600Baud = 7,
    _115200Baud = 8,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemActionOnBistFailure {
    DoNothing = 0,
    DisableProblematicCcds = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemDataPoison {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMaxActivityCount {
    Untested = 0,
    _700K = 1,
    _600K = 2,
    _500K = 3,
    _400K = 4,
    _300K = 5,
    _200K = 6,
    Unlimited = 8,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemRcwWeakDriveDisable {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemSelfRefreshExitStaggering {
    Disabled = 0,
    OneThird = 3,  // Trfc/3
    OneFourth = 4, // Trfc/4
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum CbsMemAddrCmdParityRetryDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum CcxSevAsidCount {
    _253 = 0,
    _509 = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum FchConsoleOutSuperIoType {
    Auto = 0,
    Type1 = 1,
    Type2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum FchConsoleSerialPort {
    SuperIo = 0,
    Uart0Mmio = 1,
    Uart1Mmio = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfToggle {
    Disabled = 0,
    Enabled = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemTsmeMode {
    Disabled = 0,
    Enabled = 1,
    // Auto = 0xff, // TODO: Test.
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemNvdimmPowerSource {
    DeviceManaged = 1,
    HostManaged = 2,
}

// See JESD82-31A Table 48.
#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemRdimmTimingCmdParLatency {
    _1_nCK = 0, // not valid in gear-down mode
    _2_nCK = 1,
    _3_nCK = 2, // not valid in gear-down mode
    _4_nCK = 3,
    _0_nCK = 4, // only valid if parity checking and CAL modes are disabled
    Auto = 0xff,
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemThrottleCtrlRollWindowDepth {
    Memclks(NonZeroU8),
    // 0: reserved
}

/// See UMC::SpazCtrl: AutoRefFineGranMode.
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemAutoRefreshFineGranMode {
    Fixed1Times = 0,
    Fixed2Times = 1,
    Fixed4Times = 2,
    Otf2Times = 5,
    Otf4Times = 6,
}

/// See UMC::CH::ThrottleCtrl: DisRefCmdThrotCnt.
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemAutoRefreshsCountForThrottling {
    Enabled = 0,
    Disabled = 1,
}

impl FromPrimitive for MemThrottleCtrlRollWindowDepth {
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
impl ToPrimitive for MemThrottleCtrlRollWindowDepth {
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemControllerWritingCrcMode {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemHealPprType {
    SoftRepair = 0,
    HardRepair = 1,
    NoRepair = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemHealTestSelect {
    Normal = 0,
    NoVendorTests = 1,
    ForceAllVendorTests = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfExtIpSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfMemInterleaving {
    None = 0,
    Channel = 1,
    Die = 2,
    Socket = 3,
    Auto = 7,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfMemInterleavingSize {
    _256_Byte = 0,
    _512_Byte = 1,
    _1024_Byte = 2,
    _2048_Byte = 3,
    _4096_Byte = 4,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfDramNumaPerSocket {
    None = 0,
    One = 1,
    Two = 2,
    Four = 3,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfRemapAt1TiB {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfXgmiLinkConfig {
    _2_links_connected = 0,
    _3_links_connected = 1,
    _4_links_connected = 2,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfPstateModeSelect {
    Normal = 0,
    LimitHighest = 1,
    LimitAll = 2,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum GnbSmuDfPstateFclkLimit {
    _1600_Mhz = 0,
    _1467_Mhz = 1,
    _1333_MHz = 2,
    _1200_MHz = 3,
    _1067_MHz = 4,
    _933_MHz = 5,
    _800_MHz = 6,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum SecondPcieLinkSpeed {
    Keep = 0,
    Gen1 = 1,
    Gen2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum BmcLinkSpeed {
    PcieGen1 = 1,
    PcieGen2 = 2,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum SecondPcieLinkMaxPayload {
    _128_Byte = 0,
    _256_Byte = 1,
    _512_Byte = 2,
    _1024_Byte = 3,
    _2048_Byte = 4,
    _4096_Byte = 5,
    HardwareDefault = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemControllerPmuTrainFfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemControllerPmuTrainDfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemControllerPmuTrainingMode {
    _1D = 0,
    _1D_2D_Read_Only = 1,
    _1D_2D_Write_Only = 2,
    _1D_2D = 3,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum UmaMode {
    None = 0,
    Specified = 1,
    Auto = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMbistTest {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMbistPatternSelect {
    Prbs = 0,
    Sso = 1,
    Both = 2,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMbistAggressorsChannels {
    Disabled = 0,
    _1_AggressorsPer2Channels = 1,
    _3_AggressorsPer4Channels = 3,
    _7_AggressorsPer8Channels = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMbistTestMode {
    PhysicalInterface = 0,
    DataEye = 1,
    // Both = , ?
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemMbistDataEyeType {
    _1dVolate = 0,
    _1dTiming = 1,
    _2dFullDataEye = 2,
    WorstCaseMarginOnly = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfXgmiTxEqMode {
    Disabled = 0,
    EnabledByLane = 1,
    EnabledByLink = 2,
    EnabledByLinkAndRxVetting = 3,
    Auto = 0xff,
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfXgmiLinkMaxSpeed {
    _6_40Gbps = 0,
    _7_467Gbps = 1,
    _8_533Gbps = 2,
    _9_6Gbps = 3,
    _10_667Gbps = 4,
    _11Gbps = 5,
    _12Gbps = 6,
    _13Gbps = 7,
    _14Gbps = 8,
    _15Gbps = 9,
    _16Gbps = 10,
    _17Gbps = 11,
    _18Gbps = 12,
    _19Gbps = 13,
    _20Gbps = 14,
    _21Gbps = 15,
    _22Gbps = 16,
    _23Gbps = 17,
    _24Gbps = 18,
    _25Gbps = 19,
    _26Gbps = 20,
    _27Gbps = 21,
    _28Gbps = 22,
    _29Gbps = 23,
    _30Gbps = 24,
    _31Gbps = 25,
    _32Gbps = 26,
    Auto = 0xff,
}

pub type DfXgmi2LinkMaxSpeed = DfXgmiLinkMaxSpeed;
pub type DfXgmi3LinkMaxSpeed = DfXgmiLinkMaxSpeed;
pub type DfXgmi4LinkMaxSpeed = DfXgmiLinkMaxSpeed;

/// Placement of private memory regions (PSP, SMU, CC6)
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum BmcGen2TxDeemphasis {
    Csr = 0,
    Upstream = 1,
    Minus_6_dB = 2,
    Minus_3_5_dB = 3,
    Disabled = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum BmcRcbCheckingMode {
    EnableRcbChecking = 0,
    DisableRcbChecking = 1,
    Auto = 0xff,
}

#[allow(non_camel_case_types, non_snake_case)]
#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DxioPhyParamPole {
    Value(u32), // not 0xffff_ffff
    Skip,
}
impl FromPrimitive for DxioPhyParamPole {
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DxioPhyParamIqofc {
    Value(i32),
    // Skip
}
impl FromPrimitive for DxioPhyParamIqofc {
    fn from_i64(value: i64) -> Option<Self> {
        if value >= -4 && value <= 4 {
            Some(Self::Value(value as i32))
        } else {
            None
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
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
    Auto = 0xffff_ffff, // FIXME: verify
}

type MemBusFrequencyLimit = MemClockValue;

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemHealBistEnable {
    Disabled = 0,
    TestAndRepairAllMemory = 1,
    JedecSelfHealing = 2,
    TestAndRepairAllMemoryAndJedecSelfHealing = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum FchSmbusSpeed {
    Value(u8), /* x in 66 MHz / (4 x)
                * Auto */
}
impl FromPrimitive for FchSmbusSpeed {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x100 {
            Some(Self::Value(value as u8))
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            Self::from_u64(value as u64)
        } else {
            None
        }
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
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum DfCakeCrcThresholdBounds {
    Value(u32), // x: 0...1_000_000d; Percentage is 0.00001% * x
}
impl FromPrimitive for DfCakeCrcThresholdBounds {
    fn from_u64(value: u64) -> Option<Self> {
        if value <= 1_000_000 {
            Some(Self::Value(value as u32))
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 {
            Self::from_u64(value as u64)
        } else {
            None
        }
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

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum MemTrainingHdtControl {
    DetailedDebugMessages = 5,
    CoarseDebugMessages = 10,
    StageCompletionMessages = 200,
    StageCompletionMessages1 = 201, // FIXME
    // TODO: Seen in the wild: 201
    FirmwareCompletionMessagesOnly = 0xfe,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum PspEnableDebugMode {
    Disabled = 0,
    Enabled = 1,
}

make_bitfield_serde! {
    #[bitfield(bits = 16)]
    #[repr(u16)]
    #[derive(Default, Debug, Copy, Clone, PartialEq)]
    pub struct FchGppClkMapSelection {
        pub s0_gpp0_off: B1 : pub get bool : pub set bool,
        pub s0_gpp1_off: B1 : pub get bool : pub set bool,
        pub s0_gpp4_off: B1 : pub get bool : pub set bool,
        pub s0_gpp2_off: B1 : pub get bool : pub set bool,
        pub s0_gpp3_off: B1 : pub get bool : pub set bool,
        #[skip]
        __: B3,

        pub s1_gpp0_off: B1 : pub get bool : pub set bool,
        pub s1_gpp1_off: B1 : pub get bool : pub set bool,
        pub s1_gpp4_off: B1 : pub get bool : pub set bool,
        pub s1_gpp2_off: B1 : pub get bool : pub set bool,
        pub s1_gpp3_off: B1 : pub get bool : pub set bool,
        #[skip]
        __: B3,
    }
}
impl FchGppClkMapSelection {
    pub fn builder() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(
    feature = "std",
    derive(Serialize, Deserialize, schemars::JsonSchema)
)]
pub enum FchGppClkMap {
    On,
    Value(FchGppClkMapSelection),
    Auto,
}
impl FromPrimitive for FchGppClkMap {
    fn from_u64(value: u64) -> Option<Self> {
        if value < 0x100 {
            match value {
                0xffff => Some(Self::Auto),
                0x0000 => Some(Self::On),
                x => Some(Self::Value(FchGppClkMapSelection::from_bytes([
                    (x & 0xff) as u8,
                    (x >> 8) as u8,
                ]))),
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
impl ToPrimitive for FchGppClkMap {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::On => Some(0),
            Self::Value(x) => {
                let result: u16 = (*x).into();
                if result == 0xffff || result == 0x0000 {
                    None
                } else {
                    Some(result.try_into().ok()?)
                }
            }
            Self::Auto => Some(0xffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

make_token_accessors! {
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum ByteToken: {TokenEntryId::Byte} {
        // ABL

        AblSerialBaudRate(default 8, id 0xae46_cea4) : pub get BaudRate : pub set BaudRate,

        // PSP

        PspEnableDebugMode(default 0, id 0xd109_1cd0) : pub get PspEnableDebugMode : pub set PspEnableDebugMode,

        // Memory Controller

        CbsMemSpeedDdr4(default 0xff, id 0xe060_4ce9) : pub get CbsMemSpeedDdr4 : pub set CbsMemSpeedDdr4,
        MemActionOnBistFailure(default 0, id 0xcbc2_c0dd) : pub get MemActionOnBistFailure : pub set MemActionOnBistFailure, // Milan
        MemRcwWeakDriveDisable(default 1, id 0xa30d_781a) : pub get MemRcwWeakDriveDisable : pub set MemRcwWeakDriveDisable, // FIXME is it u32 ?
        MemSelfRefreshExitStaggering(default 0, id 0xbc52_e5f7) : pub get MemSelfRefreshExitStaggering : pub set MemSelfRefreshExitStaggering,
        CbsMemAddrCmdParityRetryDdr4(default 0, id 0xbe8b_ebce) : pub get CbsMemAddrCmdParityRetryDdr4 : pub set CbsMemAddrCmdParityRetryDdr4, // FIXME: Is it u32 ?
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CbsMemAddrCmdParityErrorMaxReplayDdr4(default 8, id 0x04e6_a482) : pub get u8 : pub set u8, // 0...0x3f
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CbsMemWriteCrcErrorMaxReplayDdr4(default 8, id 0x74a0_8bec) : pub get u8 : pub set u8,
        // Byte just like AMD
        MemRcdParity(default 1, id 0x647d7662) : pub get bool : pub set bool,
        // Byte just like AMD
        CbsMemUncorrectedEccRetryDdr4(default 1, id 0xbff0_0125) : pub get bool : pub set bool,
        /// UMC::CH::SpazCtrl::UrgRefLimit; value: 1...6 (as in register mentioned first)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUrgRefLimit(default 6, id 0x1333_32df) : pub get u8 : pub set u8,
        /// UMC::CH::SpazCtrl::SubUrgRefLowerBound; value: 1...6 (as in register mentioned first)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSubUrgRefLowerBound(default 4, id 0xe756_2ab6) : pub get u8 : pub set u8,
        MemControllerPmuTrainFfeDdr4(default 0xff, id 0x0d46_186d) : pub get MemControllerPmuTrainFfeDdr4 : pub set MemControllerPmuTrainFfeDdr4, // FIXME: is it bool ?
        MemControllerPmuTrainDfeDdr4(default 0xff, id 0x36a4_bb5b) : pub get MemControllerPmuTrainDfeDdr4 : pub set MemControllerPmuTrainDfeDdr4, // FIXME: is it bool ?
        /// See Transparent Secure Memory Encryption in PPR
        MemTsmeModeRome(default 1, id 0xd1fa_6660) : pub get MemTsmeMode : pub set MemTsmeMode,
        MemTrainingHdtControl(default 200, id 0xaf6d_3a6f) : pub get MemTrainingHdtControl : pub set MemTrainingHdtControl, // TODO: Before using default, fix default.  It's possibly not correct.
        MemHealBistEnable(default 0, id 0xfba2_3a28) : pub get MemHealBistEnable : pub set MemHealBistEnable,
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSelfHealBistEnable(default 0, id 0x2c23_924c) : pub get u8 : pub set u8, // FIXME: is it bool ?  // TODO: Before using default, fix default.  It's possibly not correct.
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemPmuBistTestSelect(default 0, id 0x7034_fbfb) : pub get u8 : pub set u8, // TODO: Before using default, fix default.  It's possibly not correct.; note: range 1...7
        MemHealTestSelect(default 0, id 0x5908_2cf2) : pub get MemHealTestSelect : pub set MemHealTestSelect,
        MemHealPprType(default 0, id 0x5418_1a61) : pub get MemHealPprType : pub set MemHealPprType,
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemHealMaxBankFails(default 3, id 0x632e_55d8) : pub get u8 : pub set u8, // per bank

        // Ccx

        CcxSevAsidCount(default 1, id 0x5587_6720) : pub get CcxSevAsidCount : pub set CcxSevAsidCount,

        // Fch

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchConsoleOutMode(default 0, id 0xddb7_59da) : pub get u8 : pub set u8,
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchConsoleOutBasicEnable(default 0, id 0xa0903f98) : pub get u8 : pub set u8, // Rome (Obsolete)
        FchConsoleOutSerialPort(default 0, id 0xfff9_f34d) : pub get FchConsoleSerialPort : pub set FchConsoleSerialPort,
        FchSmbusSpeed(default 42, id 0x2447_3329) : pub get FchSmbusSpeed : pub set FchSmbusSpeed,
        FchConsoleOutSuperIoType(default 0, id 0x5c8d_6e82) : pub get FchConsoleOutSuperIoType : pub set FchConsoleOutSuperIoType, // init mode

        // Df

        DfExtIpSyncFloodPropagation(default 0, id 0xfffe_0b07) : pub get DfExtIpSyncFloodPropagation : pub set DfExtIpSyncFloodPropagation,
        DfSyncFloodPropagation(default 0, id 0x4963_9134) : pub get DfSyncFloodPropagation : pub set DfSyncFloodPropagation,
        //DfMemInterleaving(default 7, id 0xce01_87ef) : pub get DfMemInterleaving : pub set DfMemInterleaving,
        DfMemInterleaving(default 7, id 0xce0176ef) : pub get DfMemInterleaving : pub set DfMemInterleaving, // Rome
        DfMemInterleavingSize(default 7, id 0x2606_c42e) : pub get DfMemInterleavingSize : pub set DfMemInterleavingSize,
        DfDramNumaPerSocket(default 1, id 0x2cf3_dac9) : pub get DfDramNumaPerSocket : pub set DfDramNumaPerSocket, // TODO: Maybe the default value here should be 7
        DfProbeFilter(default 1, id 0x6597_c573) : pub get DfToggle : pub set DfToggle,
        DfMemClear(default 3, id 0x9d17_7e57) : pub get DfToggle : pub set DfToggle,
        DfGmiEncrypt(default 0, id 0x08a4_5920) : pub get DfToggle : pub set DfToggle,
        DfXgmiEncrypt(default 0, id 0x6bd3_2f1c) : pub get DfToggle : pub set DfToggle,
        DfSaveRestoreMemEncrypt(default 1, id 0x7b3d_1f75) : pub get DfToggle : pub set DfToggle,
        /// Where the PCI MMIO hole will start (bits 31 to 24 inclusive)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfBottomIo(default 0xe0, id 0x8fb9_8529) : pub get u8 : pub set u8,
        DfRemapAt1TiB(default 0, id 0x35ee_96f3) : pub get DfRemapAt1TiB : pub set DfRemapAt1TiB,
        DfXgmiTxEqMode(default 0xff, id 0xade7_9549) : pub get DfXgmiTxEqMode : pub set DfXgmiTxEqMode,
        DfInvertDramMap(default 0, id 0x6574_b2c0) : pub get DfToggle : pub set DfToggle,

        // Misc

        SecondPcieLinkSpeed(default 0, id 0x8723_750f) : pub get SecondPcieLinkSpeed : pub set SecondPcieLinkSpeed,
        SecondPcieLinkMaxPayload(default 0xff, id 0xe02d_f04b) : pub get SecondPcieLinkMaxPayload : pub set SecondPcieLinkMaxPayload, // Milan
        WorkloadProfile(default 0, id 0x22f4_299f) : pub get WorkloadProfile : pub set WorkloadProfile, // Milan

        // MBIST for Milan and Rome; defaults wrong!

        MemMbistDataEyeType(default 3, id 0x4e2e_dc1b) : pub get MemMbistDataEyeType : pub set MemMbistDataEyeType,
        // Byte just like AMD
        MemMbistDataEyeSilentExecution(default 0, id 0x3f74_c7e7) : pub get bool : pub set bool, // Milan
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWorseCasGranularity(default 0, id 0x23b0b6a1) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistReadDataEyeVoltageStep(default 0, id 0x35d6a4f8) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneVal(default 0, id 0x4474d416) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneVal(default 0, id 0x4d7e0206) : pub get u8 : pub set u8, // Rome
        MemMbistTestMode(default 0, id 0x567a1fc0) : pub get MemMbistTestMode : pub set MemMbistTestMode, // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelEcc(default 0, id 0x57122e99) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistReadDataEyeTimingStep(default 0, id 0x58ccd28a) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistDataEyeExecutionRepeatCount(default 0, id 0x8e4bdad7) : pub get u8 : pub set u8, // Rome; 0..=10
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelEcc(default 0, id 0xa6e92cee) : pub get u8 : pub set u8, // Rome
        /// in powers of ten; 3..=12
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistPatternLength(default 0, id 0xae7baedd) : pub get u8 : pub set u8, // Rome;
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistHaltOnError(default 0, id 0xb1940f25) : pub get u8 : pub set u8, // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWriteDataEyeVoltageStep(default 0, id 0xcda61022) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistPerBitSlaveDieReport(default 0, id 0xcff56411) : pub get u8 : pub set u8, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistWriteDataEyeTimingStep(default 0, id 0xd9025142) : pub get u8 : pub set u8, // Rome
        MemMbistAggressorsChannels(default 0, id 0xdcd1444a) : pub get MemMbistAggressorsChannels : pub set MemMbistAggressorsChannels, // Rome
        MemMbistTest(default 0, id 0xdf5502c8) : pub get MemMbistTest : pub set MemMbistTest, // (obsolete)
        MemMbistPatternSelect(default 0, id 0xf527ebf8) : pub get MemMbistPatternSelect : pub set MemMbistPatternSelect, // Rome
        MemMbistAggressorOn(default 0, id 0x32361c4) : pub get bool : pub set bool, // Rome; obsolete

        // Unsorted Milan; defaults wrong!

        MemOverrideDimmSpdMaxActivityCount(default 0xff, id 0x853cdaa) : pub get MemMaxActivityCount : pub set MemMaxActivityCount,
        GnbSmuDfPstateFclkLimit(default 0xff, id 0xea388ac3) : pub get GnbSmuDfPstateFclkLimit : pub set GnbSmuDfPstateFclkLimit, // Milan

        // Unsorted Rome; ungrouped; defaults wrong!

        /// I doubt that AMD converts those, but the 2 lowest bits usually set up the resolution. 0: 0.5 ºC; 1: 0.25 ºC; 2: 0.125 ºC; 3: 0.0625 ºC; higher resolution is slower.
        /// DIMM temperature sensor register at address 8
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorResolution(default 0, id 0x831af313) : pub get u8 : pub set u8, // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PcieResetPinSelect(default 0, id 0x8c0b2de9) : pub get u8 : pub set u8, // value 2 // Rome; 0..=4; FIXME: enum?
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramAddressCommandParityRetryCount(default 0, id 0x3e7c51f8) : pub get u8 : pub set u8, // value 1 // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemParityErrorMaxReplayDdr4(default 0, id 0xc9e9a1c9) : pub get u8 : pub set u8, // value 8 // Rome // 0..=0x3f (6 bit)
        Df2LinkMaxXgmiSpeed(default 0, id 0xd19c_6e80): pub get DfXgmi2LinkMaxSpeed : pub set DfXgmi2LinkMaxSpeed, // Genoa
        Df3LinkMaxXgmiSpeed(default 0, id 0x53ba449b) : pub get DfXgmi3LinkMaxSpeed : pub set DfXgmi3LinkMaxSpeed, // value 0xff // Rome
        Df4LinkMaxXgmiSpeed(default 0, id 0x3f307cb3) : pub get DfXgmi4LinkMaxSpeed : pub set DfXgmi4LinkMaxSpeed, // value 0xff //  Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramDoubleRefreshRate(default 0, id 0x44d40026) : pub get u8 : pub set u8, // value 0 // Rome
        /// See UMC::CH::ThrottleCtrl RollWindowDepth
        MemRollWindowDepth(default 0xff, id 0x5985083a) : pub get MemThrottleCtrlRollWindowDepth : pub set MemThrottleCtrlRollWindowDepth, // Rome
        DfPstateModeSelect(default 0xff, id 0xaeb84b12) : pub get DfPstateModeSelect : pub set DfPstateModeSelect, // value 0xff // Rome
        DfXgmiConfig(default 3, id 0xb0b6ad3e) : pub get DfXgmiLinkConfig : pub set DfXgmiLinkConfig, // Rome
        /// See DramTiming15_UMCWPHY0_mp0_umc0 CmdParLatency (for the DDR4 Registering Clock Driver).
        /// See also JESD82-31A DDR4 REGISTERING CLOCK DRIVER.
        /// See also <https://github.com/enjoy-digital/litedram/blob/master/litedram/init.py#L460>.
        MemRdimmTimingRcdF0Rc0FAdditionalLatency(default 0xff, id 0xd155798a) : pub get MemRdimmTimingCmdParLatency : pub set MemRdimmTimingCmdParLatency, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDataScramble(default 0, id 0x98aca5b4) : pub get u8 : pub set u8, // Rome (Obsolete)
        MemAutoRefreshFineGranMode(default 0, id 0x190305df) : pub get MemAutoRefreshFineGranMode : pub set MemAutoRefreshFineGranMode, // value 0 // Rome (Obsolete)
        UmaMode(default 0, id 0x1fb35295) : pub get UmaMode : pub set UmaMode, // value 2 // Rome (Obsolete)
        MemNvdimmPowerSource(default 0, id 0x286d0075) : pub get MemNvdimmPowerSource : pub set MemNvdimmPowerSource, // value 1 // Rome (Obsolete)
        MemDataPoison(default 0, id 0x48959473) : pub get MemDataPoison : pub set MemDataPoison, // value 1 // Rome (Obsolete)
        /// See PPR SwCmdThrotCyc
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        SwCmdThrotCycles(default 0, id 0xdcec8fcb) : pub get u8 : pub set u8, // value 0 // (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        OdtsCmdThrottleCycles(default 0, id 0x69318e90) : pub get u8 : pub set u8, // value 0x57 // Rome (Obsolete); TODO: Auto?
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemDramVrefRange(default 0, id 0xa8769655) : pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemCpuVrefRange(default 0, id 0x7627cb6d) : pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        MemControllerWritingCrcMode(default 0, id 0x7d1c6e46) : pub get MemControllerWritingCrcMode : pub set MemControllerWritingCrcMode, // value 0 // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemControllerWritingCrcMaxReplay(default 0, id 0x6bb1acf9) : pub get u8 : pub set u8, // value 8 // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemControllerWritingCrcLimit(default 0, id 0xc73a7692) : pub get u8 : pub set u8, // 0..=1 // Rome
        PmuTrainingMode(default 0xff, id 0xbd4a6afc) : pub get MemControllerPmuTrainingMode : pub set MemControllerPmuTrainingMode, // Rome (Obsolete)
        DfSysStorageAtTopOfMem(default 0xff, id 0x249e08d5) : pub get DfSysStorageAtTopOfMem : pub set DfSysStorageAtTopOfMem,

        // BMC Rome

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcSocket(default 0, id 0x846573f9) : pub get u8 : pub set u8, // value 0 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcDevice(default 0, id 0xd5bc5fc9) : pub get u8 : pub set u8, // value 5 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcFunction(default 0, id 0x1de4dd61) : pub get u8 : pub set u8, // value 2 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcStartLane(default 0, id 0xb88d87df) : pub get u8 : pub set u8, // value 0x81 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcEndLane(default 0, id 0x143f3963) : pub get u8 : pub set u8, // value 0x81 // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoPortSize(default 0, id 0xfc3f2520) : pub get u8 : pub set u8, // value 0 // legacy
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoBarToReplace(default 0, id 0x2c81a37f) : pub get u8 : pub set u8, // value 0; 0 to 6 // legacy
        BmcGen2TxDeemphasis(default 0xff, id 0xf30d142d) : pub get BmcGen2TxDeemphasis : pub set BmcGen2TxDeemphasis, // value 0xff
        BmcLinkSpeed(default 0, id 0x9c790f4b) : pub get BmcLinkSpeed : pub set BmcLinkSpeed, // value 1
        /// See <https://www.techdesignforums.com/practice/technique/common-pitfalls-in-pci-express-design/>.
        BmcRcbCheckingMode(default 0, id 0xae7f0df4) : pub get BmcRcbCheckingMode : pub set BmcRcbCheckingMode, // value 0xff // Rome
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum WordToken: {TokenEntryId::Word} {
        // PSP

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PspSyshubWatchdogTimerInterval(default 2600, id 0xedb5_e4c9) : pub get u16 : pub set u16, // in ms

        // Memory Controller

        CbsMemPowerDownDelay(default 0xff, id 0x1ebe_755a) : pub get CbsMemPowerDownDelay : pub set CbsMemPowerDownDelay,

        // Fch

        FchGppClkMap(default 0xffff, id 0xcd7e_6983) : pub get FchGppClkMap : pub set FchGppClkMap,

        // Unsorted Milan; obsolete and ungrouped; defaults wrong!

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        Dimm3DsSensorCritical(default 0, id 0x16b77f73) : pub get u16 : pub set u16, // value 0x50 // (Obsolete; added in Milan)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        Dimm3DsSensorUpper(default 0, id 0x2db877e4) : pub get u16 : pub set u16, // value 0x42 // (Obsolete; added in Milan)

        // Unsorted Rome; ungrouped; defaults wrong!

        EccSymbolSize(default 1, id 0x302d5c04) : pub get EccSymbolSize : pub set EccSymbolSize, // Rome (Obsolete)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubDramRate(default 0, id 0x9adddd6b) : pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16; or 0xff
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubL2Rate(default 0, id 0x2266c144) : pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubL3Rate(default 0, id 0xc0279ae0) : pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16; maybe 00h disable; maybe otherwise x: (x * 20 ns)
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubIcacheRate(default 0, id 0x99639ee4) : pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        ScrubDcacheRate(default 0, id 0xb398daa0) : pub get u16 : pub set u16, // Rome (Obsolete); <= 0x16
        /// See for example MCP9843/98243
        /// DIMM temperature sensor register at address 1
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorConfig(default 0x408, id 0x51e7b610) : pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 2
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorUpper(default 80, id 0xb5af557a) : pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 3
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorLower(default 10, id 0xc5ea38a0) : pub get u16 : pub set u16, // Rome (Obsolete)
        /// DIMM temperature sensor register at address 4
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DimmSensorCritical(default 95, id 0x38e9bf5d) : pub get u16 : pub set u16, // Rome (Obsolete)

        // BMC Rome

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        BmcVgaIoPort(default 0, id 0x6e06198) : pub get u16 : pub set u16, // value 0 // legacy
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum DwordToken: {TokenEntryId::Dword} {
        // Memory Controller

        MemBusFrequencyLimit(default 1600, id 0x3497_0a3c) : pub get MemBusFrequencyLimit : pub set MemBusFrequencyLimit,
        MemClockValue(default 0xffff_ffff, id 0xcc83_f65f) : pub get MemClockValue : pub set MemClockValue,
        MemUserTimingMode(default 0xff, id 0xfc56_0d7d) : pub get MemUserTimingMode : pub set MemUserTimingMode,
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemSelfHealBistTimeout(default 1_0000, id 0xbe75_97d4) : pub get u32 : pub set u32, // in ms
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemRestoreValidDays(default 30, id 0x6bd7_0482) : pub get u32 : pub set u32,

        // Ccx

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CcxMinSevAsid(default 1, id 0xa7c3_3753) : pub get u32 : pub set u32,

        // Fch

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        FchRom3BaseHigh(default 0, id 0x3e7d_5274) : pub get u32 : pub set u32,

        // Df

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        DfPciMmioSize(default 0x1000_0000, id 0x3d9b_7d7b) : pub get u32 : pub set u32,
        DfCakeCrcThresholdBounds(default 100, id 0x9258_cf45) : pub get DfCakeCrcThresholdBounds : pub set DfCakeCrcThresholdBounds, // default: 0.001%

        // Dxio

        DxioPhyParamVga(default 0xffff_ffff, id 0xde09_c43b) : pub get DxioPhyParamVga : pub set DxioPhyParamVga,
        DxioPhyParamPole(default 0xffff_ffff, id 0xb189_447e) : pub get DxioPhyParamPole : pub set DxioPhyParamPole,
        DxioPhyParamDc(default 0xffff_ffff, id 0x2066_7c30) : pub get DxioPhyParamDc : pub set DxioPhyParamDc,
        DxioPhyParamIqofc(default 0, id 0x7e60_69c5) : pub get DxioPhyParamIqofc : pub set DxioPhyParamIqofc, // TODO: Before using default, fix default.  It's possibly not correct.

        // MBIST for Milan and Rome; defaults wrong!

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelLo(default 0, id 0x745218ad) : pub get u32 : pub set u32, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistAggressorStaticLaneSelHi(default 0, id 0xfac9f48f) : pub get u32 : pub set u32, // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelLo(default 0, id 0x81880d15) : pub get u32 : pub set u32, // value 0 // Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemMbistTgtStaticLaneSelHi(default 0, id 0xaf669f33) : pub get u32 : pub set u32, // value 0 // Rome

        // Unsorted Milan; defaults wrong!

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        GnbOffRampStall(default 0, id 0x88b3c0d4) : pub get u32 : pub set u32, // value 0xc8
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PspMeasureConfig(default 0, id 0xdd3ad029) : pub get u32 : pub set u32, // Milan; reserved, must be 0

        // Unsorted Rome; ungrouped; defaults wrong!

        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemPowerDownMode(default 0, id 0x23dd2705) : pub get u32 : pub set u32, // power_down_mode; Rome
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUmaSize(default 0, id 0x37b1f8cf) : pub get u32 : pub set u32, // uma_size; Rome // FIXME enum
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        MemUmaAlignment(default 0, id 0x57ddf512) : pub get u32 : pub set u32, // value 0xffffc0 // Rome // FIXME enum?
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        PcieResetGpioPin(default 0, id 0x596663ac) : pub get u32 : pub set u32, // value 0xffffffff // Rome; FIXME: enum?
        #[cfg_attr(feature = "std", serde(serialize_with = "SerHex::<StrictPfx>::serialize", deserialize_with = "SerHex::<StrictPfx>::deserialize"))]
        CpuFetchFromSpiApBase(default 0, id 0xd403ea0e) : pub get u32 : pub set u32, // value 0xfff00000 // Rome
    }
}
make_token_accessors! {
    #[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
    #[non_exhaustive]
    pub enum BoolToken: {TokenEntryId::Bool} {
        // PSP

        PspTpPort(default 1, id 0x0460_abe8) : pub get bool : pub set bool,
        PspErrorDisplay(default 1, id 0xdc33_ff21) : pub get bool : pub set bool,
        PspEventLogDisplay(default 0, id 0x0c47_3e1c) : pub get bool : pub set bool,
        PspStopOnError(default 0, id 0xe702_4a21) : pub get bool : pub set bool,
        PspPsbAutoFuse(default 1, id 0x2fcd_70c9) : pub get bool : pub set bool,

        // Memory Controller

        MemEnableChipSelectInterleaving(default 1, id 0x6f81_a115) : pub get bool : pub set bool,
        MemEnableEccFeature(default 1, id 0xfa35_f040) : pub get bool : pub set bool,
        MemEnableParity(default 1, id 0x3cb8_cbd2) : pub get bool : pub set bool,
        MemEnableBankSwizzle(default 1, id 0x6414_d160) : pub get bool : pub set bool,
        MemIgnoreSpdChecksum(default 0, id 0x7d36_9dbc) : pub get bool : pub set bool,
        MemSpdReadOptimizationDdr4(default 1, id 0x6816_f949) : pub get bool : pub set bool,
        CbsMemSpdReadOptimizationDdr4(default 1, id 0x8d3a_b10e) : pub get bool : pub set bool,
        MemEnablePowerDown(default 1, id 0xbbb1_85a2) : pub get bool : pub set bool,
        MemTempControlledRefreshEnable(default 0, id 0xf051_e1c4) : pub get bool : pub set bool,
        MemOdtsCmdThrottleEnable(default 1, id 0xc073_6395) : pub get bool : pub set bool,
        MemSwCmdThrottleEnable(default 0, id 0xa29c_1cf9) : pub get bool : pub set bool,
        MemForcePowerDownThrottleEnable(default 1, id 0x1084_9d6c) : pub get bool : pub set bool,
        MemHoleRemapping(default 1, id 0x6a13_3ac5) : pub get bool : pub set bool,
        MemEnableBankGroupSwapAlt(default 1, id 0xa89d_1be8) : pub get bool : pub set bool,
        MemEnableBankGroupSwap(default 1, id 0x4692_0968) : pub get bool : pub set bool,
        MemDdrRouteBalancedTee(default 0, id 0xe68c_363d) : pub get bool : pub set bool,
        CbsMemWriteCrcRetryDdr4(default 0, id 0x25fb_6ea6) : pub get bool : pub set bool,
        CbsMemControllerWriteCrcEnableDdr4(default 0, id 0x9445_1a4b) : pub get bool : pub set bool,
        MemUncorrectedEccRetryDdr4(default 1, id 0xbff0_0125) : pub get bool : pub set bool,
        /// See Transparent Secure Memory Encryption in PPR
        MemTsmeModeMilan(default 1, id 0xd1fa_6660) : pub get bool : pub set bool,
        MemEccSyncFlood(default 0, id 0x88bd_40c2) : pub get bool : pub set bool,
        MemRestoreControl(default 0, id 0xfedb_01f8) : pub get bool : pub set bool,
        MemPostPackageRepairEnable(default 0, id 0xcdc0_3e4e) : pub get bool : pub set bool,

        // Ccx

        CcxPpinOptIn(default 0, id 0x6a67_00fd) : pub get bool : pub set bool,

        // Df

        /// [F17M30] needs it to be true
        DfGroupDPlatform(default 0, id 0x6831_8493) : pub get bool : pub set bool,

        // Dxio

        DxioVgaApiEnable(default 0, id 0xbd5a_a3c6) : pub get bool : pub set bool, // Milan

        // Misc

        ConfigureSecondPcieLink(default 0, id 0x7142_8092) : pub get bool : pub set bool,
        PerformanceTracing(default 0, id 0xf27a_10f0) : pub get bool : pub set bool, // Milan
        DisplayPmuTrainingResults(default 0, id 0x9e36_a9d4) : pub get bool : pub set bool,

        // MBIST for Milan and Rome; defaults wrong!

        MemMbistAggressorStaticLaneControl(default 0, id 0x77e6f2c9) : pub get bool : pub set bool, // Rome
        MemMbistTgtStaticLaneControl(default 0, id 0xe1cc135e) : pub get bool : pub set bool, // Rome

        // Capabilities for Rome; defaults wrong!

        MemUdimmCapable(default 0, id 0x3cf8a8ec) : pub get bool : pub set bool, // Rome
        MemSodimmCapable(default 0, id 0x7c61c187) : pub get bool : pub set bool, // Rome
        MemRdimmCapable(default 0, id 0x81726666) : pub get bool : pub set bool, // Rome
        MemLrdimmCapable(default 0, id 0x14fbf20) : pub get bool : pub set bool,
        MemDimmTypeLpddr3Capable(default 0, id 0xad96aa30) : pub get bool : pub set bool, // Rome
        MemDimmTypeDdr3Capable(default 0, id 0x789210c) : pub get bool : pub set bool, // Rome
        MemQuadRankCapable(default 0, id 0xe6dfd3dc) : pub get bool : pub set bool, // Rome

        // Unsorted Milan; defaults wrong!

        MemModeUnganged(default 0, id 0x3ce1180) : pub get bool : pub set bool,
        GnbAdditionalFeatures(default 0, id 0xf4c7789) : pub get bool : pub set bool, // Milan
        GnbAdditionalFeatureDsm(default 0, id 0x31a6afad) : pub get bool : pub set bool, // Milan
        VgaProgram(default 0, id 0x6570Eace) : pub get bool : pub set bool, // Milan
        MemNvdimmNDisable(default 0, id 0x941a92d4) : pub get bool : pub set bool, // Milan
        GnbAdditionalFeatureL3PerformanceBias(default 0, id 0xa003b37a) : pub get bool : pub set bool, // Milan
        GnbAdditionalFeatureDsmDetector(default 0, id 0xf5768cee) : pub get bool : pub set bool, // Milan

        // Unsorted Rome; ungrouped; defaults wrong!

        PcieResetControl(default 0, id 0xf7bb3451) : pub get bool : pub set bool, // Rome (Obsolete)
        MemDqsTrainingControl(default 0, id 0x3caaa3fa) : pub get bool : pub set bool, // Rome
        MemChannelInterleaving(default 0, id 0x48254f73) : pub get bool : pub set bool, // Rome
        MemPstate(default 0, id 0x56b93947) : pub get bool : pub set bool, // Rome
        /// Average the time between refresh requests
        MemAmp(default 0, id 0x592cb3ca) : pub get bool : pub set bool, // value 1 // amp_enable; Rome
        MemLimitMemoryToBelow1TiB(default 0, id 0x5e71e6d8) : pub get bool : pub set bool, // value 1 // Rome
        MemOcVddioControl(default 0, id 0x6cd36dbe) : pub get bool : pub set bool, // value 0 // Rome
        MemUmaAbove4GiB(default 0, id 0x77e41d2a) : pub get bool : pub set bool, // value 1 // Rome
        MemAutoRefreshsCountForThrottling(default 0, id 0x8f84dcb4) : pub get MemAutoRefreshsCountForThrottling : pub set MemAutoRefreshsCountForThrottling, // value 0 // Rome
        GeneralCapsuleMode(default 0, id 0x96176308) : pub get bool : pub set bool, // value 1 // Rome
        MemOnDieThermalSensor(default 0, id 0xaeb3f914) : pub get bool : pub set bool, // odts_en; Rome
        MemAllClocks(default 0, id 0xb95e0555) : pub get bool : pub set bool, // mem_all_clocks_on; Rome
        MemClear(default 0, id 0xc6acdb37) : pub get bool : pub set bool, // enable_mem_clr; Rome
        MemDdr4ForceDataMaskDisable(default 0, id 0xd68482b3) : pub get bool : pub set bool, // Rome
        MemEccRedirection(default 0, id 0xdede0e09) : pub get bool : pub set bool, // Rome
        MemTempControlledExtendedRefresh(default 0, id 0xf402f423) : pub get bool : pub set bool, // Rome (Obsolete)
        MotherBoardType0(default 0, id 0x536464b) : pub get bool : pub set bool, // value 0
        MctpRerouteEnable(default 0, id 0x79f2a8d5) : pub get bool : pub set bool, // value 0
        IohcMixedRwWorkaround(default 0, id 0xec3faf5a) : pub get bool : pub set bool, // value 0 // FIXME remove?

        // BMC Rome

        BmcVgaIoEnable(default 0, id 0x468d2cfa) : pub get bool : pub set bool, // value 0 // legacy
        BmcInitBeforeDram(default 0, id 0xfa94ee37) : pub get bool : pub set bool, // value 0
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
}
