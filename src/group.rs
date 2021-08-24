use crate::types::{Error, FileSystemError, Result};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::GroupId;
use crate::ondisk::ENTRY_ALIGNMENT;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::TOKEN_ENTRY;
pub use crate::ondisk::{ContextFormat, ContextType, EntryId, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::convert::TryInto;
use core::mem::{size_of};
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;

use crate::entry::{EntryItem, EntryMutItem, EntryItemBody};

#[derive(Debug)]
pub struct GroupItem<'a> {
    pub(crate) header: &'a GROUP_HEADER,
    pub(crate) buf: &'a [u8],
    pub(crate) used_size: usize,
}

#[derive(Debug)]
pub struct GroupIter<'a> {
    pub(crate) header: &'a GROUP_HEADER,
    buf: &'a [u8],
    remaining_used_size: usize,
}

impl<'a> Iterator for GroupIter<'a> {
    type Item = EntryItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match self.next1() {
            Ok(e) => {
                Some(e)
            },
            Err(_) => {
                None
            },
        }
    }
}
impl<'a> GroupIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b [u8]) -> Result<EntryItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
        }
        let header = match take_header_from_collection::<ENTRY_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
            }
        };
        GroupId::from_u16(header.group_id.get()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::group_id"))?;
        ContextFormat::from_u8(header.context_format).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::context_format"))?;
        let context_type = ContextType::from_u8(header.context_type).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::context_type"))?;
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size.checked_sub(size_of::<ENTRY_HEADER>()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, ENTRY_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
            },
        };

        let unit_size = header.unit_size;
        let entry_id = header.entry_id.get();
        Ok(EntryItem {
            header,
            body: EntryItemBody::<&'_ [u8]>::from_slice(unit_size, entry_id, context_type, body)?,
        })
    }

    pub(crate) fn next1(&mut self) -> Result<EntryItem<'a>> {
        if self.remaining_used_size == 0 {
            panic!("Internal error");
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                if e.header.group_id.get() == self.header.group_id.get() {
                } else {
                    return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::group_id"));
                }
                let entry_size = e.header.entry_size.get() as usize;
                if self.remaining_used_size >= entry_size {
                } else {
                    return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::entry_size"));
                }
                self.remaining_used_size -= entry_size;
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
                    item.validate()?;
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }
        Ok(())
    }
}

impl GroupItem<'_> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> GroupId {
        GroupId::from_u16(self.header.group_id.get()).unwrap()
    }

    /// This finds the entry with the given ID, INSTANCE_ID and compatible BOARD_INSTANCE_MASK, if any.  If you have a board_id, BOARD_INSTANCE_MASK = 1 << board_id
    pub fn entry(&self, id: EntryId, instance_id: u16, board_instance_mask: u16) -> Option<EntryItem<'_>> {
        for entry in self.entries() {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() & board_instance_mask != 0 {
                return Some(entry);
            }
        }
        None
    }

    pub fn entries(&self) -> GroupIter<'_> {
        GroupIter {
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}

#[derive(Debug)]
pub struct GroupMutIter<'a> {
    pub(crate) header: &'a mut GROUP_HEADER,
    buf: &'a mut [u8],
    remaining_used_size: usize,
}

impl<'a> GroupMutIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut &'b mut [u8]) -> Result<EntryMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
        }
        let header = match take_header_from_collection_mut::<ENTRY_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
            }
        };
        let context_type = ContextType::from_u8(header.context_type).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::context_type"))?;
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size.checked_sub(size_of::<ENTRY_HEADER>()).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, ENTRY_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"));
            },
        };

        let unit_size = header.unit_size;
        let entry_id = header.entry_id.get();
        Ok(EntryMutItem {
            header,
            body: EntryItemBody::<&'_ mut [u8]>::from_slice(unit_size, entry_id, context_type, body)?,
        })
    }

    /// Find the place BEFORE which the entry (GROUP_ID, ENTRY_ID, INSTANCE_ID, BOARD_INSTANCE_MASK) is supposed to go.
    pub(crate) fn move_insertion_point_before(&mut self, group_id: u16, type_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            match Self::next_item(&mut buf) {
                Ok(e) => {
                    if (e.group_id(), e.type_id(), e.instance_id(), e.board_instance_mask()) < (group_id, type_id, instance_id, board_instance_mask) {
                        self.next().unwrap();
                    } else {
                        break;
                    }
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }
        Ok(())
    }
    /// Find the place BEFORE which the entry (GROUP_ID, ID, INSTANCE_ID, BOARD_INSTANCE_MASK) is.
    /// Returns how much it moved the point, and the size of the entry.
    pub(crate) fn move_point_to(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16) -> Result<(usize, usize)> {
        let mut offset = 0usize;
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                return Err(Error::EntryNotFound);
            }
            match Self::next_item(&mut buf) {
                Ok(e) => {
                    let entry_size = e.header.entry_size.get();
                    if (e.id(), e.instance_id(), e.board_instance_mask()) != (entry_id, instance_id, board_instance_mask) {
                        self.next().unwrap();
                        offset = offset.checked_add(entry_size.into()).ok_or_else(|| Error::ArithmeticOverflow)?;
                        while offset % ENTRY_ALIGNMENT != 0 {
                            offset = offset.checked_add(1).ok_or_else(|| Error::ArithmeticOverflow)?;
                        }
                    } else {
                        return Ok((offset, entry_size.into()));
                    }
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }
    }
    /// Inserts the given entry data at the right spot.
    /// Precondition: Caller already increased the group size by entry_allocation.
    pub(crate) fn insert_entry(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, entry_allocation: u16, context_type: ContextType, payload_size: usize, payload_initializer: &mut dyn FnMut(&mut [u8]) -> (), priority_mask: u8) -> Result<()> {
        let group_id = entry_id.group_id();
        let entry_id = entry_id.type_id();

        // Make sure that move_insertion_point_before does not notice the new uninitialized entry
        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_allocation as usize).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER::entry_size"))?;
        self.move_insertion_point_before(group_id.to_u16().unwrap(), entry_id, instance_id, board_instance_mask)?;

        let mut buf = &mut self.buf[..]; // already done: offset
        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        buf.copy_within(0..self.remaining_used_size, entry_allocation as usize);

        //let mut buf = &mut self.buf[..(self.remaining_used_size + entry_allocation as usize)];
        let header = take_header_from_collection_mut::<ENTRY_HEADER>(&mut buf).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"))?;
        *header = ENTRY_HEADER::default();
        header.group_id.set(group_id.to_u16().unwrap());
        header.entry_id.set(entry_id);
        header.entry_size.set(entry_allocation); // FIXME: Verify
        header.instance_id.set(instance_id);
        header.context_type = context_type as u8;
        header.context_format = ContextFormat::Raw as u8;
        header.unit_size = 0;
        header.key_size = 0;
        header.key_pos = 0;
        if context_type == ContextType::Tokens {
            header.context_format = ContextFormat::SortAscending as u8;
            header.unit_size = 8;
            header.key_size = 4;
            header.key_pos = 0;
        }
        header.priority_mask = priority_mask;

        // Note: The following is settable by the user via EntryMutItem set-accessors: context_type, context_format, unit_size, priority_mask, key_size, key_pos
        header.board_instance_mask.set(board_instance_mask);
        let body = take_body_from_collection_mut(&mut buf, payload_size.into(), ENTRY_ALIGNMENT).ok_or_else(|| Error::FileSystem(FileSystemError::InconsistentHeader, "ENTRY_HEADER"))?;
        payload_initializer(body);
        self.remaining_used_size = self.remaining_used_size.checked_add(entry_allocation as usize).ok_or_else(|| Error::OutOfSpace)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct GroupMutItem<'a> {
    pub(crate) header: &'a mut GROUP_HEADER,
    pub(crate) buf: &'a mut [u8],
    pub(crate) used_size: usize,
}

impl<'a> GroupMutItem<'a> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> GroupId {
        GroupId::from_u16(self.header.group_id.get()).unwrap()
    }

    /// This finds the entry with the given ID, INSTANCE_ID and compatible BOARD_INSTANCE_MASK, if any.  If you have a board_id, BOARD_INSTANCE_MASK = 1 << board_id
    pub fn entry_mut(&mut self, id: EntryId, instance_id: u16, board_instance_mask: u16) -> Option<EntryMutItem<'_>> {
        for entry in self.entries_mut() {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() & board_instance_mask != 0 {
                return Some(entry);
            }
        }
        None
    }
    pub(crate) fn delete_entry(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16) -> Result<u32> {
        let mut entries = self.entries_mut();
        let (offset, entry_size) = entries.move_point_to(entry_id, instance_id, board_instance_mask)?;
        let buf = &mut self.buf[offset..];
        buf.copy_within(entry_size..self.used_size, offset);
        return Ok(entry_size as u32);
    }
    /// Resizes the given entry by SIZE_DIFF.
    /// Precondition: If SIZE_DIFF > 0, caller needs to have expanded the group by SIZE_DIFF already.
    /// Precondition: If SIZE_DIFF < 0, caller needs to call resize_entry_by BEFORE resizing the group.
    pub(crate) fn resize_entry_by(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, size_diff: i64) -> Result<EntryMutItem<'_>> {
        let mut old_used_size = self.used_size;

        let mut entries = self.entries_mut();
        let (offset, entry_size) = entries.move_point_to(entry_id, instance_id, board_instance_mask)?;
        let entry_size: u16 = entry_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;

        if size_diff > 0 {
            let size_diff: usize = size_diff.try_into().map_err(|_| Error::ArithmeticOverflow)?;
            old_used_size = old_used_size.checked_sub(size_diff).ok_or_else(|| Error::ArithmeticOverflow)?
        }
        let old_entry_size = entry_size;
        let new_entry_size: u16 = if size_diff > 0 {
            let size_diff = size_diff as u64;
            old_entry_size.checked_add(size_diff.try_into().map_err(|_| Error::ArithmeticOverflow)?).ok_or_else(|| Error::OutOfSpace)?
        } else {
            let size_diff = (-size_diff) as u64;
            old_entry_size.checked_sub(size_diff.try_into().map_err(|_| Error::ArithmeticOverflow)?).ok_or_else(|| Error::OutOfSpace)?
        };

        entry.header.entry_size.set(new_entry_size);
        let buf = &mut self.buf[offset..];
        buf.copy_within(old_entry_size as usize..old_used_size, new_entry_size as usize);
        let entry = self.entry_mut(entry_id, instance_id, board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
        Ok(entry)
    }
    /// Inserts the given token.
    /// Precondition: Caller already resized the group.
    pub(crate) fn insert_token(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  This is bad when iterating over the group members until the end--it will iterate over garbage.
        // Therefore, mask the new area out for the iterator--and reinstate it only after resize_entry_by (which has been adapted specially) is finished with Ok.
        let mut entry = self.resize_entry_by(entry_id, instance_id, board_instance_mask, (token_size as i64).into())?;
        entry.insert_token(token_id, token_value)
    }

    /// Deletes the given token.
    /// Returns the number of bytes that were deleted.
    /// Postcondition: Caller will resize the given group
    pub(crate) fn delete_token(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, token_id: u32) -> Result<i64> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  This is bad when iterating over the group members until the end--it will iterate over garbage.
        // Therefore, mask the new area out for the iterator--and reinstate it only after resize_entry_by (which has been adapted specially) is finished with Ok.
        let mut entry = self.entry_mut(entry_id, instance_id, board_instance_mask).ok_or_else(|| Error::EntryNotFound)?;
        entry.delete_token(token_id)?;
        let mut token_size_diff: i64 = token_size.try_into().map_err(|_| Error::ArithmeticOverflow)?;
        token_size_diff = -token_size_diff;
        self.resize_entry_by(entry_id, instance_id, board_instance_mask, token_size_diff)?;
        Ok(token_size_diff)
    }

    pub fn entries(&self) -> GroupIter<'_> {
        GroupIter {
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }

    pub fn entries_mut(&mut self) -> GroupMutIter<'_> {
        GroupMutIter {
            header: self.header,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}

impl<'a> Iterator for GroupMutIter<'a> {
    type Item = EntryMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                assert!(e.header.group_id.get() == self.header.group_id.get());
                let entry_size = e.header.entry_size.get() as usize;
                assert!(self.remaining_used_size >= entry_size);
                self.remaining_used_size -= entry_size;
                Some(e)
            },
            Err(_) => {
                None
            },
        }
    }
}
