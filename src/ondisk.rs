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
    //Raw(u16),
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
            //GroupId::Raw(x) => (*x).into(),
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
                //x => GroupId::Raw(x as u16),
                _ => None,
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

    Raw(u16),
}

impl ToPrimitive for PspEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::BoardIdGettingMethod => 0x60,
            Self::Raw(x) => (*x).into(),
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
                x => Some(Self::Raw(x as u16)),
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
    Raw(u16),
}

impl ToPrimitive for CcxEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Raw(x) => (*x).into(),
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
                x => Some(Self::Raw(x as u16)),
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
    Raw(u16)
}

impl ToPrimitive for DfEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::SlinkConfig => 0xCC,
            Self::XgmiTxEq => 0xD0,
            Self::XgmiPhyOverride => 0xDD,
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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

    PsoData,

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
    ErrorOutEventControl,
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

    Raw(u16),
}

impl ToPrimitive for MemoryEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::SpdInfo => 0x30,
            Self::DimmInfoSmbus => 0x31,
            Self::DimmConfigInfoId => 0x32,
            Self::MemOverclockConfig => 0x33,

            Self::PsoData => 0x40,

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
            Self::ErrorOutEventControl => 0x52,
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

            Self::Raw(x) => (*x) as i64,
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

                0x40 => Self::PsoData,

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
                0x52 => Self::ErrorOutEventControl,
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

                x => Self::Raw(x as u16),
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
    Raw(u16),
}

impl ToPrimitive for GnbEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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
    Raw(u16),
}

impl ToPrimitive for FchEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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
    Raw(u16),
}

impl ToPrimitive for CbsEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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
    Raw(u16),
}

impl ToPrimitive for OemEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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
    Raw(u16),
}

impl ToPrimitive for TokenEntryId {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Bool => 0,
            Self::Byte => 1,
            Self::Word => 2,
            Self::DWord => 4,
            Self::Raw(x) => (*x) as i64,
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
                x => Self::Raw(x as u16),
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
    reserved: U16<LittleEndian>,
    pub group_size: U32<LittleEndian>, // including header!
}

#[repr(u8)]
#[derive(Debug, PartialEq, FromPrimitive, Copy, Clone)]
pub enum ContextFormat {
    Raw = 0,
    SortAscending = 1,  // (sort by unit size)
    SortDescending = 2, // don't use
}

#[repr(u8)]
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
            reserved: 0u16.into(),
            group_size: (size_of::<Self>() as u32).into(), // probably invalid
        }
    }
}

#[derive(FromBytes, AsBytes, Unaligned, Debug)]
#[repr(C, packed)]
pub struct ENTRY_HEADER {
    pub group_id: U16<LittleEndian>, // should be equal to the group's group_id
    pub entry_id: U16<LittleEndian>,  // meaning depends on context_type
    pub entry_size: U16<LittleEndian>, // including header
    pub instance_id: U16<LittleEndian>,
    pub context_type: u8,   // see ContextType enum
    pub context_format: u8, // see ContextFormat enum
    pub unit_size: u8,      // in Byte.  Applicable when ContextType == 2.  value should be 8
    pub priority_mask: u8,
    pub key_size: u8, // Sorting key size; <= unit_size. Applicable when ContextFormat = 1. (or != 0)
    pub key_pos: u8,  // Sorting key position of the unit specified of UnitSize
    pub board_instance_mask: U16<LittleEndian>, // Board-specific Apcb instance mask
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

    #[repr(u8)]
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
            dimm_slot_present: BU8 : pub get Result<bool> : pub set bool,
            socket_id: u8 : pub get u8 : pub set u8,
            channel_id: u8 : pub get u8 : pub set u8,
            dimm_id: u8 : pub get u8 : pub set u8,
            dimm_smbus_address: u8 : pub get u8 : pub set u8,
            i2c_mux_address: u8 : pub get u8 : pub set u8,
            mux_control_address: u8 : pub get u8 : pub set u8,
            mux_channel: u8 : pub get u8 : pub set u8,
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
        pub fn new(socket_id: u8, channel_id: u8, dimm_id: u8, dimm_smbus_address: u8, i2c_mux_address: u8, mux_control_address: u8, mux_channel: u8) -> Self {
            Self {
                dimm_slot_present: BU8(1),
                socket_id,
                channel_id,
                dimm_id,
                dimm_smbus_address,
                i2c_mux_address,
                mux_control_address,
                mux_channel,
            }
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

    //pub type PsoData = &[u8];

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

    #[repr(u32)]
    #[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Copy, Clone)]
    pub enum PortType {
        PcieHt0 = 0,
        PcieHt1 = 2,
        PcieMmio = 5,
        FchHtIo = 6,
        FchMmio = 7,
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ExtVoltageControl {
            enable: BU8 : pub get Result<bool> : pub set bool,
            _reserved: [u8; 3],
            input_port: U32<LittleEndian> : pub get u32 : pub set u32,
            output_port: U32<LittleEndian> : pub get u32 : pub set u32,
            input_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // size in Byte; one of [1, 2, 4]
            output_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // size in Byte; one of [1, 2, 4]
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

    // Usually an array of those is used
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct CadBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32, // 1 or 2
            ddr_rate: U32<LittleEndian> : pub get u32 : pub set u32, // 0xA00|0x2800|0x1_0000|0x4_0000|0xA_0000|0x28_0000|0x100_0000
            vdd_io: U32<LittleEndian> : pub get u32 : pub set u32, // always 1
            dimm0_ranks: U32<LittleEndian> : pub get u32 : pub set u32, // 1|2|4|6
            dimm1_ranks: U32<LittleEndian> : pub get u32 : pub set u32, // 1|2|4|6

            gear_down_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved: U16<LittleEndian>,
            slow_mode: U16<LittleEndian> : pub get u16 : pub set u16,
            _reserved_2: U16<LittleEndian>,
            address_command_control: U32<LittleEndian> : pub get u32 : pub set u32, // 24 bit; often all used bytes are equal

            cke_drive_strength: u8 : pub get u8 : pub set u8,
            cs_odt_drive_strength: u8 : pub get u8 : pub set u8,
            address_command_drive_strength: u8 : pub get u8 : pub set u8,
            clk_drive_strength: u8 : pub get u8 : pub set u8,
        }
    }

    impl Default for CadBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rate: 0xa00.into(),
                vdd_io: 1.into(), // always
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

    impl EntryCompatible for CadBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4CadBus) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus) => true,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4CadBus) => true,
                // TODO: Check EntryId::Memory(MemoryEntryId::PsSodimmDdr4CadBus) => true
                // Definitely not: PsDramdownDdr4CadBus.
                _ => false,
            }
        }
    }

    // See <https://www.micron.com/-/media/client/global/documents/products/data-sheet/dram/ddr4/8gb_auto_ddr4_dram.pdf>
    // Usually an array of those is used
    // Note: This structure is not used for soldered-down DRAM!
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct DataBusElement {
            dimm_slots_per_channel: U32<LittleEndian> : pub get u32 : pub set u32,
            ddr_rate: U32<LittleEndian> : pub get u32 : pub set u32,
            vdd_io: U32<LittleEndian> : pub get u32 : pub set u32,
            dimm0_ranks: U32<LittleEndian> : pub get u32 : pub set u32, // 1|2|4|6
            dimm1_ranks: U32<LittleEndian> : pub get u32 : pub set u32, // 1|2|4|6

            rtt_nom: U32<LittleEndian> : pub get u32 : pub set u32, // contains nominal on-die termination mode (not used on writes); 0|1|7
            rtt_wr: U32<LittleEndian> : pub get u32 : pub set u32, // contains dynamic on-die termination mode (used on writes); 0|1|4
            rtt_park: U32<LittleEndian> : pub get u32 : pub set u32, // contains ODT termination resistor to be used when ODT is low; 4|5|7
            dq_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data
            dqs_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for data strobe (bit clock)
            odt_drive_strength: U32<LittleEndian> : pub get u32 : pub set u32, // for on-die termination
            pmu_phy_vref: U32<LittleEndian> : pub get u32 : pub set u32,
            dq_vref: U32<LittleEndian> : pub get u32 : pub set u32, // MR6 vref calibration value; 23|30|32
        }
    }

    impl Default for DataBusElement {
        fn default() -> Self {
            Self {
                dimm_slots_per_channel: 1.into(),
                ddr_rate: 0x1373200.into(), // always
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
                dq_vref: 23.into(),
            }
        }
    }

    impl EntryCompatible for DataBusElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsUdimmDdr4DataBus) => true,
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4DataBus) => true,
                EntryId::Memory(MemoryEntryId::Ps3dsRdimmDdr4DataBus) => true,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4DataBus) => true,
                // TODO: Check EntryId::Memory(PsSodimmDdr4DataBus) => true
                // Definitely not: EntryId::PsDramdownDdr4DataBus => true
                _ => false,
            }
        }
    }

    // Usually an array of those is used
    // Note: This structure is not used for LR DRAM
    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct MaxFreqElement {
            dimm_slots_per_channel: u8 : pub get u8 : pub set u8,
            _reserved: u8,
            pub conditions: [U16<LittleEndian>; 4], // number of dimm on a channel, number of single-rank dimm, number of dual-rank dimm, number of quad-rank dimm // FIXME make accessible
            pub speeds: [U16<LittleEndian>; 3], // speed limit with voltage 1.5 V, 1.35 V, 1.25 V // FIXME make accessible
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
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

    #[derive(FromBytes, AsBytes, Unaligned, Clone, Copy, PartialEq, Debug)]
    #[repr(C, packed)]
    pub struct ErrorOutEventControlBeepCodePeakAttr {
        // 5 bits: PeakCnt (number of valid bits, 0 based)
        // 3 bits: PulseWidths (in units of 0.1 s)
        // 4 bits: repeat count
        value: u32,
    }

    // This avoids depending on rust-packed_struct or rust-bitfield additionally.
    impl ErrorOutEventControlBeepCodePeakAttr {
        /// PULSE_WIDTH: in units of 0.1 s
        pub const fn new(peak_count: u32, pulse_width: u32, repeat_count: u32) -> Option<Self> {
            if peak_count < 32 {
            } else {
                return None;
            }
            if pulse_width < 8 {
            } else {
                return None;
            }
            if repeat_count < 16 {
            } else {
                return None;
            }
            Some(Self {
                value: peak_count | (pulse_width << 5) | (repeat_count << 8),
            })
        }
        pub fn peak_count(&self) -> u32 {
            self.value & 31
        }
        pub fn pulse_width(&self) -> u32 {
            (self.value >> 5) & 7
        }
        pub fn repeat_count(&self) -> u32 {
            (self.value >> 8) & 15
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ErrorOutEventControlBeepCode {
            error_type: U16<LittleEndian> : pub get u16 : pub set u16,
            peak_map: U16<LittleEndian> : pub get u16 : pub set u16,
            pub peak_attr: ErrorOutEventControlBeepCodePeakAttr, // FIXME Make accessible
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ErrorOutEventControl116 { // Milan
            enable_error_reporting: BU8 : pub get Result<bool> : pub set bool,
            enable_error_reporting_gpio: BU8 : pub get Result<bool> : pub set bool,
            enable_error_reporting_beep_codes: BU8 : pub get Result<bool> : pub set bool,
            enable_using_handshake: BU8 : pub get Result<bool> : pub set bool, // otherwise see output_delay
            input_port: U32<LittleEndian> : pub get u32 : pub set u32, // for handshake
            output_delay: U32<LittleEndian> : pub get u32 : pub set u32, // if no handshake; in units of 10 ns.
            output_port: U32<LittleEndian> : pub get u32 : pub set u32,
            stop_on_first_fatal_error: BU8: pub get Result<bool> : pub set bool,
            _reserved: [u8; 3],
            input_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // in Byte; 1|2|4
            output_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // in Byte; 1|2|4
            input_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // PortType; default: 6
            output_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // PortType; default: 6
            clear_acknowledgement: BU8 : pub get Result<bool> : pub set bool,
            _reserved_2: [u8; 3],
            pub gpio: Gpio, // FIXME: Make accessible
            _reserved_3: u8,
            pub beep_code_table: [ErrorOutEventControlBeepCode; 8], // FIXME: Make accessible
            enable_heart_beat: BU8 : pub get Result<bool> : pub set bool,
            enable_power_good_gpio: BU8 : pub get Result<bool> : pub set bool,
            pub power_good_gpio: Gpio, // FIXME: Make accessible
            _reserved_4: [u8; 3], // pad
        }
    }

    impl Default for ErrorOutEventControl116 {
        fn default() -> Self {
            Self {
                enable_error_reporting: BU8(0),
                enable_error_reporting_gpio: BU8(0),
                enable_error_reporting_beep_codes: BU8(0),
                enable_using_handshake: BU8(0),
                input_port: 0x84.into(),
                output_delay: 15000.into(),
                output_port: 0x80.into(),
                stop_on_first_fatal_error: BU8(0),
                _reserved: [0; 3],
                input_port_size: 4.into(),
                output_port_size: 4.into(),
                input_port_type: (PortType::FchHtIo as u32).into(),
                output_port_type: (PortType::FchHtIo as u32).into(),
                clear_acknowledgement: BU8(0),
                _reserved_2: [0; 3],
                gpio: Gpio::new(0, 0, 0), // probably invalid
                _reserved_3: 0,
                beep_code_table: [
                    ErrorOutEventControlBeepCode {
                        error_type: U16::<LittleEndian>::new(0x3fff),
                        peak_map: 1.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(8, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x4fff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x5fff.into(),
                        peak_map: 3.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x6fff.into(),
                        peak_map: 4.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x7fff.into(),
                        peak_map: 5.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x8fff.into(),
                        peak_map: 6.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x9fff.into(),
                        peak_map: 7.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0xffff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(4, 0, 0).unwrap(),
                    },
                ],
                enable_heart_beat: BU8(0),
                enable_power_good_gpio: BU8(0),
                power_good_gpio: Gpio::new(0, 0, 0), // probably invalid
                _reserved_4: [0; 3],
            }
        }
    }

    impl EntryCompatible for ErrorOutEventControl116 {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::ErrorOutEventControl) => true,
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct ErrorOutEventControl112 { // older than Milan
            enable_error_reporting: BU8 : pub get Result<bool> : pub set bool,
            enable_error_reporting_gpio: BU8 : pub get Result<bool> : pub set bool,
            enable_error_reporting_beep_codes: BU8 : pub get Result<bool> : pub set bool,
            enable_using_handshake: BU8 : pub get Result<bool> : pub set bool, // otherwise see output_delay
            pub input_port: U32<LittleEndian>, // for handshake
            pub output_delay: U32<LittleEndian>, // if no handshake; in units of 10 ns.
            pub output_port: U32<LittleEndian>,
            stop_on_first_fatal_error: BU8 : pub get Result<bool> : pub set bool,
            _reserved: [u8; 3],
            input_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // in Byte; 1|2|4
            output_port_size: U32<LittleEndian> : pub get u32 : pub set u32, // in Byte; 1|2|4
            input_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // default: 6
            output_port_type: U32<LittleEndian> : pub get Result<PortType> : pub set PortType, // default: 6
            clear_acknowledgement: BU8 : pub get Result<bool> : pub set bool,
            pub gpio: Gpio, // FIXME: Make accessible.
            // @40
            pub beep_code_table: [ErrorOutEventControlBeepCode; 8], // FIXME: Make accessible.
            enable_heart_beat: BU8 : pub get Result<bool> : pub set bool,
            enable_power_good_gpio: BU8 : pub get Result<bool> : pub set bool,
            pub power_good_gpio: Gpio, // FIXME: Make accessible.
            _reserved_2: [u8; 3], // pad
        }
    }

    impl Default for ErrorOutEventControl112 {
        fn default() -> Self {
            Self {
                enable_error_reporting: BU8(0),
                enable_error_reporting_gpio: BU8(0),
                enable_error_reporting_beep_codes: BU8(0),
                enable_using_handshake: BU8(0),
                input_port: 0x84.into(),
                output_delay: 15000.into(),
                output_port: 0x80.into(),
                stop_on_first_fatal_error: BU8(0),
                _reserved: [0; 3],
                input_port_size: 4.into(),
                output_port_size: 4.into(),
                input_port_type: (PortType::FchHtIo as u32).into(),
                output_port_type: (PortType::FchHtIo as u32).into(),
                clear_acknowledgement: BU8(0),
                gpio: Gpio::new(0, 0, 0), // probably invalid
                beep_code_table: [
                    ErrorOutEventControlBeepCode {
                        error_type: U16::<LittleEndian>::new(0x3fff),
                        peak_map: 1.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(8, 0, 0).unwrap(), // #: 5, 3, 4; 31; 7; 15
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x4fff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x5fff.into(),
                        peak_map: 3.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x6fff.into(),
                        peak_map: 4.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x7fff.into(),
                        peak_map: 5.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x8fff.into(),
                        peak_map: 6.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0x9fff.into(),
                        peak_map: 7.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(20, 0, 0).unwrap(),
                    },
                    ErrorOutEventControlBeepCode {
                        error_type: 0xffff.into(),
                        peak_map: 2.into(),
                        peak_attr: ErrorOutEventControlBeepCodePeakAttr::new(4, 0, 0).unwrap(),
                    },
                ],
                enable_heart_beat: BU8(0),
                enable_power_good_gpio: BU8(0),
                power_good_gpio: Gpio::new(0, 0, 0), // probably invalid
                _reserved_2: [0; 3],
            }
        }
    }

    impl EntryCompatible for ErrorOutEventControl112 {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::ErrorOutEventControl) => true,
                _ => false,
            }
        }
    }

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct OdtPatElement {
            dimm_rank_bitmap: U32<LittleEndian> : pub get u32 : pub set u32, // 2-dimm version: 4 bits dimm0; 28 bits dimm1.  1-dimm version: 32 bits dimm0
            cs0_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs1_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs2_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
            cs3_odt_pattern: U32<LittleEndian> : pub get u32 : pub set u32,
        }
    }

    impl OdtPatElement {
        pub fn dimm0_rank(&self) -> u32 {
            self.dimm_rank_bitmap.get() & 15
        }
        pub fn dimm1_rank(&self) -> u32 {
            self.dimm_rank_bitmap.get() >> 4
        }
    }

    impl Default for OdtPatElement {
        fn default() -> Self {
            Self {
                dimm_rank_bitmap: 0.into(), // probably invalid
                cs0_odt_pattern: 0.into(),
                cs1_odt_pattern: 0.into(),
                cs2_odt_pattern: 0.into(),
                cs3_odt_pattern: 0.into(),
            }
        }
    }

    impl EntryCompatible for OdtPatElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::PsRdimmDdr4OdtPat) => true,
                EntryId::Memory(MemoryEntryId::PsLrdimmDdr4OdtPat) => true,
                _ => false,
            }
        }
    }

    bitfield! {
        #[derive(FromBytes, AsBytes, Unaligned)]
        #[repr(C, packed)]
        pub struct DdrPostPackageRepairElement(U64<LittleEndian>);
        no default BitRange;
        impl Debug;
        u16;
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

    impl BitRange<u16> for DdrPostPackageRepairElement {
        fn bit_range(&self, msb: usize, lsb: usize) -> u16 {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 15);
            let mask = (1 << width) - 1;
            ((self.0.get() >> lsb) & mask) as u16
        }
        fn set_bit_range(&mut self, msb: usize, lsb: usize, value: u16) {
            assert!(lsb <= msb);
            let width = msb - lsb + 1;
            assert!(width <= 15);
            let mask = (1 << width) - 1;
            assert!(value <= mask);
            let mut dest = self.0.get();
            dest &=! ((mask as u64) << lsb);
            dest |= (value as u64) << lsb;
            self.0.set(dest);
        }
    }

    impl Bit for DdrPostPackageRepairElement {
        fn bit(&self, index: usize) -> bool {
            match self.0.get() & (1 << index) {
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

    impl EntryCompatible for DdrPostPackageRepairElement {
        fn is_entry_compatible(entry_id: EntryId, _prefix: &[u8]) -> bool {
            match entry_id {
                EntryId::Memory(MemoryEntryId::DdrPostPackageRepair) => true,
                _ => false,
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
            const_assert!(size_of::<CadBusElement>() == 36);
            const_assert!(size_of::<DataBusElement>() == 52);
            const_assert!(size_of::<MaxFreqElement>() == 16);
            const_assert!(size_of::<LrMaxFreqElement>() == 16);
            const_assert!(size_of::<Gpio>() == 3);
            const_assert!(size_of::<ErrorOutEventControlBeepCode>() == 8);
            const_assert!(size_of::<ErrorOutEventControlBeepCodePeakAttr>() == 4);
            assert!(offset_of!(ErrorOutEventControl116, beep_code_table) == 44);
            assert!(offset_of!(ErrorOutEventControl116, enable_heart_beat) == 108);
            assert!(offset_of!(ErrorOutEventControl116, power_good_gpio) == 110);
            const_assert!(size_of::<ErrorOutEventControl116>() == 116);
            assert!(offset_of!(ErrorOutEventControl112, beep_code_table) == 40);
            assert!(offset_of!(ErrorOutEventControl112, enable_heart_beat) == 104);
            assert!(offset_of!(ErrorOutEventControl112, power_good_gpio) == 106);
            const_assert!(size_of::<ErrorOutEventControl112>() == 112);
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct IdApcbMapping {
            id_and_feature_mask: u8 : pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get u8 : pub set u8,
            apcb_instance_index: u8 : pub get u8 : pub set u8,
        }
    }
    impl IdApcbMapping {
        pub fn new(id_and_feature_mask: u8, id_and_feature_value: u8, apcb_instance_index: u8) -> Self {
            Self {
                id_and_feature_mask,
                id_and_feature_value,
                apcb_instance_index,
            }
        }
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

    make_accessors! {
        #[derive(FromBytes, AsBytes, Unaligned, PartialEq, Debug)]
        #[repr(C, packed)]
        pub struct IdRevApcbMapping {
            id_and_rev_and_feature_mask: u8 : pub get u8 : pub set u8, // bit 7: normal or feature-controlled?  other bits: mask
            id_and_feature_value: u8 : pub get u8 : pub set u8,
            rev_and_feature_value: u8 : pub get u8 : pub set u8,
            apcb_instance_index: u8 : pub get u8 : pub set u8,
        }
    }

    impl IdRevApcbMapping {
        pub fn new(id_and_rev_and_feature_mask: u8, id_and_feature_value: u8, rev_and_feature_value: u8, apcb_instance_index: u8) -> Self {
            Self {
                id_and_rev_and_feature_mask,
                id_and_feature_value,
                rev_and_feature_value,
                apcb_instance_index,
            }
        }
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
