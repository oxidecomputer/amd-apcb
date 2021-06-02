// This file contains the APCB on-disk format.  Please only change it in coordination with the AMD PSP team.  Even then, you probably shouldn't.

use core::mem::size_of;

#[derive(Serialize, Deserialize)]
#[repr(C)]
pub struct APCB_V2_HEADER {
	pub signature: [u8; 4],
	pub header_size: u16, // == sizeof(APCB_V2_HEADER)
	pub version: u16, // == 0x30
	pub apcb_size: u32,
	pub unique_apcb_instance: u32,
	pub checksum_byte: u8,
	reserved1: [u8; 3], // 0
	reserved2: [u32; 3], // 0
}

// FIXME: APCB v3 has more fields, starting with another signature.  It's probably better to model that as an additional header.

/*
#[derive(Serialize, Deserialize)]
#[repr(C)]
pub struct APCB_V3_ADDITIONAL_HEADER {
}
*/

impl Default for APCB_V2_HEADER {
	fn default() -> Self {
		Self {
			signature: *b"APCB",
			header_size: size_of::<Self>() as u16,
			version: 0x30,
			apcb_size: size_of::<Self>() as u32,
			unique_apcb_instance: 0, // probably invalid
			checksum_byte: 0, // probably invalid
			reserved1: [0, 0, 0],
			reserved2: [0, 0, 0],
		}
	}
}

#[derive(Serialize, Deserialize)]
#[repr(C)]
pub struct APCB_GROUP_HEADER {
	pub signature: [u8; 4],
	pub group_id: u16,
	pub header_size: u16, // == sizeof(APCB_GROUP_HEADER)
	pub version: u16, // == 0 << 4 | 1
	reserved: u16,
	pub group_size: u32, // including header!
}

impl Default for APCB_GROUP_HEADER {
	fn default() -> Self {
		Self {
			signature: *b"    ", // probably invalid
			group_id: 0, // probably invalid
			header_size: size_of::<Self>() as u16,
			version: 0x01, // FIXME: Verify ?
			reserved: 0, // FIXME: Verify
			group_size: size_of::<Self>() as u32, // probably invalid
		}
	}
}

#[derive(Serialize, Deserialize)]
#[repr(C)]
pub struct APCB_TYPE_HEADER {
	pub group_id: u16, // should be equal to the group's group_id
	pub type_id: u16, // meaning depends on context_type
	pub type_size: u16, // including header
	pub instance_id: u16,
	pub context_type: u8, // 0: struct, 1: APCB parameter, 2: APCB V3 token[then, type_id means something else]
	pub context_format: u8, // 0: raw, 1: sort ascenting by unit_size, 2: sort descending by unit_size[don't use]
	pub unit_size: u8, // in Byte.  Applicable when ContextType == 2.  value should be 8
	pub priority_mask: u8,
	pub key_size: u8, // Sorting key size. Should be smaller than or equal to UnitSize. Applicable when ContextFormat = 1. (or != 0)
	pub key_pos: u8, // Sorting key position of the unit specified of UnitSize
	pub board_instance_mask: u8, // Board-specific APCB instance mask
}

impl Default for APCB_TYPE_HEADER {
	fn default() -> Self {
		Self {
			group_id: 0, // probably invalid
			type_id: 0, // probably invalid
			type_size: size_of::<Self>() as u16, // probably invalid
			instance_id: 0, // probably invalid
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
        Header V3
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
