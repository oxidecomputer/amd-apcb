use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::TYPE_HEADER;
use crate::ondisk::TOKEN_ENTRY;
use crate::ondisk::V2_HEADER;
use crate::ondisk::V3_HEADER_EXT;
use crate::ondisk::TYPE_ALIGNMENT;
pub use crate::ondisk::{ContextFormat, ContextType, TokenType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::convert::TryInto;
use core::mem::{size_of};
use crate::group::{GroupItem, GroupMutItem};
use crate::entry::EntryMutItem;

pub struct APCB<'a> {
    header: &'a mut V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    beginning_of_groups: Buffer<'a>,
    used_size: usize,
}

pub struct ApcbIterMut<'a> {
    header: &'a mut V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    beginning_of_groups: Buffer<'a>,
    remaining_used_size: usize,
}

pub struct ApcbIter<'a> {
    header: &'a V2_HEADER,
    v3_header_ext: Option<V3_HEADER_EXT>,
    beginning_of_groups: ReadOnlyBuffer<'a>,
    remaining_used_size: usize,
}

impl<'a> ApcbIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.beginning_of_groups.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut Buffer<'b>) -> Result<GroupMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Group", ""));
        }
        let header = match take_header_from_collection_mut::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystemError("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Group", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                panic!("XX");
                return Err(Error::FileSystemError("could not read body of Groupxx", ""));
            },
        };
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }
}

impl<'a> Iterator for ApcbIterMut<'a> {
    type Item = GroupMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.beginning_of_groups) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                assert!(self.remaining_used_size >= group_size);
                self.remaining_used_size -= group_size;
                Some(e)
            },
            Err(e) => {
                None
            },
        }
    }
}

impl<'a> ApcbIter<'a> {
    /// It's useful to have some way of NOT mutating self.beginning_of_groups.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut ReadOnlyBuffer<'b>) -> Result<GroupItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading Group", ""));
        }
        let header = match take_header_from_collection::<GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystemError("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<GROUP_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Group", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Group2", ""));
            },
        };
        let body_len = body.len();

        Ok(GroupItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }
}

impl<'a> Iterator for ApcbIter<'a> {
    type Item = GroupItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.beginning_of_groups) {
            Ok(e) => {
                let group_size = e.header.group_size.get() as usize;
                assert!(self.remaining_used_size >= group_size);
                self.remaining_used_size -= group_size;
                Some(e)
            },
            Err(e) => {
                None
            },
        }
    }
}

impl<'a> APCB<'a> {
    pub fn groups(&self) -> ApcbIter {
        ApcbIter {
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            beginning_of_groups: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group(&self, group_id: u16) -> Option<GroupItem> {
        for group in self.groups() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn groups_mut(&mut self) -> ApcbIterMut {
        ApcbIterMut {
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            beginning_of_groups: self.beginning_of_groups,
            remaining_used_size: self.used_size,
        }
    }
    pub fn group_mut(&mut self, group_id: u16) -> Option<GroupMutItem> {
        for group in self.groups_mut() {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn delete_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        let apcb_size = self.header.apcb_size.get();
        let mut group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFoundError)?;
        let size_diff = group.delete_entry(entry_id, instance_id, board_instance_mask)?;
        if size_diff > 0 {
            let old_group_size = group.header.group_size.get();
            let new_group_size = old_group_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystemError("Group is smaller than Entry in Group", "GROUP_HEADER::group_size"))?;
            group.header.group_size.set(new_group_size);

            self.beginning_of_groups.copy_within((old_group_size as usize)..self.used_size, new_group_size as usize);
            self.header.apcb_size.set(apcb_size.checked_sub(size_diff as u32).ok_or_else(|| Error::FileSystemError("APCB is smaller than Entry in Group", "HEADER_V2::apcb_size"))?);

            self.used_size = self.used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "TYPE_HEADER::type_size"))?;
        }
        Ok(())
    }
    /// Side effect: Moves cursor to the group so resized.
    pub fn resize_group_by(&mut self, group_id: u16, size_diff: i64) -> Result<GroupMutItem> {
        let old_used_size = self.used_size;
        let apcb_size = self.header.apcb_size.get();
        if size_diff > 0 {
            let size_diff: u32 = (size_diff as u64).try_into().unwrap();
            self.header.apcb_size.set(apcb_size.checked_add(size_diff).ok_or_else(|| Error::FileSystemError("APCB is too big for format", "HEADER_V2::apcb_size"))?);
        } else {
            let size_diff: u32 = ((-size_diff) as u64).try_into().unwrap();
            self.header.apcb_size.set(apcb_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystemError("APCB is smaller than Entry in Group", "HEADER_V2::apcb_size"))?);
        }

        let self_beginning_of_groups_len = self.beginning_of_groups.len();
        let group = self.group_mut(group_id).ok_or_else(|| Error::GroupNotFoundError)?;
        let old_group_size = group.header.group_size.get();
        if size_diff > 0 {
            // Grow

            let size_diff: u32 = (size_diff as u64).try_into().unwrap();
            let new_group_size = old_group_size.checked_add(size_diff).ok_or_else(|| Error::FileSystemError("Group is too big for format", "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_add(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "TYPE_HEADER::type_size"))?;
            if new_used_size <= self_beginning_of_groups_len {
            } else {
                return Err(Error::OutOfSpaceError);
            }
            group.header.group_size.set(new_group_size);
            self.beginning_of_groups.copy_within((old_group_size as usize)..old_used_size, new_group_size as usize);
            self.used_size = new_used_size;
        } else if size_diff < 0 {
            let size_diff: u32 = ((-size_diff) as u64).try_into().unwrap();
            let new_group_size = old_group_size.checked_sub(size_diff).ok_or_else(|| Error::FileSystemError("Group is smaller than Entry in Group", "GROUP_HEADER::group_size"))?;
            let new_used_size = old_used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "TYPE_HEADER::type_size"))?;
            group.header.group_size.set(new_group_size);
            self.beginning_of_groups.copy_within((old_group_size as usize)..old_used_size, new_group_size as usize);
            self.used_size = new_used_size;
        }
        self.group_mut(group_id).ok_or_else(|| Error::GroupNotFoundError)
    }
    pub fn insert_entry(&mut self, group_id: u16, type_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload: &[u8], priority_mask: u8) -> Result<EntryMutItem> {
        let mut entry_allocation: u16 = (size_of::<TYPE_HEADER>() as u16).checked_add(payload.len().try_into().unwrap()).ok_or_else(|| Error::OutOfSpaceError)?;
        while entry_allocation % (TYPE_ALIGNMENT as u16) != 0 {
            entry_allocation += 1;
        }
        let mut group = self.resize_group_by(group_id, entry_allocation.into())?;
        // Note: On some errors, group.used_size will be reduced by insert_entry again!
        group.insert_entry(group_id, type_id, instance_id, board_instance_mask, entry_allocation, context_type, payload, priority_mask)
    }
    /// Side effect: Moves iterator to unspecified item
    pub fn insert_token(&mut self, group_id: u16, type_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        let token_size = size_of::<TOKEN_ENTRY>() as u16;
        assert!(token_size % (TYPE_ALIGNMENT as u16) == 0);
        let mut group = self.resize_group_by(group_id, token_size.into())?;
        // ?? Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  This is bad when iterating over the group members until the end--it will not end well.
        group.insert_token(group_id, type_id, instance_id, board_instance_mask, token_id, token_value)
    }

    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        let apcb_size = self.header.apcb_size.get();
        let mut remaining_used_size = self.used_size;
        loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = ApcbIterMut::next_item(&mut beginning_of_groups)?;
            if group.header.group_id.get() == group_id {
                let group_size = group.header.group_size.get();
                self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::FileSystemError("Group is bigger than APCB", "HEADER_V2::apcb_size"))?);

                self.beginning_of_groups.copy_within((group_size as usize)..(apcb_size as usize), 0);

                self.used_size = self.used_size.checked_sub(group_size as usize).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "GROUP_HEADER::group_size"))?;
                break;
            } else {
                let group = ApcbIterMut::next_item(&mut self.beginning_of_groups)?;
                let group_size = group.header.group_size.get() as usize;
                assert!(self.used_size >= group_size);
                remaining_used_size = remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "GROUP_HEADER::group_size"))?;
            }
        }
        Ok(())
    }

    pub fn insert_group(&mut self, group_id: u16, signature: [u8; 4]) -> Result<GroupMutItem> {
        // TODO: insert sorted.
        let size = size_of::<GROUP_HEADER>();
        let apcb_size = self.header.apcb_size.get();
        self.header.apcb_size.set(apcb_size.checked_add(size as u32).ok_or_else(|| Error::OutOfSpaceError)?);
        let used_size = self.used_size;
        self.used_size = used_size.checked_add(size).ok_or_else(|| Error::OutOfSpaceError)?;
        if self.beginning_of_groups.len() < self.used_size {
            return Err(Error::FileSystemError("Iterator allocation is smaller than Iterator size", ""));
        }

        let mut beginning_of_group = &mut self.beginning_of_groups[used_size..self.used_size];

        let mut header = take_header_from_collection_mut::<GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::FileSystemError("could not read header of Group", ""))?;
        *header = GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::FileSystemError("could not read body of Groupz", ""))?;
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }

    pub fn load(backing_store: Buffer<'a>) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::FileSystemError("could not read APCB header", ""))?;

        if usize::from(header.header_size) >= size_of::<V2_HEADER>() {
        } else {
            return Err(Error::FileSystemError("APCB header is too small", "V2_HEADER::header_size"));
        }
        if header.version.get() == 0x30 {
        } else {
            return Err(Error::FileSystemError("APCB header version mismatch", "V2_HEADER::version"));
        }

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()
        {
            let value = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                .ok_or_else(|| Error::FileSystemError("could not read extended header of APCB", ""))?;
            if value.signature == *b"ECB2" {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::signature"));
            }
            if value.struct_version.get() == 0x12 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::struct_version"));
            }
            if value.data_version.get() == 0x100 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::data_version"));
            }
            if value.ext_header_size.get() == 96 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::ext_header_size"));
            }
            if u32::from(value.data_offset.get()) == 88 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::data_offset"));
            }
            if value.signature_ending == *b"BCBA" {
            } else {
                return Err(Error::FileSystemError("header validation failed", "V3_HEADER_EXT::signature_ending"));
            }
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        let used_size = header.apcb_size.get().checked_sub(u32::from(header.header_size.get())).ok_or_else(|| Error::FileSystemError("Iterator size is smaller than header size", "V2_HEADER::header_size"))? as usize;
        if used_size <= backing_store.len() {
        } else {
            return Err(Error::FileSystemError("Iterator size bigger than allocation", ""));
        }

        Ok(Self {
            header: header,
            v3_header_ext: v3_header_ext,
            beginning_of_groups: backing_store,
            used_size,
        })
    }
    pub fn create(backing_store: Buffer<'a>) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystemError("could not write APCB header", ""))?;
            *header = Default::default();

            let v3_header_ext = take_header_from_collection_mut::<V3_HEADER_EXT>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystemError("could not write APCB extended header", ""))?;
            *v3_header_ext = Default::default();

            header
                .header_size
                .set((size_of::<V2_HEADER>() + size_of::<V3_HEADER_EXT>()) as u16);
            header.apcb_size = (header.header_size.get() as u32).into();
        }

        Self::load(backing_store)
    }
}
