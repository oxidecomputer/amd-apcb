use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_HEADER;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
pub use crate::ondisk::{ContextFormat, ContextType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::mem::{size_of};
use crate::group::{GroupItem, GroupMutItem};
use crate::entry::EntryMutItem;

pub struct APCB<'a> {
    header: &'a mut APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: Buffer<'a>,
    remaining_used_size: usize,
}

pub struct ApcbIterMut<'a> {
    header: &'a mut APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: Buffer<'a>,
    remaining_used_size: usize,
}

pub struct ApcbIter<'a> {
    header: &'a APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
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
        let header = match take_header_from_collection_mut::<APCB_GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystemError("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<APCB_GROUP_HEADER>()).ok_or_else(|| Error::FileSystemError("could not located body of Group", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Group", ""));
            },
        };
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }
    pub fn delete_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        'outer: loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let mut group = Self::next_item(&mut beginning_of_groups)?;
            if group.header.group_id.get() == group_id {
                let entry_size = group.delete_entry(entry_id, instance_id, board_instance_mask)?;
                if entry_size > 0 {
                    let old_group_size = group.header.group_size.get();
                    let new_group_size = old_group_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Group is smaller than Entry in Group", "APCB_GROUP_HEADER::group_size"))?;
                    group.header.group_size.set(new_group_size);

                    let apcb_size = self.header.apcb_size.get();
                    self.beginning_of_groups.copy_within((old_group_size as usize)..(apcb_size as usize), new_group_size as usize);
                    self.header.apcb_size.set(apcb_size.checked_sub(entry_size as u32).ok_or_else(|| Error::FileSystemError("APCB is smaller than Entry in Group", "APCB_HEADER_V2::apcb_size"))?);

                    self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "APCB_TYPE_HEADER::type_size"))?;
                }
                break 'outer;
            }
            let group = Self::next_item(&mut self.beginning_of_groups)?;
            let group_size = group.header.group_size.get() as usize;
            self.remaining_used_size = self.remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "APCB_GROUP_HEADER::group_size"))?;
        }
        Ok(())
    }
    pub fn insert_entry(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload_size: u16) -> Result<EntryMutItem> {
        loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = Self::next_item(&mut beginning_of_groups)?;
            if group.header.group_id.get() == group_id {
                let entry_size: u16 = (size_of::<APCB_TYPE_HEADER>() as u16).checked_add(payload_size).ok_or_else(|| Error::OutOfSpaceError)?;
                let group_size = group.header.group_size.get().checked_add(entry_size as u32).ok_or_else(|| Error::OutOfSpaceError)?;
                group.header.group_size.set(group_size.into());

                let beginning_of_groups_len = beginning_of_groups.len();
                let destination = beginning_of_groups.len().checked_add(entry_size.into()).ok_or_else(|| Error::OutOfSpaceError)?;
                // Move all groups after this group further up
                self.beginning_of_groups.copy_within(beginning_of_groups_len..self.remaining_used_size, destination);

                let apcb_size = self.header.apcb_size.get();
                self.header.apcb_size.set(apcb_size.checked_add(entry_size as u32).ok_or_else(|| Error::OutOfSpaceError)?);

                self.remaining_used_size = self.remaining_used_size.checked_add(entry_size as usize).ok_or_else(|| Error::OutOfSpaceError)?;

                let mut group = Self::next_item(&mut self.beginning_of_groups)?; // reload so we get a bigger slice
                return group.insert_entry(group_id, id, instance_id, board_instance_mask, entry_size, context_type, payload_size);
            }
            let group = Self::next_item(&mut self.beginning_of_groups)?;
            let group_size = group.header.group_size.get() as usize;
            self.remaining_used_size = self.remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "APCB_GROUP_HEADER::group_size"))?;
        }
        Err(Error::GroupNotFoundError)
    }
    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = Self::next_item(&mut beginning_of_groups)?;
            if group.header.group_id.get() == group_id {
                let group_size = group.header.group_size.get();

                let apcb_size = self.header.apcb_size.get();
                self.beginning_of_groups.copy_within((group_size as usize)..(apcb_size as usize), 0);
                self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::FileSystemError("Group is bigger than APCB", "APCB_HEADER_V2::apcb_size"))?);

                self.remaining_used_size = self.remaining_used_size.checked_sub(group_size as usize).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "APCB_GROUP_HEADER::group_size"))?;
                break;
            } else {
                let group = Self::next_item(&mut self.beginning_of_groups)?;
                let group_size = group.header.group_size.get() as usize;
                assert!(self.remaining_used_size >= group_size);
                self.remaining_used_size = self.remaining_used_size.checked_sub(group_size).ok_or_else(|| Error::FileSystemError("Group is bigger than remaining Iterator size", "APCB_GROUP_HEADER::group_size"))?;
            }
        }
        Ok(())
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
        let header = match take_header_from_collection::<APCB_GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::FileSystemError("could not read header of Group", ""));
             },
        };
        let group_size = header.group_size.get() as usize;
        let payload_size = group_size.checked_sub(size_of::<APCB_GROUP_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Group", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Group", ""));
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
            remaining_used_size: self.remaining_used_size,
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
            remaining_used_size: self.remaining_used_size,
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
    pub fn insert_group(&mut self, group_id: u16, signature: [u8; 4]) -> Result<GroupMutItem> {
        // TODO: insert sorted.
        let size = size_of::<APCB_GROUP_HEADER>();
        let apcb_size = self.header.apcb_size.get();
        self.header.apcb_size.set(apcb_size.checked_add(size as u32).ok_or_else(|| Error::OutOfSpaceError)?);
        let remaining_used_size = self.remaining_used_size;
        self.remaining_used_size = remaining_used_size.checked_add(size).ok_or_else(|| Error::OutOfSpaceError)?;
        if self.beginning_of_groups.len() < self.remaining_used_size {
            return Err(Error::FileSystemError("Iterator allocation is smaller than Iterator size", ""));
        }

        let mut beginning_of_group = &mut self.beginning_of_groups[remaining_used_size..self.remaining_used_size];

        let mut header = take_header_from_collection_mut::<APCB_GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::FileSystemError("could not read header of Group", ""))?;
        *header = APCB_GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::FileSystemError("could not read body of Group", ""))?;
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }
    pub fn delete_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        self.groups_mut().delete_entry(group_id, entry_id, instance_id, board_instance_mask)
    }
    pub fn insert_entry(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload_size: u16) -> Result<()> {
        match self.groups_mut().insert_entry(group_id, id, instance_id, board_instance_mask, context_type, payload_size) {
            Ok(e) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        self.groups_mut().delete_group(group_id)
    }

    pub fn load(backing_store: Buffer<'a>) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection_mut::<APCB_V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::FileSystemError("could not read APCB header", ""))?;

        if usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>() {
        } else {
            return Err(Error::FileSystemError("APCB header is too small", "APCB_V2_HEADER::header_size"));
        }
        if header.version.get() == 0x30 {
        } else {
            return Err(Error::FileSystemError("APCB header version mismatch", "APCB_V2_HEADER::version"));
        }

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()
        {
            let value = take_header_from_collection_mut::<APCB_V3_HEADER_EXT>(&mut backing_store)
                .ok_or_else(|| Error::FileSystemError("could not read extended header of APCB", ""))?;
            if value.signature == *b"ECB2" {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::signature"));
            }
            if value.struct_version.get() == 0x12 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::struct_version"));
            }
            if value.data_version.get() == 0x100 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::data_version"));
            }
            if value.ext_header_size.get() == 96 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::ext_header_size"));
            }
            if u32::from(value.data_offset.get()) == 88 {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::data_offset"));
            }
            if value.signature_ending == *b"BCBA" {
            } else {
                return Err(Error::FileSystemError("header validation failed", "APCB_V3_HEADER_EXT::signature_ending"));
            }
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        let remaining_used_size = header.apcb_size.get().checked_sub(u32::from(header.header_size.get())).ok_or_else(|| Error::FileSystemError("Iterator size is smaller than header size", "APCB_V2_HEADER::header_size"))? as usize;
        if remaining_used_size <= backing_store.len() {
        } else {
            return Err(Error::FileSystemError("Iterator size bigger than allocation", ""));
        }

        Ok(Self {
            header: header,
            v3_header_ext: v3_header_ext,
            beginning_of_groups: backing_store,
            remaining_used_size: remaining_used_size,
        })
    }
    pub fn create(backing_store: Buffer<'a>) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection_mut::<APCB_V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystemError("could not write APCB header", ""))?;
            *header = Default::default();

            let v3_header_ext = take_header_from_collection_mut::<APCB_V3_HEADER_EXT>(&mut backing_store)
                    .ok_or_else(|| Error::FileSystemError("could not write APCB extended header", ""))?;
            *v3_header_ext = Default::default();

            header
                .header_size
                .set((size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()) as u16);
            header.apcb_size = (header.header_size.get() as u32).into();
        }

        Self::load(backing_store)
    }
}
