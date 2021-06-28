use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::TOKEN_ENTRY;
use crate::ondisk::V2_HEADER;
use crate::ondisk::V3_HEADER_EXT;
use crate::ondisk::ENTRY_ALIGNMENT;
pub use crate::ondisk::{ContextFormat, ContextType, TokenType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::convert::TryInto;
use core::mem::{size_of};
use crate::group::{GroupItem, GroupMutItem};

pub struct Apcb<'a> {
    header: &'a mut V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    beginning_of_groups: Buffer<'a>,
    used_size: usize,
}

pub struct ApcbIterMut<'a> {
    header: &'a mut V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    buf: Buffer<'a>,
    remaining_used_size: usize,
}

pub struct ApcbIter<'a> {
    header: &'a V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    buf: ReadOnlyBuffer<'a>,
    remaining_used_size: usize,
}

impl<'a> ApcbIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut Buffer<'b>) -> Result<GroupMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem("unexpected EOF while reading header of Group", ""));
        }
        let header = match take_header_from_collection_mut::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystem("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystem("could not locate body of Group", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem("could not read body of Groupxx", ""));
            },
        };
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            used_size: body_len,
        })
    }

    /// Moves the point to the group with the given GROUP_ID.  Returns (offset, group_size) of it.
    pub(crate) fn move_point_to(&mut self, group_id: u16) -> Result<(usize, usize)> {
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
                remaining_used_size = remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystem("Group is bigger than remaining Iterator size", "GROUP_HEADER::group_size"))?;
            }
        }
        Err(Error::GroupNotFound)
    }

    pub(crate) fn next1(&mut self) -> Result<GroupMutItem<'a>> {
        if self.remaining_used_size == 0 {
            return Err(Error::Internal);
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem("", "GROUP_HEADER::group_size"));
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
    fn next_item<'b>(buf: &mut ReadOnlyBuffer<'b>) -> Result<GroupItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem("unexpected EOF while reading Group", ""));
        }
        let header = match take_header_from_collection::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystem("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystem("could not locate body of Group", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem("could not read body of Group2", ""));
            },
        };
        let body_len = body.len();

        Ok(GroupItem {
            header: header,
            buf: body,
            used_size: body_len,
        })
    }
    pub(crate) fn next1(&mut self) -> Result<GroupItem<'a>> {
        if self.remaining_used_size == 0 {
            return Err(Error::Internal);
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                if self.remaining_used_size >= group_size {
                } else {
                    return Err(Error::FileSystem("", "GROUP_HEADER::group_size"));
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
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            buf: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group(&self, group_id: u16) -> Option<GroupItem<'_>> {
        for group in self.groups() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn groups_mut(&mut self) -> ApcbIterMut<'_> {
        ApcbIterMut {
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            buf: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group_mut(&mut self, group_id: u16) -> Option<GroupMutItem<'_>> {
        for group in self.groups_mut() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn delete_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        let apcb_size = self.header.apcb_size.get();
        let mut group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)?;
        let size_diff = group.delete_entry(entry_id, instance_id, board_instance_mask)?;
        if size_diff > 0 {
            let size_diff = size_diff as i64;
            self.resize_group_by(group_id, -size_diff)?;
        }
        Ok(())
    }
    fn resize_group_by(&mut self, group_id: u16, size_diff: i64) -> Result<GroupMutItem<'_>> {
        let old_used_size = self.used_size;
        let apcb_size = self.header.apcb_size.get();
        if size_diff > 0 {
            let size_diff: u32 = (size_diff as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            self.header.apcb_size.set(apcb_size.checked_add(size_diff).ok_or_else(|| Error::FileSystem("Apcb is too big for format", "HEADER_V2::apcb_size"))?);
        } else {
            let size_diff: u32 = ((-size_diff) as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            self.header.apcb_size.set(apcb_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystem("Apcb is smaller than Entry in Group", "HEADER_V2::apcb_size"))?);
        }

        let self_beginning_of_groups_len = self.beginning_of_groups.len();
        let mut groups = self.groups_mut();
        let (offset, old_group_size) = groups.move_point_to(group_id)?;
        if size_diff > 0 {
            // Grow

            let old_group_size: u32 = old_group_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let size_diff: u32 = (size_diff as u64).try_into().map_err(|_| Error::ArithmeticOverflow)?;
            let new_group_size = old_group_size.checked_add(size_diff).ok_or_else(|| Error::FileSystem("Group is too big for format", "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_add(size_diff as usize).ok_or_else(|| Error::FileSystem("Entry is bigger than remaining Iterator size", "ENTRY_HEADER::entry_size"))?;
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
            let new_group_size = old_group_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystem("Group is smaller than Entry in Group", "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystem("Entry is bigger than remaining Iterator size", "ENTRY_HEADER::entry_size"))?;
            let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
            group.header.group_size.set(new_group_size);
            let buf = &mut self.beginning_of_groups[offset..];
            buf.copy_within((old_group_size as usize)..old_used_size, new_group_size as usize);
            self.used_size = new_used_size;
        }
        self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)
    }
    pub fn insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload: &[u8], priority_mask: u8) -> Result<()> {
        let mut entry_allocation: u16 = (size_of::<ENTRY_HEADER>() as u16).checked_add(payload.len().try_into().map_err(|_| Error::ArithmeticOverflow)?).ok_or_else(|| Error::OutOfSpace)?;
        while entry_allocation % (ENTRY_ALIGNMENT as u16) != 0 {
            entry_allocation += 1;
        }
        let mut group = self.resize_group_by(group_id, entry_allocation.into())?;
        let mut entries = group.entries_mut();
        // Note: On some errors, group.used_size will be reduced by insert_entry again!
        match entries.insert_entry(group_id, entry_id, instance_id, board_instance_mask, entry_allocation, context_type, payload, priority_mask) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        // Make sure that the entry exists before resizing the group
        let mut group = self.group(group_id).ok_or_else(|| Error::GroupNotFound)?;
        group.entry(entry_id, instance_id, board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
        let token_size = size_of::<TOKEN_ENTRY>() as u16;
        if token_size % (ENTRY_ALIGNMENT as u16) == 0 {
        } else {
            // Tokens that destroy the alignment in the container have not been tested, are impossible right now anyway and have never been seen.  So disallow those.

            return Err(Error::Internal);
        }
        let mut group = self.resize_group_by(group_id, token_size.into())?;
        // Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  group.insert_token has special logic in order to survive that.
        group.insert_token(group_id, entry_id, instance_id, board_instance_mask, token_id, token_value)
    }
    pub fn delete_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32) -> Result<()> {
        // Make sure that the entry exists before resizing the group
        let mut group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFound)?;
        let token_diff = group.delete_token(group_id, entry_id, instance_id, board_instance_mask, token_id)?;
        self.resize_group_by(group_id, token_diff)?;
        Ok(())
    }

    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        let apcb_size = self.header.apcb_size.get();
        let mut groups = self.groups_mut();
        let (offset, group_size) = groups.move_point_to(group_id)?;
        self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::FileSystem("Group is bigger than Apcb", "HEADER_V2::apcb_size"))?);
        let buf = &mut self.beginning_of_groups[offset..];
        buf.copy_within(group_size..(apcb_size as usize), 0);
        self.used_size = self.used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystem("Group is bigger than remaining Iterator size", "GROUP_HEADER::group_size"))?;
        Ok(())
    }

    pub fn insert_group(&mut self, group_id: u16, signature: [u8; 4]) -> Result<GroupMutItem<'_>> {
        // TODO: insert sorted.
        let size = size_of::<GROUP_HEADER>();
        let old_apcb_size = self.header.apcb_size.get();
        let new_apcb_size = old_apcb_size.checked_add(size as u32).ok_or_else(|| Error::OutOfSpace)?;
        let old_used_size = self.used_size;
        let new_used_size = old_used_size.checked_add(size).ok_or_else(|| Error::OutOfSpace)?;
        if self.beginning_of_groups.len() < new_used_size {
            return Err(Error::FileSystem("Iterator allocation is smaller than Iterator size", ""));
        }
        self.header.apcb_size.set(new_apcb_size);
        self.used_size = new_used_size;

        let mut beginning_of_group = &mut self.beginning_of_groups[old_used_size..new_used_size];

        let mut header = take_header_from_collection_mut::<GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::FileSystem("could not read header of Group", ""))?;
        *header = GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::FileSystem("could not read body of Groupz", ""))?;
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            used_size: body_len,
        })
    }

    pub fn load(backing_store: Buffer<'a>) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::FileSystem("could not read Apcb header", ""))?;

        if usize::from(header.header_size) >= size_of::<V2_HEADER>() {
        } else {
            return Err(Error::FileSystem("Apcb header is too small", "V2_HEADER::header_size"));
        }
        if header.version.get() == 0x30 {
        } else {
            return Err(Error::FileSystem("Apcb header version mismatch", "V2_HEADER::version"));
        }

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let value = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                .ok_or_else(|| Error::FileSystem("could not read extended header of Apcb", ""))?;
            if value.signature == *b"ECB2" {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::signature"));
            }
            if value.struct_version.get() == 0x12 {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::struct_version"));
            }
            if value.data_version.get() == 0x100 {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::data_version"));
            }
            if value.ext_header_size.get() == 96 {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::ext_header_size"));
            }
            if u32::from(value.data_offset.get()) == 88 {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::data_offset"));
            }
            if value.signature_ending == *b"BCBA" {
            } else {
                return Err(Error::FileSystem("header validation failed", "V3_HEADER_EXT::signature_ending"));
            }
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        let used_size = header.apcb_size.get().checked_sub(u32::from(header.header_size.get())).ok_or_else(|| Error::FileSystem("Iterator size is smaller than header size", "V2_HEADER::header_size"))? as usize;
        if used_size <= backing_store.len() {
        } else {
            return Err(Error::FileSystem("Iterator size bigger than allocation", ""));
        }

        let result = Self {
            header: header,
            v3_header_ext: v3_header_ext,
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
    pub fn create(backing_store: Buffer<'a>) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem("could not write Apcb header", ""))?;
            *header = Default::default();

            let v3_header_ext = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystem("could not write Apcb extended header", ""))?;
            *v3_header_ext = Default::default();

            header
                .header_size
                .set((size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()) as u16);
            header.apcb_size = (header.header_size.get() as u32).into();
        }

        Self::load(backing_store)
    }
}
