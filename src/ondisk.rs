// This file contains the Apcb on-disk format.  Please only change it in coordination with the AMD PSP team.  Even then, you probably shouldn't.

use byteorder::LittleEndian;
use core::mem::{replace, size_of};
use num_derive::FromPrimitive;
use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned, U16, U32};

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

#[repr(u16)]
#[derive(Debug, PartialEq, FromPrimitive, Copy, Clone)]
pub enum TokenType {
    Bool = 0,
    Byte = 1,
    Word = 2,
    DWord = 4,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes() {
        assert!(size_of::<V2_HEADER>() == 32);
        assert!(size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>() == 128);
        assert!(size_of::<V2_HEADER>() % ENTRY_ALIGNMENT == 0);
        assert!(size_of::<GROUP_HEADER>() == 16);
        assert!(size_of::<GROUP_HEADER>() % ENTRY_ALIGNMENT == 0);
        assert!(size_of::<ENTRY_HEADER>() == 16);
        assert!(size_of::<ENTRY_HEADER>() % ENTRY_ALIGNMENT == 0);
    }
}
