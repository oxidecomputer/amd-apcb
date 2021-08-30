use crate::types::{Error, FileSystemError, Result};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::GroupId;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::TOKEN_ENTRY;
use crate::ondisk::V2_HEADER;
use crate::ondisk::V3_HEADER_EXT;
use crate::ondisk::ENTRY_ALIGNMENT;
pub use crate::ondisk::{PriorityLevels, ContextFormat, ContextType, EntryId, EntryCompatible, UnionAsBytes};
use crate::ondisk::{take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut, HeaderOfSequence};
use core::convert::TryInto;
use core::default::Default;
use core::mem::{size_of};
use crate::group::{GroupItem, GroupMutItem};
use static_assertions::const_assert;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use zerocopy::AsBytes;

pub struct Apcb<'a> {
    header: &'a mut V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    beginning_of_groups: &'a mut [u8],
    used_size: usize,
}

pub struct ApcbIterMut<'a> {
    buf: &'a mut [u8],
    remaining_used_size: usize,
}

pub struct ApcbIter<'a> {
    buf: &'a [u8],
    remaining_used_size: usize,
}

impl<'a> ApcbIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b mut [u8]) -> Result<GroupMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
        }
        let header = match take_header_from_collection_mut::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
            },
        };
        let body_len = body.len();

        Ok(GroupMutItem {
            header,
            buf: body,
            used_size: body_len,
        })
    }

    /// Moves the point to the group with the given GROUP_ID.  Returns (offset, group_size) of it.
    pub(crate) fn move_point_to(&mut self, group_id: GroupId) -> Result<(usize, usize)> {
        let group_id = group_id.to_u16().unwrap();
        let mut remaining_used_size = self.remaining_used_size;
        let mut offset = 0usize;
        loop {
            let mut buf = &mut self.buf[..remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            let group = ApcbIterMut::next_item(&mut buf)?;
            let group_size = group.header.group_size.get();
            if group.header.group_id.get() == group_id {
                return Ok((offset, group_size as usize));
            } else {
                let group = ApcbIterMut::next_item(&mut self.buf)?;
                let group_size = group.header.group_size.get() as usize;
                offset = offset.checked_add(group_size).ok_or_else(|| Error::ArithmeticOverflow)?;
                remaining_used_size = remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"))?;
            }
        }
        Err(Error::GroupNotFound)
    }

    pub(crate) fn next1(&mut self) -> Result<GroupMutItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"));
                }
                self.remaining_used_size -= group_size;
                Ok(e)
            },
            Err(e) => {
                Err(e)
            },
        }
    }
}

impl<'a> Iterator for ApcbIterMut<'a> {
    type Item = GroupMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(item) => {
                Some(item)
            },
            Err(_) => {
                None
            },
        }
    }
}

impl<'a> ApcbIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b [u8]) -> Result<GroupItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
        }
        let header = match take_header_from_collection::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"));
            },
        };
        let body_len = body.len();

        Ok(GroupItem {
            header,
            buf: body,
            used_size: body_len,
        })
    }
    pub(crate) fn next1(&mut self) -> Result<GroupItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"));
                }
                self.remaining_used_size -= group_size;
                Ok(e)
            },
            Err(e) => {
                Err(e)
            },
        }
    }
    /// Validates the entries (recursively).  Also consumes iterator.
    pub(crate) fn validate(mut self) -> Result<()> {
        while self.remaining_used_size > 0 {
            match self.next1() {
                Ok(item) => {
                    GroupId::from_u16(item.header.group_id.get()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_id"))?;
                    item.entries().validate()?;
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for ApcbIter<'a> {
    type Item = GroupItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(item) => {
                Some(item)
            },
            Err(_) => {
                None
            },
        }
    }
}

impl<'a> Apcb<'a> {
    pub fn groups(&self) -> ApcbIter<'_> {
        ApcbIter {
            buf: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group(&self, group_id: GroupId) -> Option<GroupItem<'_>> {
        for group in self.groups() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn groups_mut(&mut self) -> ApcbIterMut<'_> {
        ApcbIterMut {
            buf: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group_mut(&mut self, group_id: GroupId) -> Option<GroupMutItem<'_>> {
        //let group_id = group_id.to_u16().unwrap();
        for group in self.groups_mut() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn delete_entry(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        let group_id = entry_id.group_id();
        let mut group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)?;
        let size_diff = group.delete_entry(entry_id, instance_id, board_instance_mask)?;
        if size_diff > 0 {
            let size_diff = size_diff as i64;
            self.resize_group_by(group_id, -size_diff)?;
        }
        Ok(())
    }
    fn resize_group_by(&mut self, group_id: GroupId, size_diff: i64) -> Result<GroupMutItem<'_>> {
        let old_used_size = self.used_size;
        let apcb_size = self.header.apcb_size.get();
        if size_diff > 0 {
            let size_diff: u32 = (size_diff as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            self.header.apcb_size.set(apcb_size.checked_add(size_diff).ok_or_else(|| Error::FileSystem(FileSystemError::PayloadTooBig, "HEADER_V2::apcb_size"))?);
        } else {
            let size_diff: u32 = ((-size_diff) as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            self.header.apcb_size.set(apcb_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "HEADER_V2::apcb_size"))?);
        }

        let self_beginning_of_groups_len = self.beginning_of_groups.len();
        let mut groups = self.groups_mut();
        let (offset, old_group_size) = groups.move_point_to(group_id)?;
        if size_diff > 0 {
            // Grow

            let old_group_size: u32 = old_group_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let size_diff: u32 = (size_diff as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let new_group_size = old_group_size.checked_add(size_diff).ok_or_else(|| Error::FileSystem(FileSystemError::PayloadTooBig, "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_add(size_diff as usize).ok_or_else(|| Error::FileSystem(FileSystemError::PayloadTooBig, "ENTRY_HEADER::entry_size"))?;
            if new_used_size <= self_beginning_of_groups_len {
            } else {
                return Err(Error::OutOfSpace);
            }
            let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
            group.header.group_size.set(new_group_size);
            let buf = &mut self.beginning_of_groups[offset..];
            buf.copy_within((old_group_size as usize)..old_used_size, new_group_size as usize);
            self.used_size = new_used_size;
        } else if size_diff < 0 {
            let old_group_size: u32 = old_group_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let size_diff: u32 = ((-size_diff) as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let new_group_size = old_group_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::entry_size"))?;
            let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
            group.header.group_size.set(new_group_size);
            let buf = &mut self.beginning_of_groups[offset..];
            buf.copy_within((old_group_size as usize)..old_used_size, new_group_size as usize);
            self.used_size = new_used_size;
        }
        self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)
    }
    fn internal_insert_entry(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload_size: usize, priority_mask: PriorityLevels, payload_initializer: &mut dyn FnMut(&mut [u8]) -> ()) -> Result<()> {
        let group_id = entry_id.group_id();
        let mut entry_allocation: u16 = (size_of::<ENTRY_HEADER>() as u16).checked_add(payload_size.try_into().map_err(|_| Error::ArithmeticOverflow)?).ok_or_else(|| Error::OutOfSpace)?;
        while entry_allocation % (ENTRY_ALIGNMENT as u16) != 0 {
            entry_allocation += 1;
        }
        let mut group = self.resize_group_by(group_id, entry_allocation.into())?;
        let mut entries = group.entries_mut();
        // Note: On some errors, group.used_size will be reduced by insert_entry again!
        match entries.insert_entry(entry_id, instance_id, board_instance_mask, entry_allocation, context_type, payload_size, payload_initializer, priority_mask) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn insert_entry(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, context_type: ContextType, priority_mask: PriorityLevels, payload: &[u8]) -> Result<()> {
        let payload_size = payload.len();
        self.internal_insert_entry(entry_id, instance_id, board_instance_mask, context_type, payload_size, priority_mask, &mut |body: &mut [u8]| {
            body.copy_from_slice(payload);
        })
    }
    /// Inserts a new entry (see insert_entry), puts PAYLOAD into it.
    pub fn insert_struct_entry<T: EntryCompatible + AsBytes>(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, priority_mask: PriorityLevels, payload: &T) -> Result<()> {
        let blob = payload.as_bytes();
        if T::is_entry_compatible(entry_id, blob) {
            self.insert_entry(entry_id, instance_id, board_instance_mask, ContextType::Struct, priority_mask, blob)
        } else {
            Err(Error::EntryTypeMismatch)
        }
    }
    /// Inserts a new entry (see insert_entry), puts PAYLOAD into it.  T can be a enum of struct refs (PlatformSpecificElementRef, PlatformTuningElementRef) or just one struct.
    pub fn insert_struct_sequence_as_entry<T: EntryCompatible + UnionAsBytes>(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, priority_mask: PriorityLevels, payload: &[T]) -> Result<()> {
        let mut payload_size: usize = 0;
        for item in payload {
            let blob = item.as_bytes();
            if !T::is_entry_compatible(entry_id, blob) {
                return Err(Error::EntryTypeMismatch);
            }
            payload_size = payload_size.checked_add(blob.len()).ok_or_else(|| Error::ArithmeticOverflow)?;
        }
        self.internal_insert_entry(entry_id, instance_id, board_instance_mask, ContextType::Struct, payload_size, priority_mask, &mut |body: &mut [u8]| {
            let mut body = body;
            for item in payload {
                let source = item.as_bytes();
                let (a, rest) = body.split_at_mut(source.len());
                a.copy_from_slice(source);
                body = rest;
            }
        })
    }

    /// Inserts a new entry (see insert_entry), puts HEADER and then PAYLOAD into it.
    pub fn insert_headered_struct_array_entry<H: EntryCompatible + AsBytes + HeaderOfSequence>(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, priority_mask: PriorityLevels, header: &H, payload: &[H::TailSequenceType]) -> Result<()> {
        let blob = header.as_bytes();
        if H::is_entry_compatible(entry_id, blob) {
            let payload_size = size_of::<H>().checked_add(size_of::<H::TailSequenceType>().checked_mul(payload.len()).ok_or_else(|| Error::ArithmeticOverflow)?).ok_or_else(|| Error::ArithmeticOverflow)?;
            self.internal_insert_entry(entry_id, instance_id, board_instance_mask, ContextType::Struct, payload_size, priority_mask, &mut |body: &mut [u8]| {
                let mut body = body;
                let (a, rest) = body.split_at_mut(blob.len());
                a.copy_from_slice(blob);
                body = rest;
                for item in payload {
                    let source = item.as_bytes();
                    let (a, rest) = body.split_at_mut(source.len());
                    a.copy_from_slice(source);
                    body = rest;
                }
            })
        } else {
            Err(Error::EntryTypeMismatch)
        }
    }

    pub fn insert_token(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        let group_id = entry_id.group_id();
        // Make sure that the entry exists before resizing the group
        let group = self.group(group_id).ok_or_else(|| Error::GroupNotFound)?;
        group.entry(entry_id, instance_id, board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
        // Tokens that destroy the alignment in the container have not been tested, are impossible right now anyway and have never been seen.  So disallow those.
        const TOKEN_SIZE: u16 = size_of::<TOKEN_ENTRY>() as u16;
        const_assert!(TOKEN_SIZE % (ENTRY_ALIGNMENT as u16) == 0);
        let mut group = self.resize_group_by(group_id, TOKEN_SIZE.into())?;
        // Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  group.insert_token has special logic in order to survive that.
        group.insert_token(entry_id, instance_id, board_instance_mask, token_id, token_value)
    }
    pub fn delete_token(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, token_id: u32) -> Result<()> {
        let group_id = entry_id.group_id();
        // Make sure that the entry exists before resizing the group
        let mut group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)?;
        let token_diff = group.delete_token(entry_id, instance_id, board_instance_mask, token_id)?;
        self.resize_group_by(group_id, token_diff)?;
        Ok(())
    }

    pub fn delete_group(&mut self, group_id: GroupId) -> Result<()> {
        let apcb_size = self.header.apcb_size.get();
        let mut groups = self.groups_mut();
        let (offset, group_size) = groups.move_point_to(group_id)?;
        self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "HEADER_V2::apcb_size"))?);
        let buf = &mut self.beginning_of_groups[offset..];
        buf.copy_within(group_size..(apcb_size as usize), 0);
        self.used_size = self.used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER::group_size"))?;
        Ok(())
    }

    pub fn insert_group(&mut self, group_id: GroupId, signature: [u8; 4]) -> Result<GroupMutItem<'_>> {
        // TODO: insert sorted.
        let size = size_of::<GROUP_HEADER>();
        let old_apcb_size = self.header.apcb_size.get();
        let new_apcb_size = old_apcb_size.checked_add(size as u32).ok_or_else(|| Error::OutOfSpace)?;
        let old_used_size = self.used_size;
        let new_used_size = old_used_size.checked_add(size).ok_or_else(|| Error::OutOfSpace)?;
        if self.beginning_of_groups.len() < new_used_size {
            return Err(Error::OutOfSpace);
        }
        self.header.apcb_size.set(new_apcb_size);
        self.used_size = new_used_size;

        let mut beginning_of_group = &mut self.beginning_of_groups[old_used_size..new_used_size];

        let mut header = take_header_from_collection_mut::<GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"))?;
        *header = GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.to_u16().unwrap().into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "GROUP_HEADER"))?;
        let body_len = body.len();

        Ok(GroupMutItem {
            header,
            buf: body,
            used_size: body_len,
        })
    }

    pub fn load(backing_store: &'a mut [u8]) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER"))?;

        if usize::from(header.header_size) >= size_of::<V2_HEADER>() {
        } else {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER::header_size"));
        }
        if header.version.get() == 0x30 {
        } else {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER::version"));
        }

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let value = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT"))?;
            if value.signature == *b"ECB2" {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::signature"));
            }
            if value.struct_version.get() == 0x12 {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::struct_version"));
            }
            if value.data_version.get() == 0x100 {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::data_version"));
            }
            if value.ext_header_size.get() == 96 {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::ext_header_size"));
            }
            if u32::from(value.data_offset.get()) == 88 {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::data_offset"));
            }
            if value.signature_ending == *b"BCBA" {
            } else {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT::signature_ending"));
            }
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        let used_size = header.apcb_size.get().checked_sub(u32::from(header.header_size.get())).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER::header_size"))? as usize;
        if used_size <= backing_store.len() {
        } else {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER::header_size"));
        }

        let result = Self {
            header,
            v3_header_ext,
            beginning_of_groups: backing_store,
            used_size,
        };
        match result.groups().validate() {
            Ok(_) => {
            },
            Err(e) => {
                return Err(e);
            }
        }
        Ok(result)
    }
    /// User is expected to call this once after modifying anything in the apcb (including insertions and deletions).
    /// We update both the checksum and the unique_apcb_instance.
    pub fn update_checksum(backing_store: &'_ mut [u8]) -> Result<()> {
        let apcb_size: u32;
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER"))?;
            header.checksum_byte = 0;
            apcb_size = header.apcb_size.get();
        }
        let mut checksum_byte = 0u8;
        {
            let backing_store = &mut *backing_store;
            for c in &backing_store[..apcb_size as usize] {
                checksum_byte = checksum_byte.wrapping_add(*c);
            }
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER"))?;
            let checksum_byte = (0x100u16 - u16::from(checksum_byte)) as u8; // Note: This can overflow

            // Use the chance to also update unique_apcb_instance (assumption: user calls update_checksum only when there was an actual change).
            header.unique_apcb_instance.set(header.unique_apcb_instance.get().wrapping_add(1));
            header.checksum_byte = checksum_byte.wrapping_sub(1); // correct for changed unique_apcb_instance
        }
        Ok(())
    }
    pub fn create(backing_store: &'a mut [u8], unique_apcb_instance: u32) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V2_HEADER"))?;
            *header = Default::default();
            header.unique_apcb_instance.set(unique_apcb_instance);

            let v3_header_ext = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "V3_HEADER_EXT"))?;
            *v3_header_ext = Default::default();

            header
                .header_size
                .set((size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()) as u16);
            header.apcb_size = (header.header_size.get() as u32).into();
        }
        Self::update_checksum(backing_store)?;
        Self::load(backing_store)
    }
    /// Note: Each modification in the APCB causes the value of unique_apcb_instance to change.
    pub fn unique_apcb_instance(&self) -> u32 {
        self.header.unique_apcb_instance.get()
    }
}
