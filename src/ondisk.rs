// This file contains the Apcb on-disk format.  Please only change it in coordination with the AMD PSP team.  Even then, you probably shouldn't.

//#![feature(trace_macros)] trace_macros!(true);

use byteorder::LittleEndian;
use core::mem::{replace, size_of};
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned, U16, U32, U64};
use core::convert::TryInto;
use paste::paste;
use crate::types::Error;
use crate::types::Result;
use crate::struct_accessors::{Getter, Setter, make_accessors};
use crate::token_accessors::{make_token_accessors, TokensMut, Tokens};
use crate::entry::EntryItemBody;
use crate::types::PriorityLevel;

/// AsBytes that we CAN implement ourselves.  Used for platform_specific_override and platform_tuning, which both are a sequence of variable-length records.
pub trait UnionAsBytes {
    fn as_bytes(&self) -> &[u8];
}

pub trait UnionFromBytes<'a>: Sized {
    fn skip_step(prefix: &[u8]) -> Option<usize>;
    fn from_bytes(world: &'a [u8]) -> Option<Self>;
}

/// There are (very few) Struct Entries like this: Header S0 S1 S2 S3.
/// This trait is implemented by structs that are used as a header of a sequence.  Then, the header structs specify (in their impl) what the (struct or enum) type of the sequence will be.
pub trait HeaderWithTail {
    type TailSequenceType: AsBytes + FromBytes;
}

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// If the item cannot be parsed, returns None and does not advance.
pub fn take_header_from_collection_mut<'a, T: Sized + FromBytes + AsBytes>(buf: &mut &'a mut [u8]) -> Option<&'a mut T> {
    let xbuf = replace(&mut *buf, &mut []);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item.into_mut())
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// Assuming that *BUF had been aligned to ALIGNMENT before the call, it also ensures that *BUF is aligned to ALIGNMENT after the call.
/// If the item cannot be parsed, returns None and does not advance.
pub fn take_body_from_collection_mut<'a>(buf: &mut &'a mut [u8], size: usize, alignment: usize) -> Option<&'a mut [u8]> {
    let xbuf = replace(&mut *buf, &mut []);
    if xbuf.len() >= size {
        let (item, xbuf) = xbuf.split_at_mut(size);
        if size % alignment != 0 && xbuf.len() >= alignment - (size % alignment) {
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

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// If the item cannot be parsed, returns None and does not advance.
pub fn take_header_from_collection<'a, T: Sized + FromBytes>(buf: &mut &'a [u8]) -> Option<&'a T> {
    let xbuf = replace(&mut *buf, &mut []);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item.into_ref())
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// Assuming that *BUF had been aligned to ALIGNMENT before the call, it also ensures that *BUF is aligned to ALIGNMENT after the call.
/// If the item cannot be parsed, returns None and does not advance.
pub fn take_body_from_collection<'a>(buf: &mut &'a [u8], size: usize, alignment: usize) -> Option<&'a [u8]> {
    let xbuf = replace(&mut *buf, &mut []);
    if xbuf.len() >= size {
        let (item, xbuf) = xbuf.split_at(size);
        if size % alignment != 0 && xbuf.len() >= alignment - (size % alignment) {
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

#[derive(FromBytes, AsBytes, Unaligned)]
#[repr(C, packed)]
pub struct V2_HEADER {
    pub signature: [u8; 4],
    pub header_size: U16<LittleEndian>, // == sizeof(V2_HEADER); but 128 for V3
    pub version: U16<LittleEndian>,     // == 0x30
    pub apcb_size: U32<LittleEndian>,
    pub unique_apcb_instance: U32<LittleEndian>,
    pub checksum_byte: u8,
    reserved1: [u8; 3],                // 0
    reserved2: [U32<LittleEndian>; 3], // 0
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

#[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
#[repr(C, packed)]
pub struct V3_HEADER_EXT {
    pub signature: [u8; 4],        // "ECB2"
    reserved_1: U16<LittleEndian>, // 0
    reserved_2: U16<LittleEndian>, // 0x10
    pub struct_version: U16<LittleEndian>,
    pub data_version: U16<LittleEndian>,
    pub ext_header_size: U32<LittleEndian>, // 96
    reserved_3: U16<LittleEndian>,          // 0
    reserved_4: U16<LittleEndian>,          // 0xFFFF
    reserved_5: U16<LittleEndian>,          // 0x40
    reserved_6: U16<LittleEndian>,          // 0x00
    reserved_7: [U32<LittleEndian>; 2],     // 0 0
    pub data_offset: U16<LittleEndian>,     // 0x58
    pub header_checksum: u8,
    reserved_8: u8,                     // 0
    reserved_9: [U32<LittleEndian>; 3], // 0 0 0
    pub integrity_sign: [u8; 32],
    reserved_10: [U32<LittleEndian>; 3], // 0 0 0
    pub signature_ending: [u8; 4],       // "BCBA"
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PspEntryId {
    BoardIdGettingMethod,

    Unknown(u16),
}

impl ToPrimitive for PspEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
                0x60 => Some(Self::BoardIdGettingMethod),
                x => Some(Self::Unknown(x as u16)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)

        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CcxEntryId {
    Unknown(u16),
}

impl ToPrimitive for CcxEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
                x => Some(Self::Unknown(x as u16)),
            }
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DfEntryId {
    SlinkConfig,
    XgmiTxEq,
    XgmiPhyOverride,
    Unknown(u16)
}

impl ToPrimitive for DfEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemoryEntryId {
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GnbEntryId {
    Unknown(u16),
}

impl ToPrimitive for GnbEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FchEntryId {
    Unknown(u16),
}

impl ToPrimitive for FchEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CbsEntryId {
    Unknown(u16),
}

impl ToPrimitive for CbsEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
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
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
            Self::from_u64(value)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TokenEntryId {
    Bool,
    Byte,
    Word,
    DWord,
    Unknown(u16),
}

impl ToPrimitive for TokenEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Bool => 0,
            Self::Byte => 1,
            Self::Word => 2,
            Self::DWord => 4,
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
                4 => Self::DWord,
                x => Self::Unknown(x as u16),
            })
        } else {
            None
        }
    }
    fn from_i64(value: i64) -> Option<Self> {
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
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
            GroupId::Memory => Self::Memory(MemoryEntryId::from_u16(type_id).unwrap()),
            GroupId::Gnb => Self::Gnb(GnbEntryId::from_u16(type_id).unwrap()),
            GroupId::Fch => Self::Fch(FchEntryId::from_u16(type_id).unwrap()),
            GroupId::Cbs => Self::Cbs(CbsEntryId::from_u16(type_id).unwrap()),
            GroupId::Oem => Self::Oem(OemEntryId::from_u16(type_id).unwrap()),
            GroupId::Token => Self::Token(TokenEntryId::from_u16(type_id).unwrap()),
            GroupId::Unknown(x) => Self::Unknown(x, RawEntryId::from_u16(type_id).unwrap()),
        }
    }
}

#[derive(FromBytes, AsBytes, Unaligned, Debug)]
#[repr(C, packed)]
pub struct GROUP_HEADER {
    pub signature: [u8; 4],
    pub group_id: U16<LittleEndian>,
    pub header_size: U16<LittleEndian>, // == sizeof(GROUP_HEADER)
    pub version: U16<LittleEndian>,     // == 0 << 4 | 1
    _reserved: U16<LittleEndian>,
    pub group_size: U32<LittleEndian>, // including header!
}

#[derive(Debug, PartialEq, FromPrimitive, Copy, Clone)]
pub enum ContextFormat {
    Raw = 0,
    SortAscending = 1,  // (sort by unit size)
    SortDescending = 2, // don't use
}

#[derive(Debug, PartialEq, FromPrimitive, Copy, Clone)]
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

bitfield! {
    #[derive(Copy, Clone)]
    pub struct PriorityLevels(u8);
    impl Debug;
    bool;
    hard_force, set_hard_force: 0;
    high, set_high: 1;
    medium, set_medium: 2;
    event_logging, set_event_logging: 3;
    low, set_low: 4;
    default, set_default: 5;
}

impl FromPrimitive for PriorityLevels {
    #[inline]
    fn from_u64(raw_value: u64) -> Option<Self> {
        if raw_value < 64 { // valid
            Some(Self(raw_value as u8))
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

impl ToPrimitive for PriorityLevels {
    #[inline]
    fn to_i64(&self) -> Option<i64> {
        Some(self.0.into())
    }
    #[inline]
    fn to_u64(&self) -> Option<u64> {
        Some(self.0.into())
    }
}

impl PriorityLevels {
    pub fn from_level(level: PriorityLevel) -> Self {
        let mut result = Self(0);
        match level {
            PriorityLevel::HardForce => {
                result.set_hard_force(true);
            },
            PriorityLevel::High => {
                result.set_high(true);
            },
            PriorityLevel::Medium => {
                result.set_medium(true);
            },
            PriorityLevel::EventLogging => {
                result.set_event_logging(true);
            },
            PriorityLevel::Low => {
                result.set_low(true);
            },
            PriorityLevel::Default => {
                result.set_default(true);
            },
        }
        result
    }
}

make_accessors! {
    #[derive(FromBytes, AsBytes, Unaligned, Debug)]
    #[repr(C, packed)]
    pub struct ENTRY_HEADER {
        // FIXME: de-pub those
        pub group_id: U16<LittleEndian>, // should be equal to the group's group_id
        pub entry_id: U16<LittleEndian>,  // meaning depends on context_type
        pub entry_size: U16<LittleEndian>, // including header
        pub instance_id: U16<LittleEndian>,
        pub context_type: u8,   // see ContextType enum
        pub context_format: u8, // see ContextFormat enum
        pub unit_size: u8,      // in Byte.  Applicable when ContextType == 2.  value should be 8
        pub priority_mask: u8 : pub get Result<PriorityLevels> : pub set PriorityLevels,
        pub key_size: u8, // Sorting key size; <= unit_size. Applicable when ContextFormat = 1. (or != 0)
        pub key_pos: u8,  // Sorting key position of the unit specified of UnitSize
        pub board_instance_mask: U16<LittleEndian>, // Board-specific Apcb instance mask
    }
}

impl Default for ENTRY_HEADER {
    fn default() -> Self {
        Self {
            group_id: 0u16.into(),                        // probably invalid
            entry_id: 0u16.into(),                         // probably invalid
            entry_size: (size_of::<Self>() as u16).into(), // probably invalid
            instance_id: 0u16.into(),                     // probably invalid
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

#[derive(FromBytes, AsBytes, Debug)]
#[repr(C, packed)]
pub struct TOKEN_ENTRY {
    pub key: U32<LittleEndian>,
    pub value: U32<LittleEndian>,
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
    /// Returns whether the ENTRY_ID can in principle house the impl of the trait EntryCompatible.
    /// Note: Usually, caller still needs to check whether the size is correct.  Since arrays are allowed and the ondisk structures then are array Element only, the payload size can be a natural multiple of the struct size.
    fn is_entry_compatible(_entry_id: EntryId, _prefix: &[u8]) -> bool {
        false
    }
}

// Starting here come the actual Entry formats (struct )

pub mod df {
    use super::*;
    use crate::struct_accessors::{Getter, Setter, make_accessors};
    use crate::types::Result;

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum SlinkRegionDescriptionInterleavingSize {
        B256 = 0,
        B512 = 1,
        B1024 = 2,
        B2048 = 3,
        Auto = 7,
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct SlinkRegionDescription {
            size: U64<LittleEndian> : pub get u64 : pub set u64,
            alignment: u8 : pub get u8 : pub set u8,
            socket: u8 : pub get u8 : pub set u8, // 0|1
            phys_nbio_map: u8 : pub get u8 : pub set u8, // bitmap
            interleaving: u8 : pub get Result<SlinkRegionDescriptionInterleavingSize> : pub set SlinkRegionDescriptionInterleavingSize,
            _reserved: [u8; 4],
        }
    }

    impl Default for SlinkRegionDescription {
        fn default() -> Self {
            Self {
                size: 0.into(),
                alignment: 0,
                socket: 0,
                phys_nbio_map: 0,
                interleaving: SlinkRegionDescriptionInterleavingSize::B256 as u8,
                _reserved: [0; 4],
            }
        }
    }

    impl SlinkRegionDescription {
        // Not sure why AMD still uses those--but it does use them, so whatever.
        pub fn dummy(socket: u8) -> SlinkRegionDescription {
            let mut result = Self::default();
            result.socket = socket;
            result
        }
    }

    // Rome only; even there, it's almost all 0s
    #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
    #[repr(C, packed)]
    pub struct SlinkConfig {
        pub regions: [SlinkRegionDescription; 4],
    }

    impl EntryCompatible for SlinkConfig {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Df(DfEntryId::SlinkConfig) => true,
                _ => false,
            }
        }
    }

    impl HeaderWithTail for SlinkConfig {
        type TailSequenceType = ();
    }

    impl SlinkConfig {
        pub fn new(regions: [SlinkRegionDescription; 4]) -> Self {
            Self {
                regions,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use static_assertions::const_assert;

        #[test]
        fn test_df_structs() {
            const_assert!(size_of::<SlinkConfig>() == 16 * 4);
            const_assert!(size_of::<SlinkRegionDescription>() == 16);
            assert!(offset_of!(SlinkRegionDescription, alignment) == 8);
            assert!(offset_of!(SlinkRegionDescription, socket) == 9);
            assert!(offset_of!(SlinkRegionDescription, phys_nbio_map) == 10);
            assert!(offset_of!(SlinkRegionDescription, interleaving) == 11);
        }

        #[test]
        fn test_df_struct_accessors() {
            let slink_config = SlinkConfig::new([SlinkRegionDescription::dummy(0), SlinkRegionDescription::dummy(0), SlinkRegionDescription::dummy(1), SlinkRegionDescription::dummy(1)]);
            assert!(slink_config.regions[0].socket == 0);
            assert!(slink_config.regions[1].socket == 0);
            assert!(slink_config.regions[2].socket == 1);
            assert!(slink_config.regions[3].socket == 1);
        }
    }
}

pub mod memory {
    use super::*;
    use crate::struct_accessors::{Getter, Setter, make_accessors, BU8};
    use crate::types::Result;
    use bitfield::Bit;
    use bitfield::BitRange;

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct DimmInfoSmbusElement {
            dimm_slot_present: BU8 : pub get Result<bool>, // if false, it's soldered-down and not a slot
            socket_id: u8 : pub get u8 : pub set u8,
            channel_id: u8 : pub get u8 : pub set u8,
            dimm_id: u8 : pub get u8 : pub set u8,
            // For soldered-down DIMMs, SPD data is hardcoded in APCB (in entry MemoryEntryId::SpdInfo), and DIMM_SMBUS_ADDRESS here is the index in the structure for SpdInfo.
            dimm_smbus_address: u8,
            i2c_mux_address: u8,
            mux_control_address: u8,
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
                },
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
                 },
             }
        }
        pub fn dimm_spd_info_index(&self) -> Option<u8> {
             match self.dimm_slot_present {
                 BU8(0) => {
                     Some(self.dimm_smbus_address)
                 },
                 _ => {
                     None
                 },
             }
        }
        pub fn set_dimm_spd_info_index(&mut self, value: u8) -> Result<()> {
             self.dimm_smbus_address = match self.dimm_slot_present {
                 BU8(0) => value,
                 _ => {
                     return Err(Error::EntryTypeMismatch);
                 },
             };
             Ok(())
        }
        pub fn mux_control_address(&self) -> Option<u8> {
            match self.mux_control_address {
                0xFF => None,
                x => Some(x),
            }
        }
        pub fn set_mux_control_address(&mut self, value: Option<u8>) -> Result<()> {
            self.mux_control_address = match value {
                None => 0xFF,
                Some(0xFF) => {
                    return Err(Error::EntryTypeMismatch);
                },
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
                },
                Some(x) => x,
             };
             Ok(())
        }
    }

    // An union of one variant.
    impl UnionAsBytes for DimmInfoSmbusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }
    impl DimmInfoSmbusElement {
        pub fn not_present() -> Self {
            Self {
                dimm_slot_present: BU8(0),
                socket_id: 0,
                channel_id: 0,
                dimm_id: 0,
                dimm_smbus_address: 0,
                i2c_mux_address: 0,
                mux_control_address: 0,
                mux_channel: 0,
            }
        }
        pub fn new_slot(socket_id: u8, channel_id: u8, dimm_id: u8, dimm_smbus_address: u8, i2c_mux_address: Option<u8>, mux_control_address: Option<u8>, mux_channel: Option<u8>) -> Result<Self> {
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
        pub fn new_soldered_down(socket_id: u8, channel_id: u8, dimm_id: u8, dimm_spd_info_index: u8, i2c_mux_address: Option<u8>, mux_control_address: Option<u8>, mux_channel: Option<u8>) -> Result<Self> {
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
            match entry_id {
                EntryId::Memory(MemoryEntryId::DimmInfoSmbus) => true,
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct AblConsoleOutControl {
            enable_console_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_flow_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_setreg_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_getreg_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_status_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_pmu_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_pmu_sram_read_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_pmu_sram_write_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_test_verbose_logging: BU8 : pub get Result<bool> : pub set bool,
            enable_mem_basic_output_logging: BU8 : pub get Result<bool> : pub set bool,
            _reserved: U16<LittleEndian>,
            abl_console_port: U32<LittleEndian> : pub get u32 : pub set u32,
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct AblBreakpointControl {
            enable_breakpoint: BU8 : pub get Result<bool> : pub set bool,
            break_on_all_dies: BU8 : pub get Result<bool> : pub set bool,
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

    #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
    #[repr(C, packed)]
    pub struct ConsoleOutControl {
        pub abl_console_out_control: AblConsoleOutControl,
        pub abl_breakpoint_control: AblBreakpointControl,
        _reserved: U16<LittleEndian>,
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
            match entry_id {
                EntryId::Memory(MemoryEntryId::ConsoleOutControl) => true,
                _ => false,
            }
        }
    }

    impl HeaderWithTail for ConsoleOutControl {
        type TailSequenceType = ();
    }

    impl ConsoleOutControl {
        pub fn new(abl_console_out_control: AblConsoleOutControl, abl_breakpoint_control: AblBreakpointControl) -> Self {
            Self {
                abl_console_out_control,
                abl_breakpoint_control,
                _reserved: 0.into(),
            }
        }
    }

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum PortType {
        PcieHt0 = 0,
        PcieHt1 = 2,
        PcieMmio = 5,
        FchHtIo = 6,
        FchMmio = 7,
    }

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum PortSize {
        Size8Bit = 1,
        Size16Bit = 2,
        Size32Bit = 4,
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ExtVoltageControl {
            enable: BU8 : pub get Result<bool> : pub set bool,
            _reserved: [u8; 3],
            input_port: U32<LittleEndian> : pub get u32 : pub set u32,
            output_port: U32<LittleEndian> : pub get u32 : pub set u32,
            input_port_size: U32<LittleEndian> : pub get Result<PortSize> : pub set PortSize,
            output_port_size: U32<LittleEndian> : pub get Result<PortSize> : pub set PortSize,
            input_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // default: 6 (FCH)
            output_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // default: 6 (FCH)
            clear_acknowledgement: BU8 : pub get Result<bool> : pub set bool,
            _reserved_2: [u8; 3],
        }
    }

    impl EntryCompatible for ExtVoltageControl {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::ExtVoltageControl) => true,
                _ => false,
            }
        }
    }

    impl HeaderWithTail for ExtVoltageControl {
        type TailSequenceType = ();
    }

    impl Default for ExtVoltageControl {
        fn default() -> Self {
            Self {
                enable: BU8(0),
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

    bitfield! {
        pub struct Ddr4DimmRanks(u32);
        impl Debug;
        bool;
        #[inline]
        pub unpopulated, set_unpopulated: 0;
        #[inline]
        pub single_rank, set_single_rank: 1;
        #[inline]
        pub dual_rank, set_dual_rank: 2;
        #[inline]
        pub quad_rank, set_quad_rank: 3;
    }

macro_rules! impl_bitfield_primitive_conversion {
    ($bitfield:ty, $valid_bits:expr) => (
    impl $bitfield {
        const VALID_BITS: u32 = $valid_bits;
        pub const fn all_reserved_bits_are_unused(value: u32) -> bool {
            value & !Self::VALID_BITS == 0
        }
    }
    impl FromPrimitive for $bitfield {
        #[inline]
        fn from_u64(raw_value: u64) -> Option<Self> {
            if raw_value > 0xffff_ffff {
                None
            } else {
                let raw_value: u32 = match raw_value.try_into() {
                                         Ok(v) => {
                                             v
                                         },
                                         Err(_) => {
                                             return None;
                                         }
                                     };
                if Self::all_reserved_bits_are_unused(raw_value) { // valid
                    Some(Self(raw_value as u32))
                } else {
                    None
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

    impl ToPrimitive for $bitfield {
        #[inline]
        fn to_i64(&self) -> Option<i64> {
            Some(self.0.into())
        }
        #[inline]
        fn to_u64(&self) -> Option<u64> {
            Some(self.0.into())
        }
    }
)}
    impl_bitfield_primitive_conversion!(Ddr4DimmRanks, 0b111);
    impl Ddr4DimmRanks {
        pub fn new() -> Self {
            Self(0)
        }
    }

    bitfield! {
        pub struct LrdimmDdr4DimmRanks(u32);
        impl Debug;
        bool;
        #[inline]
        pub unpopulated, set_unpopulated: 0;
        #[inline]
        pub lr, set_lr: 1;
    }
    impl_bitfield_primitive_conversion!(LrdimmDdr4DimmRanks, 0b11);

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum CadBusClkDriveStrength {
        Auto = 0xFF,
        Strength120Ohm = 0,
        Strength60Ohm = 1,
        Strength40Ohm = 3,
        Strength30Ohm = 7,
        Strength24Ohm = 15,
        Strength20Ohm = 31,
    }

    type CadBusAddressCommandDriveStrength = CadBusClkDriveStrength;
    type CadBusCkeDriveStrength = CadBusClkDriveStrength;
    type CadBusCsOdtDriveStrength = CadBusClkDriveStrength;

    bitfield! {
        pub struct DdrRates(u32);
        impl Debug;
        bool;
        // Note: Bit index is (x/2)//66 of ddrx
        #[inline]
        pub ddr400, set_ddr400: 3;
        #[inline]
        pub ddr533, set_ddr533: 4;
        #[inline]
        pub ddr667, set_ddr667: 5;
        #[inline]
        pub ddr800, set_ddr800: 6;
        #[inline]
        pub ddr1066, set_ddr1066: 8;
        #[inline]
        pub ddr1333, set_ddr1333: 10;
        #[inline]
        pub ddr1600, set_ddr1600: 12;
        #[inline]
        pub ddr1866, set_ddr1866: 14;
        // AMD-missing pub ddr2100, set_ddr2100: 15,
        #[inline]
        pub ddr2133, set_ddr2133: 16;
        #[inline]
        pub ddr2400, set_ddr2400: 18;
        #[inline]
        pub ddr2667, set_ddr2667: 20;
        // AMD-missing pub ddr2800, set_ddr2800: 21,
        #[inline]
        pub ddr2933, set_ddr2933: 22;
        // AMD-missing pub ddr3066, set_ddr3066: 23,
        #[inline]
        pub ddr3200, set_ddr3200: 24;
        // AMD-missing pub ddr3334, set_ddr3334: 25,
        // AMD-missing pub ddr3466, set_ddr3466: 26,
        // AMD-missing pub ddr3600, set_ddr3600: 27,
        // AMD-missing pub ddr3734, set_ddr3734: 28,
        // AMD-missing pub ddr3866, set_ddr3866: 29,
        // AMD-missing pub ddr4000, set_ddr4000: 30,
        // AMD-missing pub ddr4200, set_ddr4200: 31,
        // AMD-missing pub ddr4266, set_ddr4266: 32,
        // AMD-missing pub ddr4334, set_ddr4334: 32,
        // AMD-missing pub ddr4400, set_ddr4400: 33,
    }
    impl_bitfield_primitive_conversion!(DdrRates, 0b0000_0001_0101_0101_0101_0101_0111_1000);
    impl DdrRates {
        pub fn new() -> Self {
            Self(0)
        }
    }

    bitfield! {
        pub struct DimmsPerChannel(u32);
        impl Debug;
        bool;
        pub one_dimm, set_one_dimm: 0;
        pub two_dimms, set_two_dimms: 1;
        pub three_dimms, set_three_dimms: 2;
        pub four_dimms, set_four_dimms: 3;
    }
    //impl_bitfield_primitive_conversion!(DimmsPerChannel, 0b111);
    impl DimmsPerChannel {
        const VALID_BITS: u32 = 0b111;
        pub const fn all_reserved_bits_are_unused(value: u32) -> bool {
            value & !Self::VALID_BITS == 0
        }
        pub fn new() -> Self {
            Self(0)
        }
        pub fn dont_care() -> Self {
            Self(0xFF)
        }
        pub fn no_slot() -> Self {
            Self(0xF0)
        }
        pub fn is_no_slot(&self) -> bool {
            self.0 == 0xF0
        }
        pub fn is_dont_care(&self) -> bool {
            self.0 == 0xFF
        }
    }
    impl FromPrimitive for DimmsPerChannel {
        #[inline]
        fn from_u64(raw_value: u64) -> Option<Self> {
            if raw_value > 0xffff_ffff {
                None
            } else {
                let raw_value = raw_value as u32;
                if raw_value == 0xFF || raw_value == 0xF0 || Self::all_reserved_bits_are_unused(raw_value) {
                    Some(Self(raw_value.into()))
                } else {
                    None
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
            let value = self.0;
            if value == 0xFF || Self::all_reserved_bits_are_unused(value) {
                Some(value.into())
            } else {
                None
            }
        }
        #[inline]
        fn to_u64(&self) -> Option<u64> {
            Some(self.to_i64()? as u64)
        }
    }

    bitfield! {
        pub struct RdimmDdr4Voltages(u32);
        impl Debug;
        bool;
        pub v_1_2, set_v_1_2: 0;
        // all = 7
    }
    impl_bitfield_primitive_conversion!(RdimmDdr4Voltages, 0b111);

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct RdimmDdr4CadBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rates: U32<LittleEndian> : pub get Result<DdrRates> : pub set DdrRates,
            vdd_io: U32<LittleEndian> : pub get Result<RdimmDdr4Voltages> : pub set RdimmDdr4Voltages,
            dimm0_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,
            dimm1_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,

            gear_down_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved: U16<LittleEndian>,
            slow_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved_2: U16<LittleEndian>,
            address_command_control: U32<LittleEndian> : pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get Result<CadBusCkeDriveStrength> : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get Result<CadBusCsOdtDriveStrength> : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get Result<CadBusAddressCommandDriveStrength> : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get Result<CadBusClkDriveStrength> : pub set CadBusClkDriveStrength,
        }
    }

    // An union of one variant.
    impl UnionAsBytes for RdimmDdr4CadBusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
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

    impl EntryCompatible for RdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                // definitely not: EntryId::Memory(MemoryEntryId::PsUdimmDdr4CadBus) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus) => true,
                // TODO: Check EntryId::Memory(MemoryEntryId::PsSodimmDdr4CadBus) => true
                // Definitely not: PsDramdownDdr4CadBus.
                _ => false,
            }
        }
    }

    bitfield! {
        pub struct UdimmDdr4Voltages(u32);
        impl Debug;
        bool;
        pub v_1_5, set_v_1_5: 0;
        pub v_1_35, set_v_1_35: 1;
        pub v_1_25, set_v_1_25: 2;
        // all = 7
    }
    impl_bitfield_primitive_conversion!(UdimmDdr4Voltages, 0b111);

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct UdimmDdr4CadBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rates: U32<LittleEndian> : pub get Result<DdrRates> : pub set DdrRates,
            vdd_io: U32<LittleEndian> : pub get Result<UdimmDdr4Voltages> : pub set UdimmDdr4Voltages,
            dimm0_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,
            dimm1_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,

            gear_down_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved: U16<LittleEndian>,
            slow_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved_2: U16<LittleEndian>,
            address_command_control: U32<LittleEndian> : pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get Result<CadBusCkeDriveStrength> : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get Result<CadBusCsOdtDriveStrength> : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get Result<CadBusAddressCommandDriveStrength> : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get Result<CadBusClkDriveStrength> : pub set CadBusClkDriveStrength,
        }
    }

    // An union of one variant.
    impl UnionAsBytes for UdimmDdr4CadBusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
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

    impl EntryCompatible for UdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4CadBus) => true,
                // definitely not: EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus) => true,
                // TODO: Check EntryId::Memory(MemoryEntryId::PsSodimmDdr4CadBus) => true
                // Definitely not: PsDramdownDdr4CadBus.
                _ => false,
            }
        }
    }

    bitfield! {
        pub struct LrdimmDdr4Voltages(u32);
        impl Debug;
        bool;
        pub v_1_2, set_v_1_2: 0;
        // all = 7
    }
    impl_bitfield_primitive_conversion!(LrdimmDdr4Voltages, 0b111);

    // Usually an array of those is used
    make_accessors! {
        /// Control/Address Bus Element
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4CadBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rates: U32<LittleEndian> : pub get Result<DdrRates> : pub set DdrRates,
            vdd_io: U32<LittleEndian> : pub get Result<LrdimmDdr4Voltages> : pub set LrdimmDdr4Voltages,
            dimm0_ranks: U32<LittleEndian> : pub get Result<LrdimmDdr4DimmRanks> : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks: U32<LittleEndian> : pub get Result<LrdimmDdr4DimmRanks> : pub set LrdimmDdr4DimmRanks,

            gear_down_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved: U16<LittleEndian>,
            slow_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved_2: U16<LittleEndian>,
            address_command_control: U32<LittleEndian> : pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get Result<CadBusCkeDriveStrength> : pub set CadBusCkeDriveStrength,
            cs_odt_drive_strength: u8 : pub get Result<CadBusCsOdtDriveStrength> : pub set CadBusCsOdtDriveStrength,
            address_command_drive_strength: u8 : pub get Result<CadBusAddressCommandDriveStrength> : pub set CadBusAddressCommandDriveStrength,
            clk_drive_strength: u8 : pub get Result<CadBusClkDriveStrength> : pub set CadBusClkDriveStrength,
        }
    }

    // An union of one variant.
    impl UnionAsBytes for LrdimmDdr4CadBusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
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

    impl EntryCompatible for LrdimmDdr4CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4CadBus) => true,
                _ => false,
            }
        }
    }

    // Those are all divisors of 240, except for the 34
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum RttNom {
        RttOff = 0,
        Rtt60Ohm = 1,
        Rtt120Ohm = 2,
        Rtt40Ohm = 3,
        Rtt240Ohm = 4,
        Rtt48Ohm = 5,
        Rtt80Ohm = 6,
        Rtt34Ohm = 7,
    }
    type RttPark = RttNom;
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum RttWr {
        RttOff = 0,
        Rtt120Ohm = 1,
        Rtt240Ohm = 2,
        RttFloating = 3,
        Rtt80Ohm = 4,
    }

    // See <https://www.micron.com/-/media/client/global/documents/products/data-sheet/dram/ddr4/8gb_auto_ddr4_dram.pdf>
    // Usually an array of those is used
    // Note: This structure is not used for soldered-down DRAM!
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct Ddr4DataBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rates: U32<LittleEndian> : pub get Result<DdrRates> : pub set DdrRates,
            vdd_io: U32<LittleEndian> : pub get Result<RdimmDdr4Voltages> : pub set RdimmDdr4Voltages,
            dimm0_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,
            dimm1_ranks: U32<LittleEndian> : pub get Result<Ddr4DimmRanks> : pub set Ddr4DimmRanks,

            rtt_nom: U32<LittleEndian> : pub get Result<RttNom> : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr: U32<LittleEndian> : pub get Result<RttWr> : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park: U32<LittleEndian> : pub get Result<RttPark> : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data
            dqs_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data strobe (bit clock)
            odt_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for on-die termination
            pmu_phy_vref: U32<LittleEndian> : pub get u32 : pub set u32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            vref_dq: U32<LittleEndian> : pub get u32 : pub set u32, // MR6 vref calibration value; 23|30|32
        }
    }

    // An union of one variant.
    impl UnionAsBytes for Ddr4DataBusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4DataBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rates: U32<LittleEndian> : pub get Result<DdrRates> : pub set DdrRates,
            vdd_io: U32<LittleEndian> : pub get Result<LrdimmDdr4Voltages> : pub set LrdimmDdr4Voltages,
            dimm0_ranks: U32<LittleEndian> : pub get Result<LrdimmDdr4DimmRanks> : pub set LrdimmDdr4DimmRanks,
            dimm1_ranks: U32<LittleEndian> : pub get Result<LrdimmDdr4DimmRanks> : pub set LrdimmDdr4DimmRanks,

            rtt_nom: U32<LittleEndian> : pub get Result<RttNom> : pub set RttNom, // contains nominal on-die termination mode (not used on writes)
            rtt_wr: U32<LittleEndian> : pub get Result<RttWr> : pub set RttWr, // contains dynamic on-die termination mode (used on writes)
            rtt_park: U32<LittleEndian> : pub get Result<RttPark> : pub set RttPark, // contains ODT termination resistor to be used when ODT is low
            dq_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data
            dqs_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data strobe (bit clock)
            odt_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for on-die termination
            pmu_phy_vref: U32<LittleEndian> : pub get u32 : pub set u32,
            // See <https://www.systemverilog.io/ddr4-initialization-and-calibration>
            vref_dq: U32<LittleEndian> : pub get u32 : pub set u32, // MR6 vref calibration value; 23|30|32
        }
    }

    // An union of one variant.
    impl UnionAsBytes for LrdimmDdr4DataBusElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
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

    // ACTUAL 1/T, where T is one period.  For DDR, that means DDR400 has frequency 200.
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
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct MaxFreqElement {
            dimm_slots_per_channel: u8 : pub get Result<DimmsPerChannel> : pub set DimmsPerChannel,
            _reserved: u8,
            conditions: [U16<LittleEndian>; 4], // number of dimm on a channel, number of single-rank dimm, number of dual-rank dimm, number of quad-rank dimm // FIXME make accessible
            speeds: [U16<LittleEndian>; 3], // speed limit with voltage 1.5 V, 1.35 V, 1.25 V // FIXME make accessible
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
           DdrSpeed::from_u16(self.speeds[0].get()).ok_or_else(|| Error::EntryTypeMismatch)
       }
       pub fn set_speed(&mut self, value: DdrSpeed) {
           self.speeds[0].set(value.to_u16().unwrap())
       }
       pub fn new(dimm_slots_per_channel: DimmsPerChannel, dimm_count: u16, single_rank_count: u16, dual_rank_count: u16, quad_rank_count: u16, speed_0: DdrSpeed) -> Self {
           Self {
               dimm_slots_per_channel: dimm_slots_per_channel.to_u8().unwrap(),
               conditions: [dimm_count.into(), single_rank_count.into(), dual_rank_count.into(), quad_rank_count.into()],
               speeds: [speed_0.to_u16().unwrap().into(), DdrSpeed::UnsupportedMilan.to_u16().unwrap().into(), DdrSpeed::UnsupportedMilan.to_u16().unwrap().into()],
               .. Self::default()
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
                // Definitely not: EntryId::Memory(MemoryEntryId::PsLrdimmDdr4) => true
                // TODO: Check EntryId::Memory(PsSodimmDdr4MaxFreq) => true
                // Definitely not: EntryId::PsDramdownDdr4MaxFreq => true

                EntryId::Memory(MemoryEntryId::PsUdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4StretchFreq) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4StretchFreq) => true,
                _ => false,
            }
        }
    }

    // An union of one variant.
    impl UnionAsBytes for MaxFreqElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }

    // Usually an array of those is used
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct LrMaxFreqElement {
            dimm_slots_per_channel: u8 : pub get u8 : pub set u8,
            _reserved: u8,
            pub conditions: [U16<LittleEndian>; 4], // maybe: number of dimm on a channel, 0, number of lr dimm, 0 // FIXME: Make accessible
            pub speeds: [U16<LittleEndian>; 3], // maybe: speed limit with voltage 1.5 V, 1.35 V, 1.25 V; FIXME: Make accessible
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

    impl EntryCompatible for LrMaxFreqElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4MaxFreq) => true,
                _ => false,
            }
        }
    }

    // An union of one variant.
    impl UnionAsBytes for LrMaxFreqElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug, Clone, Copy)]
        #[repr(C, packed)]
        pub struct Gpio {
            pin: u8 : pub get u8 : pub set u8, // in FCH
            iomux_control: u8 : pub get u8 : pub set u8, // how to configure that pin
            bank_control: u8 : pub get u8 : pub set u8, // how to configure bank control
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
    }

    bitfield! {
        #[derive(FromBytes, AsBytes, Unaligned, Clone, Copy, PartialEq)]
        #[repr(C, packed)]
        pub struct ErrorOutControlBeepCodePeakAttr(U32<LittleEndian>);
        no default BitRange;
        impl Debug;
        u16;
        pub repeat_count, set_repeat_count: 11, 8;
        pub pulse_width, set_pulse_width: 7, 5; // in units of 0.1 s
        pub peak_count, set_peak_count: 4, 0;
    }

    impl BitRange<u16> for ErrorOutControlBeepCodePeakAttr {
        fn bit_range(&self, msb: usize, lsb: usize) -> u16 {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 15);
            let mask = (1u32 << width) - 1;
            ((self.0.get() >> lsb) & mask) as u16
        }
        fn set_bit_range(&mut self, msb: usize, lsb: usize, value: u16) {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 15);
            let mask = (1u32 << width) - 1;
            assert!(u32::from(value) <= mask);
            let mut dest = self.0.get();
            dest &=! (mask << lsb);
            dest |= (value as u32) << lsb;
            self.0.set(dest);
        }
    }

    impl Bit for ErrorOutControlBeepCodePeakAttr {
        fn bit(&self, index: usize) -> bool {
            match self.0.get() & (1u32 << index) {
                0 => false,
                _ => true,
            }
        }
        fn set_bit(&mut self, index: usize, value: bool) {
            match value {
                true => {
                    self.0.set(self.0.get() | (1u32 << index));
                },
                false => {
                    self.0.set(self.0.get() &! (1u32 << index));
                },
            }
        }
    }

    impl ErrorOutControlBeepCodePeakAttr {
        /// PULSE_WIDTH: in units of 0.1 s
        pub fn new(peak_count: u16, pulse_width: u16, repeat_count: u16) -> Option<Self> {
            let mut result = Self(0.into());
            if peak_count < 32 {
                result.set_peak_count(peak_count);
            } else {
                return None;
            }
            if pulse_width < 8 {
                result.set_pulse_width(pulse_width);
            } else {
                return None;
            }
            if repeat_count < 16 {
                result.set_repeat_count(repeat_count);
            } else {
                return None;
            }
            Some(result)
        }
    }

    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum ErrorOutControlBeepCodeErrorType {
        General = 3,
        Memory = 4,
        Df = 5,
        Ccx = 6,
        Gnb = 7,
        Psp = 8,
        Smu = 9,
    }
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ErrorOutControlBeepCode {
            error_type: U16<LittleEndian>,
            peak_map: U16<LittleEndian> : pub get u16 : pub set u16,
            pub peak_attr: ErrorOutControlBeepCodePeakAttr,
        }
    }
    impl ErrorOutControlBeepCode {
        pub fn error_type(&self) -> Result<ErrorOutControlBeepCodeErrorType> {
            Ok(ErrorOutControlBeepCodeErrorType::from_u16((self.error_type.get() & 0xF000) >> 12).ok_or_else(|| Error::EntryTypeMismatch)?)
        }
        pub fn set_error_type(&mut self, value: ErrorOutControlBeepCodeErrorType) {
            self.error_type.set((self.error_type.get() & 0x0FFF) | (value.to_u16().unwrap() << 12));
        }
        pub fn new(error_type: ErrorOutControlBeepCodeErrorType, peak_map: u16, peak_attr: ErrorOutControlBeepCodePeakAttr) -> Self {
            Self {
                error_type: ((error_type.to_u16().unwrap() << 12) | 0xFFF).into(),
                peak_map: peak_map.into(),
                peak_attr
            }
        }
    }

    macro_rules! define_ErrorOutControl {($struct_name:ident, $padding_before_gpio:expr, $padding_after_gpio: expr) => (

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct $struct_name {
            enable_error_reporting: BU8 : pub get Result<bool> : pub set bool,
            enable_error_reporting_gpio: BU8,
            enable_error_reporting_beep_codes: BU8 : pub get Result<bool> : pub set bool,
            enable_using_handshake: BU8 : pub get Result<bool> : pub set bool, // otherwise see output_delay
            input_port: U32<LittleEndian> : pub get u32 : pub set u32, // for handshake
            output_delay: U32<LittleEndian> : pub get u32 : pub set u32, // if no handshake; in units of 10 ns.
            output_port: U32<LittleEndian> : pub get u32 : pub set u32,
            stop_on_first_fatal_error: BU8: pub get Result<bool> : pub set bool,
            _reserved: [u8; 3],
            input_port_size: U32<LittleEndian> : pub get Result<PortSize> : pub set PortSize,
            output_port_size: U32<LittleEndian> : pub get Result<PortSize> : pub set PortSize,
            input_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // PortType; default: 6
            output_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // PortType; default: 6
            clear_acknowledgement: BU8 : pub get Result<bool> : pub set bool,
            _reserved_before_gpio: [u8; $padding_before_gpio],
            error_reporting_gpio: Gpio,
            _reserved_after_gpio: [u8; $padding_after_gpio],
            pub beep_code_table: [ErrorOutControlBeepCode; 8], // FIXME: Make accessible
            enable_heart_beat: BU8 : pub get Result<bool> : pub set bool,
            enable_power_good_gpio: BU8,
            power_good_gpio: Gpio,
            _reserved_end: [u8; 3], // pad
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
        type TailSequenceType = ();
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
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(8, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x4fff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x5fff.into(),
                        peak_map: 3.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x6fff.into(),
                        peak_map: 4.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x7fff.into(),
                        peak_map: 5.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x8fff.into(),
                        peak_map: 6.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0x9fff.into(),
                        peak_map: 7.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutControlBeepCode {
                        error_type: 0xffff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutControlBeepCodePeakAttr::new(4, 0, 0).unwrap(),
                    },
                ],
                enable_heart_beat: BU8(1),
                enable_power_good_gpio: BU8(0),
                power_good_gpio: Gpio::new(0, 0, 0), // probably invalid
                _reserved_end: [0; 3],
            }
        }
    }

)}

    define_ErrorOutControl!(ErrorOutControl116, 3, 1); // Milan
    define_ErrorOutControl!(ErrorOutControl112, 0, 0);

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct Ddr4OdtPatElement {
            dimm_rank_bitmap: U32<LittleEndian>,
            cs0_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32, // TODO: parts: CS0, write?
            cs1_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32, // TODO: parts: CS1, write?
            cs2_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32, // TODO: parts: CS2, write?
            cs3_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32, // TODO: parts: CS3, write?
        }
    }

    impl Ddr4OdtPatElement {
        pub fn dimm0_rank(&self) -> Result<Ddr4DimmRanks> {
            Ddr4DimmRanks::from_u32(self.dimm_rank_bitmap.get() & 15).ok_or_else(|| Error::EntryTypeMismatch)
        }
        pub fn set_dimm0_rank(&mut self, value: Ddr4DimmRanks) {
            let value = value.to_u32().unwrap();
            let x = self.dimm_rank_bitmap.get();
            self.dimm_rank_bitmap.set((x &! 0xF) | value);
        }
        pub fn dimm1_rank(&self) -> Result<Ddr4DimmRanks> {
            Ddr4DimmRanks::from_u32((self.dimm_rank_bitmap.get() >> 4) & 15).ok_or_else(|| Error::EntryTypeMismatch)
        }
        pub fn set_dimm1_rank(&mut self, value: Ddr4DimmRanks) {
            let value = value.to_u32().unwrap();
            let x = self.dimm_rank_bitmap.get();
            self.dimm_rank_bitmap.set((x &! 0xF0) | (value << 4));
        }
    }

    impl Default for Ddr4OdtPatElement {
        fn default() -> Self {
            Self {
                dimm_rank_bitmap: (1 | (2 << 4)).into(),
                cs0_odt_pattern: 0.into(),
                cs1_odt_pattern: 0.into(),
                cs2_odt_pattern: 0.into(),
                cs3_odt_pattern: 0.into(),
            }
        }
    }

    impl EntryCompatible for Ddr4OdtPatElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                // definitely not EntryId::Memory(MemoryEntryId::PsLrdimmDdr4OdtPat) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4OdtPat) => true,
                _ => false,
            }
        }
    }

    // An union of one variant.
    impl UnionAsBytes for Ddr4OdtPatElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct LrdimmDdr4OdtPatElement {
            dimm_rank_bitmap: U32<LittleEndian>,
            cs0_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs1_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs2_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs3_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
        }
    }

    impl LrdimmDdr4OdtPatElement {
        pub fn dimm0_rank(&self) -> Result<LrdimmDdr4DimmRanks> {
            LrdimmDdr4DimmRanks::from_u32(self.dimm_rank_bitmap.get() & 15).ok_or_else(|| Error::EntryTypeMismatch)
        }
        pub fn set_dimm0_rank(&mut self, value: LrdimmDdr4DimmRanks) {
            let value = value.to_u32().unwrap();
            let x = self.dimm_rank_bitmap.get();
            self.dimm_rank_bitmap.set((x &! 0xF) | value);
        }
        pub fn dimm1_rank(&self) -> Result<LrdimmDdr4DimmRanks> {
            LrdimmDdr4DimmRanks::from_u32((self.dimm_rank_bitmap.get() >> 4) & 15).ok_or_else(|| Error::EntryTypeMismatch)
        }
        pub fn set_dimm1_rank(&mut self, value: LrdimmDdr4DimmRanks) {
            let value = value.to_u32().unwrap();
            let x = self.dimm_rank_bitmap.get();
            self.dimm_rank_bitmap.set((x &! 0xF0) | (value << 4));
        }
    }

    impl Default for LrdimmDdr4OdtPatElement {
        fn default() -> Self {
            Self {
                dimm_rank_bitmap: (1 | (2 << 4)).into(),
                cs0_odt_pattern: 0.into(),
                cs1_odt_pattern: 0.into(),
                cs2_odt_pattern: 0.into(),
                cs3_odt_pattern: 0.into(),
            }
        }
    }

    impl EntryCompatible for LrdimmDdr4OdtPatElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4OdtPat) => true,
                // definitely not EntryId::Memory(MemoryEntryId::PsRdimmDdr4OdtPat) => true,
                _ => false,
            }
        }
    }

    // An union of one variant.
    impl UnionAsBytes for LrdimmDdr4OdtPatElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }

    bitfield! {
        #[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
        #[repr(C, packed)]
        pub struct DdrPostPackageRepairBody(U64<LittleEndian>);
        no default BitRange;
        impl Debug;
        u64;
        pub channel, set_channel: 55, 53;
        pub socket, set_socket: 52, 50;
        pub row, set_row: 49, 32;
        pub target_device, set_target_device: 31, 27;
        pub valid, set_valid: 26;
        pub hard_repair, set_hard_repair: 25;
        pub column, set_column: 24, 15;
        pub chip_select, set_chip_select: 14, 13;
        pub device_width, set_device_width: 12, 8; // device width of DIMMs to repair; or 0x1F for heeding target_device instead
        pub rank_multiplier, set_rank_multiplier: 7, 5;
        pub bank, set_bank: 4, 0;
    }

    impl BitRange<u64> for DdrPostPackageRepairBody {
        fn bit_range(&self, msb: usize, lsb: usize) -> u64 {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 63);
            let mask = (1u64 << width) - 1;
            (self.0.get() >> lsb) & mask
        }
        fn set_bit_range(&mut self, msb: usize, lsb: usize, value: u64) {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 63);
            let mask = (1u64 << width) - 1;
            assert!(u64::from(value) <= mask);
            let mut dest = self.0.get();
            dest &=! ((mask as u64) << lsb);
            dest |= (value as u64) << lsb;
            self.0.set(dest);
        }
    }

    impl Bit for DdrPostPackageRepairBody {
        fn bit(&self, index: usize) -> bool {
            match self.0.get() & (1u64 << index) {
                0 => false,
                _ => true,
            }
        }
        fn set_bit(&mut self, index: usize, value: bool) {
            match value {
                true => {
                    self.0.set(self.0.get() | (1u64 << index));
                },
                false => {
                    self.0.set(self.0.get() &! (1u64 << index));
                },
            }
        }
    }

    #[derive(FromBytes, AsBytes, Unaligned, Debug)]
    #[repr(C, packed)]
    pub struct DdrPostPackageRepairElement {
        body: DdrPostPackageRepairBody,
    }

    impl DdrPostPackageRepairElement {
        #[inline]
        pub fn body(&self) -> Option<&DdrPostPackageRepairBody> {
            if self.body.valid() {
                Some(&self.body)
            } else {
                None
            }
        }
        #[inline]
        pub fn invalid() -> DdrPostPackageRepairElement {
            Self {
                body: DdrPostPackageRepairBody(0xff00_0000_0000_0000.into()),
            }
        }
        #[inline]
        pub fn set_body(&mut self, value: Option<&DdrPostPackageRepairBody>) {
            match value {
                Some(body) => {
                    self.body = *body;
                },
                None => {
                    self.body = Self::invalid().body;
                },
            }
        }
    }

    impl EntryCompatible for DdrPostPackageRepairElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::DdrPostPackageRepair) => true,
                _ => false,
            }
        }
    }


    // An union of one variant.
    impl UnionAsBytes for DdrPostPackageRepairElement {
        #[inline]
        fn as_bytes(&self) -> &[u8] {
            AsBytes::as_bytes(self)
        }
    }

    pub mod platform_specific_override {
        // See AMD #44065

        use byteorder::LittleEndian;
        use core::mem::size_of;
        use static_assertions::const_assert;
        use zerocopy::{AsBytes, FromBytes, Unaligned, U16, U32};
        use super::super::*;
        use super::UnionAsBytes;
        use crate::struct_accessors::{Getter, Setter, make_accessors};
        use crate::types::Result;

        bitfield! {
            pub struct ChannelIds(u8);
            impl Debug;
            bool;
            #[inline]
            pub a, set_a: 0;
            #[inline]
            pub b, set_b: 1;
            #[inline]
            pub c, set_c: 2;
            #[inline]
            pub d, set_d: 3;
            #[inline]
            pub e, set_e: 4;
            #[inline]
            pub f, set_f: 5;
            #[inline]
            pub g, set_g: 6;
            #[inline]
            pub h, set_h: 7;
        }

        impl FromPrimitive for ChannelIds {
            #[inline]
            fn from_u64(raw_value: u64) -> Option<Self> {
                if raw_value <= 0xFF { // valid
                    Some(Self(raw_value as u8))
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

        impl ToPrimitive for ChannelIds {
            #[inline]
            fn to_i64(&self) -> Option<i64> {
                Some(self.0.into())
            }
            #[inline]
            fn to_u64(&self) -> Option<u64> {
                Some(self.0.into())
            }
        }

        impl ChannelIds {
            pub fn all() -> Self {
                Self(0xFF)
            }
        }

        bitfield! {
            pub struct SocketIds(u8);
            impl Debug;
            bool;
            #[inline]
            pub socket_0, set_socket_0: 0;
            #[inline]
            pub socket_1, set_socket_1: 1;
            #[inline]
            pub socket_2, set_socket_2: 2;
            #[inline]
            pub socket_3, set_socket_3: 3;
            #[inline]
            pub socket_4, set_socket_4: 4;
            #[inline]
            pub socket_5, set_socket_5: 5;
            #[inline]
            pub socket_6, set_socket_6: 6;
            #[inline]
            pub socket_7, set_socket_7: 7;
        }

        impl FromPrimitive for SocketIds {
            #[inline]
            fn from_u64(raw_value: u64) -> Option<Self> {
                if raw_value <= 0xFF { // valid
                    Some(Self(raw_value as u8))
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

        impl ToPrimitive for SocketIds {
            #[inline]
            fn to_i64(&self) -> Option<i64> {
                Some(self.0.into())
            }
            #[inline]
            fn to_u64(&self) -> Option<u64> {
                Some(self.0.into())
            }
        }

        impl SocketIds {
            pub fn all() -> Self {
                Self(0xFF)
            }
        }

        bitfield! {
            pub struct DimmSlots(u8);
            impl Debug;
            bool;
            #[inline]
            pub dimm_slot_0, set_dimm_slot_0: 0;
            #[inline]
            pub dimm_slot_1, set_dimm_slot_1: 1;
            #[inline]
            pub dimm_slot_2, set_dimm_slot_2: 2;
            #[inline]
            pub dimm_slot_3, set_dimm_slot_3: 3;
        }

        impl FromPrimitive for DimmSlots {
            #[inline]
            fn from_u64(raw_value: u64) -> Option<Self> {
                if raw_value == 0xFF { // valid
                    Some(Self(raw_value as u8))
                } else if raw_value < 16 { // valid
                    Some(Self(raw_value as u8))
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
                Some(self.0.into())
            }
            #[inline]
            fn to_u64(&self) -> Option<u64> {
                Some(self.0.into())
            }
        }

        impl DimmSlots {
            pub fn all() -> Self {
                Self(0xFF)
            }
        }

        macro_rules! impl_EntryCompatible {($struct_:ty, $type_:expr, $payload_size:expr) => (
            const_assert!($payload_size as usize + 2usize == size_of::<$struct_>());

            impl EntryCompatible for $struct_ {
                fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
                    match entry_id {
                        EntryId::Memory(MemoryEntryId::PlatformSpecificOverride) => {
                            prefix.len() >= 2 && prefix[0] == $type_ && prefix[1] as usize + 2usize == size_of::<Self>()
                        },
                        _ => false,
                    }
                }
            }
//            impl HeaderWithTail for $struct_ {
//                type TailSequenceType = ();
//            }
        )}

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct CkeTristateMap {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                /// index i = CPU package's clock enable (CKE) pin, value = memory rank's CKE pin mask
                pub connections: [u8; 4],
            }
        }
        impl_EntryCompatible!(CkeTristateMap, 1, 7);
        impl Default for CkeTristateMap {
            fn default() -> Self {
                Self {
                    type_: 1.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    connections: [0; 4], // probably invalid
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct OdtTristateMap {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                /// index i = CPU package's ODT pin (MA_ODT\[i\]), value = memory rank's ODT pin mask
                pub connections: [u8; 4],
            }
        }
        impl_EntryCompatible!(OdtTristateMap, 2, 7);
        impl Default for OdtTristateMap {
            fn default() -> Self {
                Self {
                    type_: 2.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    connections: [0; 4], // probably invalid
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct CsTristateMap {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                /// index i = CPU package CS pin (MA_CS_L\[i\]), value = memory rank's CS pin mask
                pub connections: [u8; 8],
            }
        }
        impl_EntryCompatible!(CsTristateMap, 3, 11);
        impl Default for CsTristateMap {
            fn default() -> Self {
                Self {
                    type_: 3.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    connections: [0; 8], // probably invalid
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MaxDimmsPerChannel {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                value: u8 : pub get Result<u8> : pub set u8,
            }
        }
        impl_EntryCompatible!(MaxDimmsPerChannel, 4, 4);
        impl Default for MaxDimmsPerChannel {
            fn default() -> Self {
                Self {
                    type_: 4.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    value: 2,
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MemclkMap {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: must always be "all"
                /// See BKDG.  Example: index i = M\[B,A\]_CLK_H/L\[i\]; value = mask of which CS are connected (CS0 = 1 << 0, CS1 = 1 << 1, CS2 = 1 << 2, CS3 = 1 << 3).
                pub connections: [u8; 8], // index: memory clock pin; value: mask for connected CS pins on DIMM
            }
        }
        impl_EntryCompatible!(MemclkMap, 7, 11);
        impl Default for MemclkMap {
            fn default() -> Self {
                Self {
                    type_: 7.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    connections: [0; 8], // all disabled
                }
            }
        }
        impl MemclkMap {
            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, connections: [u8; 8]) -> Self {
                Self {
                    sockets: sockets.to_u8().unwrap(),
                    channels: channels.to_u8().unwrap(),
                    dimms: dimms.to_u8().unwrap(),
                    connections,
                    ..Self::default()
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MaxChannelsPerSocket {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds>,  // Note: must always be "all"
                dimms: u8 : pub get Result<DimmSlots>, // Note: must always be "all" here
                value: u8 : pub get Result<u8> : pub set u8,
            }
        }
        impl_EntryCompatible!(MaxChannelsPerSocket, 8, 4);
        impl Default for MaxChannelsPerSocket {
            fn default() -> Self {
                Self {
                    type_: 8.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    value: 2,
                }
            }
        }
        impl MaxChannelsPerSocket {
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
        pub enum TimingMode {
            Auto = 0,
            Limit = 1,
            Specific = 2,
        }

        #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MemBusSpeed {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: must always be "all"
                timing_mode: U32<LittleEndian> : pub get Result<TimingMode> : pub set TimingMode,
                bus_speed: U32<LittleEndian> : pub get Result<MemBusSpeedType> : pub set MemBusSpeedType,
            }
        }
        impl_EntryCompatible!(MemBusSpeed, 9, 11);
        impl Default for MemBusSpeed {
            fn default() -> Self {
                Self {
                    type_: 9.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    timing_mode: TimingMode::Auto.to_u32().unwrap().into(),
                    bus_speed: MemBusSpeedType::Ddr1600.to_u32().unwrap().into(), // User probably wants to change this
                }
            }
        }
        impl MemBusSpeed {
            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, timing_mode: u32, bus_speed: u32) -> Self {
                Self {
                    sockets: sockets.to_u8().unwrap(),
                    channels: channels.to_u8().unwrap(),
                    dimms: dimms.to_u8().unwrap(),
                    timing_mode: timing_mode.into(),
                    bus_speed: bus_speed.into(),
                    ..Self::default()
                }
            }
        }

        make_accessors! {
            /// Max. Chip Selects per channel
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MaxCsPerChannel {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: must always be "all"
                value: u8,
            }
        }
        impl_EntryCompatible!(MaxCsPerChannel, 10, 4);
        impl Default for MaxCsPerChannel {
            fn default() -> Self {
                Self {
                    type_: 10.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    value: 0, // probably invalid
                }
            }
        }
        impl MaxCsPerChannel {
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MemTechnology {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds>, // Note: must always be "all" here
                dimms: u8 : pub get Result<DimmSlots>, // Note: must always be "all" here
                technology_type: U32<LittleEndian> : pub get Result<MemTechnologyType> : pub set MemTechnologyType,
            }
        }
        impl_EntryCompatible!(MemTechnology, 11, 7);
        impl Default for MemTechnology {
            fn default() -> Self {
                Self {
                    type_: 11.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    technology_type: 0.into(), // probably invalid
                }
            }
        }
        impl MemTechnology {
            pub fn new(sockets: SocketIds, channels: ChannelIds, dimms: DimmSlots, technology_type: u32) -> Self {
                Self {
                    sockets: sockets.to_u8().unwrap(),
                    channels: channels.to_u8().unwrap(),
                    dimms: dimms.to_u8().unwrap(),
                    technology_type: technology_type.into(),
                    ..Self::default()
                }
            }
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct WriteLevellingSeedDelay {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                seed: [u8; 8],
                ecc_seed: u8,
            }
        }
        impl_EntryCompatible!(WriteLevellingSeedDelay, 12, 12);
        impl Default for WriteLevellingSeedDelay {
            fn default() -> Self {
                Self {
                    type_: 12.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
                    seed: [0.into(); 8], // probably invalid
                    ecc_seed: 0.into(), // probably invalid
                }
            }
        }

        make_accessors! {
            /// See <https://www.amd.com/system/files/TechDocs/43170_14h_Mod_00h-0Fh_BKDG.pdf> section 2.9.3.7.2.1
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct RxEnSeed {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                seed: [U16<LittleEndian>; 8],
                ecc_seed: U16<LittleEndian>,
            }
        }
        impl_EntryCompatible!(RxEnSeed, 13, 21);
        impl Default for RxEnSeed {
            fn default() -> Self {
                Self {
                    type_: 13.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq)]
            #[repr(C, packed)]
            pub struct LrDimmNoCs6Cs7Routing {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots> : pub set DimmSlots,
                value: u8, // Note: always 1
            }
        }
        impl_EntryCompatible!(LrDimmNoCs6Cs7Routing, 14, 4);
        impl Default for LrDimmNoCs6Cs7Routing {
            fn default() -> Self {
                Self {
                    type_: 14.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct SolderedDownSodimm {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                value: u8, // Note: always 1
            }
        }
        impl_EntryCompatible!(SolderedDownSodimm, 15, 4);
        impl Default for SolderedDownSodimm {
            fn default() -> Self {
                Self {
                    type_: 15.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct LvDimmForce1V5 {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds>, // Note: always "all"
                channels: u8 : pub get Result<ChannelIds>, // Note: always "all"
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                value: u8, // Note: always 1
            }
        }
        impl_EntryCompatible!(LvDimmForce1V5, 16, 4);
        impl Default for LvDimmForce1V5 {
            fn default() -> Self {
                Self {
                    type_: 16.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MinimumRwDataEyeWidth {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                min_read_data_eye_width: u8 : pub get Result<u8> : pub set u8,
                min_write_data_eye_width: u8 : pub get Result<u8> : pub set u8,
            }
        }
        impl_EntryCompatible!(MinimumRwDataEyeWidth, 17, 5);
        impl Default for MinimumRwDataEyeWidth {
            fn default() -> Self {
                Self {
                    type_: 17.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct CpuFamilyFilter {
                type_: u8,
                payload_size: u8,
                cpu_family_revision: U32<LittleEndian> : pub get Result<u32> : pub set u32,
            }
        }
        impl_EntryCompatible!(CpuFamilyFilter, 18, 4);
        impl Default for CpuFamilyFilter {
            fn default() -> Self {
                Self {
                    type_: 18.into(),
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
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct SolderedDownDimmsPerChannel {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds> : pub set SocketIds,
                channels: u8 : pub get Result<ChannelIds> : pub set ChannelIds,
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                value: u8 : pub get Result<u8> : pub set u8,
            }
        }
        impl_EntryCompatible!(SolderedDownDimmsPerChannel, 19, 4);
        impl Default for SolderedDownDimmsPerChannel {
            fn default() -> Self {
                Self {
                    type_: 19.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
        pub enum MemPowerPolicyType {
            Performance = 0,
            BatteryLife = 1,
            Auto = 2,
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MemPowerPolicy {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds>, // Note: always "all"
                channels: u8 : pub get Result<ChannelIds>, // Note: always "all"
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                value: u8 : pub get Result<MemPowerPolicyType> : pub set MemPowerPolicyType,
            }
        }
        impl_EntryCompatible!(MemPowerPolicy, 20, 4);
        impl Default for MemPowerPolicy {
            fn default() -> Self {
                Self {
                    type_: 20.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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
        pub enum MotherboardLayerCount {
            L4 = 0,
            L6 = 1,
        }

        make_accessors! {
            #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
            #[repr(C, packed)]
            pub struct MotherboardLayers {
                type_: u8,
                payload_size: u8,
                sockets: u8 : pub get Result<SocketIds>, // Note: always "all"
                channels: u8 : pub get Result<ChannelIds>, // Note: always "all"
                dimms: u8 : pub get Result<DimmSlots>, // Note: always "all"
                value: u8 : pub get Result<MotherboardLayerCount> : pub set MotherboardLayerCount,
            }
        }
        impl_EntryCompatible!(MotherboardLayers, 21, 4);
        impl Default for MotherboardLayers {
            fn default() -> Self {
                Self {
                    type_: 21.into(),
                    payload_size: (size_of::<Self>() - 2) as u8,
                    sockets: SocketIds::all().to_u8().unwrap(),
                    channels: ChannelIds::all().to_u8().unwrap(),
                    dimms: DimmSlots::all().to_u8().unwrap(),
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

        // Maintainer: Add struct references in here as you add new structs above.  Also adapt the is_entry_compatible impl.
        pub enum PlatformSpecificOverrideElementRef<'a> {
            CkeTristateMap(&'a CkeTristateMap),
            OdtTristateMap(&'a OdtTristateMap),
            CsTristateMap(&'a CsTristateMap),
            MaxDimmsPerChannel(&'a MaxDimmsPerChannel),
            MemclkMap(&'a MemclkMap),
            MaxChannelsPerSocket(&'a MaxChannelsPerSocket),
            MemBusSpeed(&'a MemBusSpeed),
            MaxCsPerChannel(&'a MaxCsPerChannel),
            MemTechnology(&'a MemTechnology),
            WriteLevellingSeedDelay(&'a WriteLevellingSeedDelay),
            RxEnSeed(&'a RxEnSeed),
            LrDimmNoCs6Cs7Routing(&'a LrDimmNoCs6Cs7Routing),
            SolderedDownSodimm(&'a SolderedDownSodimm),
            LvDimmForce1V5(&'a LvDimmForce1V5),
            MemPowerPolicy(&'a MemPowerPolicy),
            MinimumRwDataEyeWidth(&'a MinimumRwDataEyeWidth),
            MotherboardLayers(&'a MotherboardLayers),
            CpuFamilyFilter(&'a CpuFamilyFilter),
            SolderedDownDimmsPerChannel(&'a SolderedDownDimmsPerChannel),
        }
        impl UnionAsBytes for PlatformSpecificOverrideElementRef<'_> {
            #[inline]
            fn as_bytes(&self) -> &[u8] {
                match self {
                    Self::CkeTristateMap(item) => {
                        item.as_bytes()
                    },
                    Self::OdtTristateMap(item) => {
                        item.as_bytes()
                    },
                    Self::CsTristateMap(item) => {
                        item.as_bytes()
                    },
                    Self::MaxDimmsPerChannel(item) => {
                        item.as_bytes()
                    },
                    Self::MemclkMap(item) => {
                        item.as_bytes()
                    },
                    Self::MaxChannelsPerSocket(item) => {
                        item.as_bytes()
                    },
                    Self::MemBusSpeed(item) => {
                        item.as_bytes()
                    },
                    Self::MaxCsPerChannel(item) => {
                        item.as_bytes()
                    },
                    Self::MemTechnology(item) => {
                        item.as_bytes()
                    },
                    Self::WriteLevellingSeedDelay(item) => {
                        item.as_bytes()
                    },
                    Self::RxEnSeed(item) => {
                        item.as_bytes()
                    },
                    Self::LrDimmNoCs6Cs7Routing(item) => {
                        item.as_bytes()
                    },
                    Self::SolderedDownSodimm(item) => {
                        item.as_bytes()
                    },
                    Self::LvDimmForce1V5(item) => {
                        item.as_bytes()
                    },
                    Self::MemPowerPolicy(item) => {
                        item.as_bytes()
                    },
                    Self::MinimumRwDataEyeWidth(item) => {
                        item.as_bytes()
                    },
                    Self::MotherboardLayers(item) => {
                        item.as_bytes()
                    },
                    Self::CpuFamilyFilter(item) => {
                        item.as_bytes()
                    },
                    Self::SolderedDownDimmsPerChannel(item) => {
                        item.as_bytes()
                    },
                }
            }
        }
        impl EntryCompatible for PlatformSpecificOverrideElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
                CkeTristateMap::is_entry_compatible(entry_id, prefix)
                || OdtTristateMap::is_entry_compatible(entry_id, prefix)
                || CsTristateMap::is_entry_compatible(entry_id, prefix)
                || MaxDimmsPerChannel::is_entry_compatible(entry_id, prefix)
                || MemclkMap::is_entry_compatible(entry_id, prefix)
                || MaxChannelsPerSocket::is_entry_compatible(entry_id, prefix)
                || MemBusSpeed::is_entry_compatible(entry_id, prefix)
                || MaxCsPerChannel::is_entry_compatible(entry_id, prefix)
                || MemTechnology::is_entry_compatible(entry_id, prefix)
                || WriteLevellingSeedDelay::is_entry_compatible(entry_id, prefix)
                || RxEnSeed::is_entry_compatible(entry_id, prefix)
                || LrDimmNoCs6Cs7Routing::is_entry_compatible(entry_id, prefix)
                || SolderedDownSodimm::is_entry_compatible(entry_id, prefix)
                || LvDimmForce1V5::is_entry_compatible(entry_id, prefix)
                || MemPowerPolicy::is_entry_compatible(entry_id, prefix)
                || MinimumRwDataEyeWidth::is_entry_compatible(entry_id, prefix)
                || MotherboardLayers::is_entry_compatible(entry_id, prefix)
                || CpuFamilyFilter::is_entry_compatible(entry_id, prefix)
                || SolderedDownDimmsPerChannel::is_entry_compatible(entry_id, prefix)
            }
        }

        impl<'a> UnionFromBytes<'a> for PlatformSpecificOverrideElementRef<'a> {
            fn skip_step(prefix: &[u8]) -> Option<usize> {
                if prefix.len() >= 2 {
                    (prefix[1] as usize).checked_add(2)
                } else {
                    None
                }
            }
            #[inline]
            fn from_bytes(world: &'a [u8]) -> Option<Self> {
                if world.len() >= 2 {
                    let mut buf = world.clone();
                    if size_of::<CkeTristateMap>() == Self::skip_step(world)? && world[0] == CkeTristateMap::default().type_ {
                        Some(Self::CkeTristateMap(take_header_from_collection::<CkeTristateMap>(&mut buf)?))
                    } else if size_of::<OdtTristateMap>() == Self::skip_step(world)? && world[0] == OdtTristateMap::default().type_ {
                        Some(Self::OdtTristateMap(take_header_from_collection::<OdtTristateMap>(&mut buf)?))
                    } else if size_of::<CsTristateMap>() == Self::skip_step(world)? && world[0] == CsTristateMap::default().type_ {
                        Some(Self::CsTristateMap(take_header_from_collection::<CsTristateMap>(&mut buf)?))
                    } else if size_of::<MaxDimmsPerChannel>() == Self::skip_step(world)? && world[0] == MaxDimmsPerChannel::default().type_ {
                        Some(Self::MaxDimmsPerChannel(take_header_from_collection::<MaxDimmsPerChannel>(&mut buf)?))
                    } else if size_of::<MemclkMap>() == Self::skip_step(world)? && world[0] == MemclkMap::default().type_ {
                        Some(Self::MemclkMap(take_header_from_collection::<MemclkMap>(&mut buf)?))
                    } else if size_of::<MaxChannelsPerSocket>() == Self::skip_step(world)? && world[0] == MaxChannelsPerSocket::default().type_ {
                        Some(Self::MaxChannelsPerSocket(take_header_from_collection::<MaxChannelsPerSocket>(&mut buf)?))
                    } else if size_of::<MemBusSpeed>() == Self::skip_step(world)? && world[0] == MemBusSpeed::default().type_ {
                        Some(Self::MemBusSpeed(take_header_from_collection::<MemBusSpeed>(&mut buf)?))
                    } else if size_of::<MaxCsPerChannel>() == Self::skip_step(world)? && world[0] == MaxCsPerChannel::default().type_ {
                        Some(Self::MaxCsPerChannel(take_header_from_collection::<MaxCsPerChannel>(&mut buf)?))
                    } else if size_of::<MemTechnology>() == Self::skip_step(world)? && world[0] == MemTechnology::default().type_ {
                        Some(Self::MemTechnology(take_header_from_collection::<MemTechnology>(&mut buf)?))
                    } else if size_of::<WriteLevellingSeedDelay>() == Self::skip_step(world)? && world[0] == WriteLevellingSeedDelay::default().type_ {
                        Some(Self::WriteLevellingSeedDelay(take_header_from_collection::<WriteLevellingSeedDelay>(&mut buf)?))
                    } else if size_of::<RxEnSeed>() == Self::skip_step(world)? && world[0] == RxEnSeed::default().type_ {
                        Some(Self::RxEnSeed(take_header_from_collection::<RxEnSeed>(&mut buf)?))
                    } else if size_of::<LrDimmNoCs6Cs7Routing>() == Self::skip_step(world)? && world[0] == LrDimmNoCs6Cs7Routing::default().type_ {
                        Some(Self::LrDimmNoCs6Cs7Routing(take_header_from_collection::<LrDimmNoCs6Cs7Routing>(&mut buf)?))
                    } else if size_of::<SolderedDownSodimm>() == Self::skip_step(world)? && world[0] == SolderedDownSodimm::default().type_ {
                        Some(Self::SolderedDownSodimm(take_header_from_collection::<SolderedDownSodimm>(&mut buf)?))
                    } else if size_of::<LvDimmForce1V5>() == Self::skip_step(world)? && world[0] == LvDimmForce1V5::default().type_ {
                        Some(Self::LvDimmForce1V5(take_header_from_collection::<LvDimmForce1V5>(&mut buf)?))
                    } else if size_of::<MemPowerPolicy>() == Self::skip_step(world)? && world[0] == MemPowerPolicy::default().type_ {
                        Some(Self::MemPowerPolicy(take_header_from_collection::<MemPowerPolicy>(&mut buf)?))
                    } else if size_of::<MinimumRwDataEyeWidth>() == Self::skip_step(world)? && world[0] == MinimumRwDataEyeWidth::default().type_ {
                        Some(Self::MinimumRwDataEyeWidth(take_header_from_collection::<MinimumRwDataEyeWidth>(&mut buf)?))
                    } else if size_of::<MotherboardLayers>() == Self::skip_step(world)? && world[0] == MotherboardLayers::default().type_ {
                        Some(Self::MotherboardLayers(take_header_from_collection::<MotherboardLayers>(&mut buf)?))
                    } else if size_of::<CpuFamilyFilter>() == Self::skip_step(world)? && world[0] == CpuFamilyFilter::default().type_ {
                        Some(Self::CpuFamilyFilter(take_header_from_collection::<CpuFamilyFilter>(&mut buf)?))
                    } else if size_of::<SolderedDownDimmsPerChannel>() == Self::skip_step(world)? && world[0] == SolderedDownDimmsPerChannel::default().type_ {
                        Some(Self::SolderedDownDimmsPerChannel(take_header_from_collection::<SolderedDownDimmsPerChannel>(&mut buf)?))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
        // TODO: conditional overrides, actions.
    }

    pub mod platform_tuning {
        use byteorder::LittleEndian;
        use core::mem::size_of;
        use static_assertions::const_assert;
        use zerocopy::{AsBytes, FromBytes, Unaligned, U16};
        use super::super::*;
        use super::UnionAsBytes;
        //use crate::struct_accessors::{Getter, Setter, make_accessors};
        //use crate::types::Result;

        macro_rules! impl_EntryCompatible {($struct_:ty, $type_:expr, $total_size:expr) => (
            const_assert!($total_size as usize == size_of::<$struct_>());
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
            }
//            impl HeaderWithTail for $struct_ {
//                type TailSequenceType = ();
//            }
        )}

        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct Terminator {
            type_: U16<LittleEndian>,
        }
        impl_EntryCompatible!(Terminator, 0xfeef, 2);
        impl Default for Terminator {
            fn default() -> Self {
                Self {
                    type_: 0xfeef.into(),
                }
            }
        }

        // Maintainer: Add struct references in here as you add new structs above.  Also adapt the is_entry_compatible impl.
        pub enum PlatformTuningElementRef<'a> {
            Terminator(&'a Terminator),
        }
        impl UnionAsBytes for PlatformTuningElementRef<'_> {
            #[inline]
            fn as_bytes(&self) -> &[u8] {
                match self {
                    Self::Terminator(item) => {
                        item.as_bytes()
                    },
                }
            }
        }
        impl EntryCompatible for PlatformTuningElementRef<'_> {
            fn is_entry_compatible(entry_id: EntryId, prefix: &[u8]) -> bool {
                Terminator::is_entry_compatible(entry_id, prefix)
            }
        }
//        impl HeaderWithTail for PlatformTuningElementRef<'_> {
//            type TailSequenceType = ();
//        }
        impl<'a> UnionFromBytes<'a> for PlatformTuningElementRef<'a> {
            fn skip_step(prefix: &[u8]) -> Option<usize> {
                if prefix.len() >= 2 {
                    let type_lo = prefix[0];
                    let type_hi = prefix[1];
                    if type_lo == 0xef && type_hi == 0xfe { // no len available
                        Some(2)
                    } else if prefix.len() >= 3 {
                        (prefix[2] as usize).checked_add(2)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            #[inline]
            fn from_bytes(world: &'a [u8]) -> Option<Self> {
                if world.len() >= 2 {
                    if size_of::<Terminator>() == Self::skip_step(world)? && world[0] as u16 == Terminator::default().type_.get() & 0xFF && world[1] as u16 == Terminator::default().type_.get() >> 8 {
                        let mut buf = world.clone();
                        Some(Self::Terminator(take_header_from_collection::<Terminator>(&mut buf)?))
                    } else {
                        None
                    }
                } else {
                    None
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
            let dimm_info = DimmInfoSmbusElement::new_slot(1, 2, 3, 4, Some(5), Some(6), Some(7)).unwrap();
            assert!(dimm_info.dimm_slot_present().unwrap());
            assert!(dimm_info.socket_id() == 1);
            assert!(dimm_info.channel_id() == 2);
            assert!(dimm_info.dimm_id() == 3);
            assert!(dimm_info.dimm_smbus_address() == Some(4));
            assert!(dimm_info.i2c_mux_address() == Some(5));
            assert!(dimm_info.mux_control_address() == Some(6));
            assert!(dimm_info.mux_channel() == Some(7));
            let dimm_info = DimmInfoSmbusElement::new_slot(1, 2, 3, 4, Some(5), Some(6), Some(7)).unwrap();
            assert!(dimm_info.dimm_slot_present().unwrap());
            let dimm_info_2 = DimmInfoSmbusElement::new_soldered_down(1, 2, 3, 4, Some(5), Some(6), Some(7)).unwrap();
            assert!(!dimm_info_2.dimm_slot_present().unwrap());

            let mut abl_console_out_control = AblConsoleOutControl::default();
            abl_console_out_control.set_enable_mem_setreg_logging(true);
            assert!(abl_console_out_control.enable_mem_setreg_logging().unwrap());
            abl_console_out_control.set_enable_mem_setreg_logging(false);
            assert!(!abl_console_out_control.enable_mem_setreg_logging().unwrap());
            let console_out_control = ConsoleOutControl::new(abl_console_out_control, AblBreakpointControl::default());
            assert!(console_out_control.abl_console_out_control.abl_console_port() == 0x80);
        }
    }
}

pub mod psp {
    use super::*;
    use super::memory::Gpio;
    use crate::struct_accessors::{Getter, Setter, make_accessors};

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct IdApcbMapping {
            id_and_feature_mask: u8 : pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get u8 : pub set u8,
            board_instance_index: u8 : pub get u8 : pub set u8,
        }
    }
    impl IdApcbMapping {
        pub fn new(id_and_feature_mask: u8, id_and_feature_value: u8, board_instance_index: u8) -> Self {
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct IdRevApcbMapping {
            id_and_rev_and_feature_mask: u8 : pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get u8 : pub set u8,
            rev_and_feature_value: u8 : pub get u8 : pub set u8, // FIXME: or 0xff=NA
            board_instance_index: u8 : pub get u8 : pub set u8,
        }
    }

    impl IdRevApcbMapping {
        pub fn new(id_and_rev_and_feature_mask: u8, id_and_feature_value: u8, rev_and_feature_value: u8, board_instance_index: u8) -> Self {
            Self {
                id_and_rev_and_feature_mask,
                id_and_feature_value,
                rev_and_feature_value,
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodCustom {
            access_method: U16<LittleEndian>, // 0xF for BoardIdGettingMethodCustom
            feature_mask: U16<LittleEndian> : pub get u16 : pub set u16,
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
                match entry_id {
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod) => true,
                    _ => false,
                }
            } else {
                false
            }
        }
    }
    impl HeaderWithTail for BoardIdGettingMethodCustom {
        type TailSequenceType = IdApcbMapping;
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodGpio {
            access_method: U16<LittleEndian>, // 3 for BoardIdGettingMethodGpio
            pub bit_locations: [Gpio; 4], // for the board id FIXME Make accessible
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
                match entry_id {
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod) => true,
                    _ => false,
                }
            } else {
                false
            }
        }
    }
    impl HeaderWithTail for BoardIdGettingMethodGpio {
        type TailSequenceType = IdApcbMapping;
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodEeprom {
            access_method: U16<LittleEndian>, // 2 for BoardIdGettingMethodEeprom
            i2c_controller_index: U16<LittleEndian> : pub get u16 : pub set u16,
            device_address: U16<LittleEndian> : pub get u16 : pub set u16,
            board_id_offset: U16<LittleEndian> : pub get u16 : pub set u16, // Byte offset
            board_rev_offset: U16<LittleEndian> : pub get u16 : pub set u16, // Byte offset
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
        pub fn new(i2c_controller_index: u16, device_address: u16, board_id_offset: u16, board_rev_offset: u16) -> Self {
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
                match entry_id {
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod) => true,
                    _ => false,
                }
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for BoardIdGettingMethodEeprom {
        type TailSequenceType = IdRevApcbMapping;
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct BoardIdGettingMethodSmbus {
            access_method: U16<LittleEndian>, // 1 for BoardIdGettingMethodSmbus
            i2c_controller_index: U16<LittleEndian> : pub get u16 : pub set u16,
            i2c_mux_address: u8 : pub get u8 : pub set u8,
            mux_control_address: u8 : pub get u8 : pub set u8,
            mux_channel: u8 : pub get u8 : pub set u8,
            smbus_address: U16<LittleEndian> : pub get u16 : pub set u16,
            register_index: U16<LittleEndian> : pub get u16 : pub set u16,
        }
    }

    impl Default for BoardIdGettingMethodSmbus {
        fn default() -> Self {
            Self {
                access_method: 1.into(),
                i2c_controller_index: 0.into(), // maybe invalid
                i2c_mux_address: 0, // maybe invalid
                mux_control_address: 0, // maybe invalid
                mux_channel: 0, // maybe invalid
                smbus_address: 0.into(), // maybe invalid
                register_index: 0.into(),
            }
        }
    }

    impl BoardIdGettingMethodSmbus {
        pub fn new(i2c_controller_index: u16, i2c_mux_address: u8, mux_control_address: u8, mux_channel: u8, smbus_address: u16, register_index: u16) -> Self {
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
                match entry_id {
                    EntryId::Psp(PspEntryId::BoardIdGettingMethod) => true,
                    _ => false,
                }
            } else {
                false
            }
        }
    }

    impl HeaderWithTail for BoardIdGettingMethodSmbus {
        type TailSequenceType = IdApcbMapping;
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

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum BaudRate {
    B2400 = 0,
    B3600 = 1,
    B4800 = 2,
    B7200 = 3,
    B9600 = 4,
    B19200 = 5,
    B38400 = 6,
    B57600 = 7,
    B115200 = 8,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemActionOnBistFailure {
    DoNothing = 0,
    DisableProblematicCcds = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemRcwWeakDriveDisable {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemSelfRefreshExitStaggering {
    Disabled = 0,
    OneThird = 3, // Trfc/3
    OneFourth = 4, // Trfc/4
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum CbsMemAddrCmdParityRetryDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum CcxSevAsidCount {
    Count253 = 0,
    Count509 = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum FchConsoleOutSuperIoType {
    Auto = 0,
    Type1 = 1,
    Type2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfToggle {
    Disabled = 0,
    Enabled = 1,
    Auto = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemHealPprType {
    SoftRepair = 0,
    HardRepair = 1,
    NoRepair = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemHealTestSelect {
    Default = 0,
    NoVendorTests = 1,
    ForceAllVendorTests = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfExtIpSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfSyncFloodPropagation {
    Allow = 0,
    Disable = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfMemInterleaving {
    None = 0,
    Channel = 1,
    Die = 2,
    Socket = 3,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfMemInterleavingSize {
    Size256Byte = 0,
    Size512Byte = 1,
    Size1024Byte = 2,
    Size2048Byte = 3,
    Size4096Byte = 4,
    Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfDramNumaPerSocket {
   None = 0,
   One = 1,
   Two = 2,
   Four = 3,
   Auto = 7,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfRemapAt1TiB {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum SecondPcieLinkSpeed {
    Keep = 0,
    Gen1 = 1,
    Gen2 = 2,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum SecondPcieLinkMaxPayload {
    P128Byte = 0,
    P256Byte = 1,
    P512Byte = 2,
    P1024Byte = 3,
    P2048Byte = 4,
    P4096Byte = 5,
    HardwareDefault = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
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
pub enum MemControllerPmuTrainFfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemControllerPmuTrainDfeDdr4 {
    Disabled = 0,
    Enabled = 1,
    Auto = 0xff,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemMbistDataEyeType {
    Type1dVolate = 0,
    Type1dTiming = 1,
    Type2dFullDataEye = 2,
    TypeWorstCaseMarginOnly = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfInvertDramMap {
    Normal = 0,
    Inverted = 1,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum DfXgmiTxEqMode {
    Disabled = 0,
    EnabledByLane = 1,
    EnabledByLink = 2,
    EnabledByLinkAndRxVetting = 3,
    Auto = 0xff,
}

// This trait exists so we can impl it for bool; the macro MAKE_TOKEN_ACCESSORS will call the function by name without specifying the trait anyway.
trait ToPrimitive1 {
    fn to_u32(&self) -> Option<u32>;
}
// This trait exists so we can impl it for bool; the macro MAKE_TOKEN_ACCESSORS will call the function by name without specifying the trait anyway.
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
        if value >= 0 && value < 0x1_0000_0000 {
            let value: u64 = value.try_into().unwrap();
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
            },
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
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
        if value >= 0 && value < 0x1_0000_0000 {
            let value: u64 = value.try_into().unwrap();
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
            },
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
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
        if value >= 0 && value < 0x1_0000_0000 {
            let value: u64 = value.try_into().unwrap();
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
            },
            Self::Skip => Some(0xffff_ffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
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
            Self::Value(x) => {
                Some((*x).into())
            },
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemClockValue { // in MHz
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
        if value >= 0 && value < 0x1_0000 {
            let value: u64 = value.try_into().unwrap();
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
            },
            Self::Auto => Some(0xffff),
        }
    }
    fn to_u64(&self) -> Option<u64> {
        Some(self.to_i64()? as u64)
    }
}

type MemUserTimingMode = memory::platform_specific_override::TimingMode;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum MemHealBistEnable {
    Disabled = 0,
    TestAndRepairAllMemory = 1,
    JedecSelfHealing = 2,
    TestAndRepairAllMemoryAndJedecSelfHealing = 3,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
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
pub enum FchSmbusSpeed {
    Value(u8),  // x in 66 MHz / (4 x)
    // Auto
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
pub enum MemTrainingHdtControl {
    DetailedDebugMessages = 5,
    CoarseDebugMessages = 10,
    StageCompletionMessages = 200,
    // TODO: Seen in the wild: 201
    FirmwareCompletionMessagesOnly = 0xfe,
}

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
pub enum PspEnableDebugMode {
    Disabled = 0,
    Enabled = 1,
}

make_token_accessors! {
    // ABL

    abl_serial_baud_rate(TokenEntryId::Byte, default 8, id 0xae46_cea4) : pub get BaudRate : pub set BaudRate,

    // PSP

    psp_tp_port(TokenEntryId::Bool, default 1, id 0x0460_abe8) : pub get bool : pub set bool,
    psp_error_display(TokenEntryId::Bool, default 1, id 0xdc33_ff21) : pub get bool : pub set bool,
    psp_event_log_display(TokenEntryId::Bool, default 0, id 0x0c47_3e1c) : pub get bool : pub set bool, // TODO: Before using default, fix default.  It's possibly not correct.
    psp_stop_on_error(TokenEntryId::Bool, default 0, id 0xe702_4a21) : pub get bool : pub set bool,
    psp_psb_auto_fuse(TokenEntryId::Bool, default 1, id 0x2fcd_70c9) : pub get bool : pub set bool,
    psp_enable_debug_mode(TokenEntryId::Byte, default 0, id 0xd109_1cd0) : pub get PspEnableDebugMode : pub set PspEnableDebugMode,
    psp_syshub_watchdog_timer_interval(TokenEntryId::Word, default 2600, id 0xedb5_e4c9) : pub get u16 : pub set u16, // in ms

    // Memory Controller

    mem_enable_chip_select_interleaving(TokenEntryId::Bool, default 1, id 0x6f81_a115) : pub get bool : pub set bool,
    mem_enable_ecc_feature(TokenEntryId::Bool, default 1, id 0xfa35_f040) : pub get bool : pub set bool,
    mem_enable_parity(TokenEntryId::Bool, default 1, id 0x3cb8_cbd2) : pub get bool : pub set bool,
    mem_bus_frequency_limit(TokenEntryId::DWord, default 1600, id 0x3497_0a3c) : pub get MemBusFrequencyLimit : pub set MemBusFrequencyLimit,
    mem_clock_value(TokenEntryId::DWord, default 0xffff_ffff, id 0xcc83_f65f) : pub get MemClockValue : pub set MemClockValue,
    cbs_mem_speed_ddr4(TokenEntryId::Byte, default 0xff, id 0xe060_4ce9) : pub get CbsMemSpeedDdr4 : pub set CbsMemSpeedDdr4,
    mem_enable_bank_swizzle(TokenEntryId::Bool, default 1, id 0x6414_d160) : pub get bool : pub set bool,
    mem_action_on_bist_failure(TokenEntryId::Byte, default 0, id 0xcbc2_c0dd) : pub get MemActionOnBistFailure : pub set MemActionOnBistFailure,
    mem_rcw_weak_drive_disable(TokenEntryId::Byte, default 1, id 0xa30d_781a) : pub get MemRcwWeakDriveDisable : pub set MemRcwWeakDriveDisable, // FIXME is it u32 ?

    mem_user_timing_mode(TokenEntryId::DWord, default 0xff, id 0xfc56_0d7d) : pub get MemUserTimingMode : pub set MemUserTimingMode,
    mem_ignore_spd_checksum(TokenEntryId::Bool, default 0, id 0x7d36_9dbc) : pub get bool : pub set bool,
    mem_spd_read_optimization_ddr4(TokenEntryId::Bool, default 1, id 0x6816_f949) : pub get bool : pub set bool,
    cbs_mem_spd_read_optimization_ddr4(TokenEntryId::Bool, default 0, id 0x8d3a_b10e) : pub get bool : pub set bool, // TODO: Before using default, fix default.  It's possibly not correct.
    mem_enable_power_down(TokenEntryId::Bool, default 1, id 0xbbb1_85a2) : pub get bool : pub set bool,
    mem_self_refresh_exit_staggering(TokenEntryId::Byte, default 0, id 0xbc52_e5f7) : pub get MemSelfRefreshExitStaggering : pub set MemSelfRefreshExitStaggering,
    cbs_mem_power_down_delay(TokenEntryId::Word, default 0xff, id 0x1ebe_755a) : pub get CbsMemPowerDownDelay : pub set CbsMemPowerDownDelay,
    mem_temp_controlled_refresh_enable(TokenEntryId::Bool, default 0, id 0xf051_e1c4) : pub get bool : pub set bool,
    mem_odts_cmd_throttle_enable(TokenEntryId::Bool, default 1, id 0xc073_6395) : pub get bool : pub set bool,
    mem_sw_cmd_throttle_enable(TokenEntryId::Bool, default 0, id 0xa29c_1cf9) : pub get bool : pub set bool,
    mem_force_power_down_throttle_enable(TokenEntryId::Bool, default 1, id 0x1084_9d6c) : pub get bool : pub set bool,
    mem_hole_remapping(TokenEntryId::Bool, default 1, id 0x6a13_3ac5) : pub get bool : pub set bool,
    mem_enable_bank_group_swap_alt(TokenEntryId::Bool, default 1, id 0xa89d_1be8) : pub get bool : pub set bool,
    mem_enable_bank_group_swap(TokenEntryId::Bool, default 1, id 0x4692_0968) : pub get bool : pub set bool,
    mem_ddr_route_balanced_tee(TokenEntryId::Bool, default 0, id 0xe68c_363d) : pub get bool : pub set bool,
    cbs_mem_addr_cmd_parity_retry_ddr4(TokenEntryId::Byte, default 0, id 0xbe8b_ebce) : pub get CbsMemAddrCmdParityRetryDdr4 : pub set CbsMemAddrCmdParityRetryDdr4, // FIXME: Is it u32 ?
    cbs_mem_addr_cmd_parity_error_max_replay_ddr4(TokenEntryId::Byte, default 8, id 0x04e6_a482) : pub get u8 : pub set u8, // 0...0x3f
    cbs_mem_write_crc_retry_ddr4(TokenEntryId::Bool, default 0, id 0x25fb_6ea6) : pub get bool : pub set bool,
    cbs_mem_write_crc_error_max_replay_ddr4(TokenEntryId::Byte, default 8, id 0x74a0_8bec) : pub get u8 : pub set u8,
    cbs_mem_controller_write_crc_enable_ddr4(TokenEntryId::Bool, default 0, id 0x9445_1a4b) : pub get bool : pub set bool,

    mem_rcd_parity(TokenEntryId::Bool, default 1, id 0x647d_7662) : pub get bool : pub set bool,
    mem_uncorrected_ecc_retry_ddr4(TokenEntryId::Bool, default 1, id 0xbff0_0125) : pub get bool : pub set bool,
    mem_urg_ref_limit(TokenEntryId::Byte, default 6, id 0x1333_32df) : pub get u8 : pub set u8, // UMC::CH::SpazCtrl::UrgRefLimit; value: 1...6 (as in register mentioned first)
    mem_sub_urg_ref_lower_bound(TokenEntryId::Byte, default 4, id 0xe756_2ab6) : pub get u8 : pub set u8, // UMC::CH::SpazCtrl::SubUrgRefLowerBound; value: 1...6 (as in register mentioned first)
    mem_controller_pmu_train_ffe_ddr4(TokenEntryId::Byte, default 0xff, id 0x0d46_186d) : pub get MemControllerPmuTrainFfeDdr4 : pub set MemControllerPmuTrainFfeDdr4, // FIXME: is it bool ?
    mem_controller_pmu_train_dfe_ddr4(TokenEntryId::Byte, default 0xff, id 0x36a4_bb5b) : pub get MemControllerPmuTrainDfeDdr4 : pub set MemControllerPmuTrainDfeDdr4, // FIXME: is it bool ?
    mem_tsme_enable(TokenEntryId::Bool, default 1, id 0xd1fa_6660) : pub get bool : pub set bool, // See Transparent Secure Memory Encryption in PPR
    mem_training_hdt_control(TokenEntryId::Byte, default 200, id 0xaf6d_3a6f) : pub get MemTrainingHdtControl : pub set MemTrainingHdtControl, // TODO: Before using default, fix default.  It's possibly not correct.
    mem_mbist_data_eye_type(TokenEntryId::Byte, default 3, id 0x4e2e_dc1b) : pub get MemMbistDataEyeType : pub set MemMbistDataEyeType,
    mem_mbist_data_eye_silent_execution(TokenEntryId::Bool, default 0, id 0x3f74_c7e7) : pub get bool : pub set bool,
    mem_heal_bist_enable(TokenEntryId::Byte, default 0, id 0xfba2_3a28) : pub get MemHealBistEnable : pub set MemHealBistEnable,
    MemSelfHealBistEnable(TokenEntryId::Byte, default 0, id 0x2c23_924c), // FIXME: is it bool ?  // TODO: Before using default, fix default.  It's possibly not correct.
    mem_self_heal_bist_timeout(TokenEntryId::DWord, default 1000, id 0xbe75_97d4) : pub get u32 : pub set u32, // in ms; // TODO: Before using default, fix default.  It's possibly not correct.
    MemPmuBistTestSelect(TokenEntryId::Byte, default 0, id 0x7034_fbfb), // TODO: Before using default, fix default.  It's possibly not correct.; note: range 1...7
    mem_heal_test_select(TokenEntryId::Byte, default 0, id 0x5908_2cf2) : pub get MemHealTestSelect : pub set MemHealTestSelect,
    mem_heal_ppr_type(TokenEntryId::Byte, default 0, id 0x5418_1a61) : pub get MemHealPprType : pub set MemHealPprType,
    mem_heal_max_bank_fails(TokenEntryId::Byte, default 3, id 0x632e_55d8) : pub get u8 : pub set u8, // per bank
    mem_ecc_sync_flood(TokenEntryId::Bool, default 0, id 0x88bd_40c2) : pub get bool : pub set bool, // TODO: Before using default, fix default.  It's possibly not correct.
    mem_restore_control(TokenEntryId::Bool, default 0, id 0xfedb_01f8) : pub get bool : pub set bool,
    mem_restore_valid_days(TokenEntryId::DWord, default 30, id 0x6bd7_0482) : pub get u32 : pub set u32,
    mem_post_package_repair_enable(TokenEntryId::Bool, default 0, id 0xcdc0_3e4e) : pub get bool : pub set bool,

    // Ccx

    ccx_sev_asid_count(TokenEntryId::Byte, default 1, id 0x5587_6720) : pub get CcxSevAsidCount : pub set CcxSevAsidCount,
    ccx_min_sev_asid(TokenEntryId::DWord, default 1, id 0xa7c3_3753) : pub get u32 : pub set u32,
    ccx_ppin_opt_in(TokenEntryId::Bool, default 0, id 0x6a67_00fd) : pub get bool : pub set bool, // TODO: Before using default, fix default.  It's possibly not correct.

    // Fch

    fch_smbus_speed(TokenEntryId::Byte, default 42, id 0x2447_3329) : pub get FchSmbusSpeed : pub set FchSmbusSpeed,
    fch_rom3_base_high(TokenEntryId::DWord, default 0, id 0x3e7d_5274) : pub get u32 : pub set u32,
    FchGppClkMap(TokenEntryId::Word, default 0xffff, id 0xcd7e_6983), // u16; bitfield; GppAllClkForceOn; Auto; S0Gpp0ClkOff; ...; S1Gpp4ClkOff
    FchConsoleOutEnable(TokenEntryId::Byte, default 0, id 0xddb7_59da), // TODO: Before using default, fix default.  It's possibly not correct.
    fch_console_out_super_io_type(TokenEntryId::Byte, default 0, id 0x5c8d_6e82) : pub get FchConsoleOutSuperIoType : pub set FchConsoleOutSuperIoType, // init mode

    // Df

    df_group_d_platform(TokenEntryId::Bool, default 0, id 0x6831_8493) : pub get bool : pub set bool, // [F17M30] needs it to be true
    df_ext_ip_sync_flood_propagation(TokenEntryId::Byte, default 0, id 0xfffe_0b07) : pub get DfExtIpSyncFloodPropagation : pub set DfExtIpSyncFloodPropagation,
    df_sync_flood_propagation(TokenEntryId::Byte, default 0, id 0x4963_9134) : pub get DfSyncFloodPropagation : pub set DfSyncFloodPropagation, // enum; 0: allow syncflood propagation; 1: disable syncflood propagation; 0xff: auto
    df_mem_interleaving(TokenEntryId::Byte, default 7, id 0xce01_87ef) : pub get DfMemInterleaving : pub set DfMemInterleaving,
    df_mem_interleaving_size(TokenEntryId::Byte, default 7, id 0x2606_c42e) : pub get DfMemInterleavingSize : pub set DfMemInterleavingSize,
    df_dram_numa_per_socket(TokenEntryId::Byte, default 1, id 0x2cf3_dac9) : pub get DfDramNumaPerSocket : pub set DfDramNumaPerSocket,
    df_probe_filter(TokenEntryId::Byte, default 1, id 0x6597_c573) : pub get DfToggle : pub set DfToggle,
    df_mem_clear(TokenEntryId::Byte, default 3, id 0x9d17_7e57) : pub get DfToggle : pub set DfToggle,
    df_gmi_encrypt(TokenEntryId::Byte, default 0, id 0x08a4_5920) : pub get DfToggle : pub set DfToggle,
    df_xgmi_encrypt(TokenEntryId::Byte, default 0, id 0x6bd3_2f1c) : pub get DfToggle : pub set DfToggle,
    df_save_restore_mem_encrypt(TokenEntryId::Byte, default 1, id 0x7b3d_1f75) : pub get DfToggle : pub set DfToggle,
    df_bottom_io(TokenEntryId::Byte, default 0xe0, id 0x8fb9_8529) : pub get u8 : pub set u8, // Where the PCI MMIO hole will start (bits 31 to 24 inclusive)
    df_pci_mmio_size(TokenEntryId::DWord, default 0x1000_0000, id 0x3d9b_7d7b) : pub get u32 : pub set u32,
    df_remap_at_1tib(TokenEntryId::Byte, default 0, id 0x35ee_96f3) : pub get DfRemapAt1TiB : pub set DfRemapAt1TiB,
    //Df3Xgmi2LinkConfig(TokenEntryId::Byte, default ?, id 0xb0b6_ad3a), // enum; 0: 2link; 1: 3link; 2: 4link
    //Df3LinkMaxXgmiSpeed(TokenEntryId::Byte, default ?, id 0x53ba_449b),
    //Df4LinkMaxXgmiSpeed(TokenEntryId::Byte, default ?, id 0x3f30_7cb3),
    df_xgmi_tx_eq_mode(TokenEntryId::Byte, default 0xff, id 0xade7_9549) : pub get DfXgmiTxEqMode : pub set DfXgmiTxEqMode,
    df_cake_crc_threshold_bounds(TokenEntryId::DWord, default 100, id 0x9258_cf45) : pub get DfCakeCrcThresholdBounds : pub set DfCakeCrcThresholdBounds, // default: 0.001%
    df_invert_dram_map(TokenEntryId::Byte, default 0, id 0x6574_b2c0) : pub get DfInvertDramMap : pub set DfInvertDramMap,

    // Dxio

    dxio_vga_api_enable(TokenEntryId::Bool, default 0, id 0xbd5a_a3c6) : pub get bool : pub set bool,
    dxio_phy_param_vga(TokenEntryId::DWord, default 0xffff_ffff, id 0xde09_c43b) : pub get DxioPhyParamVga : pub set DxioPhyParamVga,
    dxio_phy_param_pole(TokenEntryId::DWord, default 0xffff_ffff, id 0xb189_447e) : pub get DxioPhyParamPole : pub set DxioPhyParamPole,
    dxio_phy_param_dc(TokenEntryId::DWord, default 0xffff_ffff, id 0x2066_7c30) : pub get DxioPhyParamDc : pub set DxioPhyParamDc,
    dxio_phy_param_iqofc(TokenEntryId::DWord, default 0, id 0x7e60_69c5) : pub get DxioPhyParamIqofc : pub set DxioPhyParamIqofc, // TODO: Before using default, fix default.  It's possibly not correct.

    // Misc

    configure_second_pcie_link(TokenEntryId::Bool, default 0, id 0x7142_8092) : pub get bool : pub set bool,
    second_pcie_link_speed(TokenEntryId::Byte, default 0, id 0x8723_750f) : pub get SecondPcieLinkSpeed : pub set SecondPcieLinkSpeed,
    second_pcie_link_max_payload(TokenEntryId::Byte, default 0xff, id 0xe02d_f04b) : pub get SecondPcieLinkMaxPayload : pub set SecondPcieLinkMaxPayload, // TODO: Before using default, fix default.  It's possibly not correct.
    performance_tracing(TokenEntryId::Bool, default 0, id 0xf27a_10f0) : pub get bool : pub set bool,
    display_pmu_training_results(TokenEntryId::Bool, default 0, id 0x9e36_a9d4) : pub get bool : pub set bool,
    workload_profile(TokenEntryId::Byte, default 0, id 0x22f4_299f) : pub get WorkloadProfile : pub set WorkloadProfile, // enum; 0...18
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::const_assert;

    #[test]
    fn test_struct_sizes() {
        const_assert!(size_of::<V2_HEADER>() == 32);
        const_assert!(size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>() == 128);
        const_assert!(size_of::<V2_HEADER>() % ENTRY_ALIGNMENT == 0);
        const_assert!(size_of::<GROUP_HEADER>() == 16);
        const_assert!(size_of::<GROUP_HEADER>() % ENTRY_ALIGNMENT == 0);
        const_assert!(size_of::<ENTRY_HEADER>() == 16);
        const_assert!(size_of::<ENTRY_HEADER>() % ENTRY_ALIGNMENT == 0);
    }
}
