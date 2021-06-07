// This file contains the APCB on-disk format.  Please only change it in coordination with the AMD PSP team.  Even then, you probably shouldn't.

use byteorder::LittleEndian;
use core::mem::size_of;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use zerocopy::{AsBytes, FromBytes, Unaligned, U16, U32};

#[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
#[repr(C, packed)]
pub struct APCB_V2_HEADER {
    pub signature: [u8; 4],
    pub header_size: U16<LittleEndian>, // == sizeof(APCB_V2_HEADER); but 128 for V3
    pub version: U16<LittleEndian>,     // == 0x30
    pub apcb_size: U32<LittleEndian>,
    pub unique_apcb_instance: U32<LittleEndian>,
    pub checksum_byte: u8,
    reserved1: [u8; 3],  // 0
    reserved2: [u32; 3], // 0
}

impl Default for APCB_V2_HEADER {
    fn default() -> Self {
        Self {
            signature: *b"APCB",
            header_size: (size_of::<Self>() as u16).into(),
            version: 0x30u16.into(),
            apcb_size: (size_of::<Self>() as u32).into(),
            unique_apcb_instance: 0u32.into(), // probably invalid
            checksum_byte: 0,                  // probably invalid
            reserved1: [0, 0, 0],
            reserved2: [0, 0, 0],
        }
    }
}

#[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
#[repr(C, packed)]
pub struct APCB_V3_HEADER_EXT {
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
    reserved_7: [u32; 2],                   // 0 0
    pub data_offset: U16<LittleEndian>,     // 0x58
    pub header_checksum: u8,
    reserved_8: u8,       // 0
    reserved_9: [u32; 3], // 0 0 0
    pub integrity_sign: [u8; 32],
    reserved_10: [u32; 3],         // 0 0 0
    pub signature_ending: [u8; 4], // "BCPA"
}

impl Default for APCB_V3_HEADER_EXT {
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
            reserved_7: [0, 0],
            data_offset: 0x58u16.into(),
            header_checksum: 0, // invalid--but unused by AMD Rome
            reserved_8: 0,
            reserved_9: [0, 0, 0],
            integrity_sign: [0; 32], // invalid--but unused by AMD Rome
            reserved_10: [0; 3],
            signature_ending: *b"BCPA",
        }
    }
}

#[repr(u16)]
#[derive(Debug, PartialEq, FromPrimitive)]
pub enum GroupId {
    Psp = 0x1701, // usual signature: "PSPG"
    Ccx = 0x1702,
    Df = 0x1703,     // usual signature: "DFG "
    Memory = 0x1704, // usual signature: "MEMG"
    Gnb = 0x1705,
    Fch = 0x1706,
    Cbs = 0x1707,
    Oem = 0x1708,
    Token = 0x3000, // usual signature: "TOKN"
}

#[repr(u16)]
#[derive(Debug, PartialEq, FromPrimitive)]
pub enum PspTypeId {
    BoardIdGettingMethod = 0x60,
}

#[repr(u16)]
#[derive(Debug, PartialEq, FromPrimitive)]
pub enum DfTypeId {
    SlinkConfig = 0xCC,
    XgmiTxEq = 0xD0,
    XgmiPhyOverride = 0xDD,
}

#[repr(u16)]
#[derive(Debug, PartialEq, FromPrimitive)]
pub enum MemoryTypeId {
    SpdInfo = 0x30,
    DimmInfoSmbus = 0x31,
    DimmConfigInfoId = 0x32,
    MemOverclockConfig = 0x33,

    PsoData = 0x40,

    PsUdimmDdr4OdtPat = 0x41,
    PsUdimmDdr4CadBus = 0x42,
    PsUdimmDdr4DataBus = 0x43,
    PsUdimmDdr4MaxFreq = 0x44,
    PsUdimmDdr4StretchFreq = 0x45,

    PsRdimmDdr4OdtPat = 0x46,
    PsRdimmDdr4CadBus = 0x47,
    PsRdimmDdr4DataBus = 0x48,
    PsRdimmDdr4MaxFreq = 0x49,
    PsRdimmDdr4StretchFreq = 0x4A,

    Ps3dsRdimmDdr4MaxFreq = 0x4B,
    Ps3dsRdimmDdr4StretchFreq = 0x4C,
    Ps3dsRdimmDdr4DataBus = 0x4D,

    ConsoleOutControl = 0x50,
    EventControl = 0x51,
    ErrorOutEventControl = 0x52,
    ExtVoltageControl = 0x53,

    PsLrdimmDdr4OdtPat = 0x54,
    PsLrdimmDdr4CadBus = 0x55,
    PsLrdimmDdr4DataBus = 0x56,
    PsLrdimmDdr4MaxFreq = 0x57,
    PsLrdimmDdr4StretchFreq = 0x58,

    PsSodimmDdr4OdtPat = 0x59,
    PsSodimmDdr4CadBus = 0x5A,
    PsSodimmDdr4DataBus = 0x5B,
    PsSodimmDdr4MaxFreq = 0x5C,
    PsSodimmDdr4StretchFreq = 0x5D,

    DdrPostPackageRepair = 0x5E,

    PsDramdownDdr4OdtPat = 0x70,
    PsDramdownDdr4CadBus = 0x71,
    PsDramdownDdr4DataBus = 0x72,
    PsDramdownDdr4MaxFreq = 0x73,
    PsDramdownDdr4StretchFreq = 0x74,

    PlatformTuning = 0x75,
}

#[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
#[repr(C, packed)]
pub struct APCB_GROUP_HEADER {
    pub signature: [u8; 4],
    pub group_id: U16<LittleEndian>,
    pub header_size: U16<LittleEndian>, // == sizeof(APCB_GROUP_HEADER)
    pub version: U16<LittleEndian>,     // == 0 << 4 | 1
    reserved: u16,
    pub group_size: U32<LittleEndian>, // including header!
}

impl Default for APCB_GROUP_HEADER {
    fn default() -> Self {
        Self {
            signature: *b"    ",   // probably invalid
            group_id: 0u16.into(), // probably invalid
            header_size: (size_of::<Self>() as u16).into(),
            version: 0x01u16.into(),
            reserved: 0,
            group_size: (size_of::<Self>() as u32).into(), // probably invalid
        }
    }
}

#[derive(FromBytes, AsBytes, Unaligned, Clone, Copy)]
#[repr(C, packed)]
pub struct APCB_TYPE_HEADER {
    pub group_id: U16<LittleEndian>, // should be equal to the group's group_id
    pub type_id: U16<LittleEndian>,  // meaning depends on context_type
    pub type_size: U16<LittleEndian>, // including header
    pub instance_id: U16<LittleEndian>,
    pub context_type: u8, // 0: struct, 1: APCB parameter, 2: APCB V3 token[then, type_id means something else]
    pub context_format: u8, // 0: raw, 1: sort ascenting by unit_size, 2: sort descending by unit_size[don't use]
    pub unit_size: u8,      // in Byte.  Applicable when ContextType == 2.  value should be 8
    pub priority_mask: u8,
    pub key_size: u8, // Sorting key size; <= unit_size. Applicable when ContextFormat = 1. (or != 0)
    pub key_pos: u8,  // Sorting key position of the unit specified of UnitSize
    pub board_instance_mask: u8, // Board-specific APCB instance mask
}

impl Default for APCB_TYPE_HEADER {
    fn default() -> Self {
        Self {
            group_id: 0u16.into(),                        // probably invalid
            type_id: 0u16.into(),                         // probably invalid
            type_size: (size_of::<Self>() as u16).into(), // probably invalid
            instance_id: 0u16.into(),                     // probably invalid
            context_type: 2,
            context_format: 0,
            unit_size: 8,
            priority_mask: 0, // FIXME: Verify
            key_size: 0,
            key_pos: 0,
            board_instance_mask: 0xFF, // FIXME: Verify
        }
    }
}

pub const APCB_TYPE_ALIGNMENT: usize = 4;
/*
APCB:
        Header V2
        V3 Header Ext
        [Group]

Group:
        Header
        Body
                [Type * alignment(4 Byte)]
Type:
    Header
    Body
    Alignment
*/
