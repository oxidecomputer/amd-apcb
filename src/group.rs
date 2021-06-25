use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::ENTRY_ALIGNMENT;
use crate::ondisk::ENTRY_HEADER;
use crate::ondisk::TOKEN_ENTRY;
pub use crate::ondisk::{ContextFormat, ContextType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::convert::TryInto;
use core::mem::{size_of};
use num_traits::FromPrimitive;

use crate::entry::{EntryItem, EntryMutItem, EntryItemBody};

#[derive(Debug)]
pub struct GroupItem<'a> {
    pub header: &'a GROUP_HEADER,
    pub(crate) buf: ReadOnlyBuffer<'a>, // FIXME: private
    pub(crate) used_size: usize, // FIXME: private
}

#[derive(Debug)]
pub struct GroupIter<'a> {
    pub header: &'a GROUP_HEADER,
    pub(crate) buf: ReadOnlyBuffer<'a>, // FIXME: private
    pub(crate) remaining_used_size: usize, // FIXME: private
}

impl<'a> Iterator for GroupIter<'a> {
    type Item = EntryItem<'a>;

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
            Err(e) => {
                None
            },
        }
    }
}
impl GroupIter<'_> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'a>(buf: &mut ReadOnlyBuffer<'a>) -> Result<EntryItem<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Entry", ""));
        }
        let header = match take_header_from_collection::<ENTRY_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size.checked_sub(size_of::<ENTRY_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Entry", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, ENTRY_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        let unit_size = header.unit_size;
        let entry_id = header.entry_id.get();
        Ok(EntryItem {
            header: header,
            body: EntryItemBody::<ReadOnlyBuffer<'_>>::from_slice(unit_size, entry_id, context_type, body)?,
        })
    }

}

impl GroupItem<'_> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> u16 {
        self.header.group_id.get()
    }

    /// Side effect: Moves the iterator!
    pub fn entry(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Option<EntryItem<'_>> {
        for entry in self.entries() {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() == board_instance_mask {
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
    pub header: &'a mut GROUP_HEADER,
    pub(crate) buf: Buffer<'a>, // FIXME: private
    pub(crate) remaining_used_size: usize, // FIXME: private
}

impl<'a> GroupMutIter<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut Buffer<'b>) -> Result<EntryMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Entry", ""));
        }
        let header = match take_header_from_collection_mut::<ENTRY_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let entry_size = header.entry_size.get() as usize;

        let payload_size = entry_size.checked_sub(size_of::<ENTRY_HEADER>()).ok_or_else(|| Error::FileSystemError("unexpected EOF while locating body of Entry", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, ENTRY_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        let unit_size = header.unit_size;
        let entry_id = header.entry_id.get();
        Ok(EntryMutItem {
            header: header,
            body: EntryItemBody::<Buffer<'_>>::from_slice(unit_size, entry_id, context_type, body)?,
        })
    }

    /// Find the place BEFORE which the entry (GROUP_ID, ID, INSTANCE_ID, BOARD_INSTANCE_MASK) is supposed to go.
    pub(crate) fn move_insertion_point_before(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            match Self::next_item(&mut buf) {
                Ok(e) => {
                    if (e.header.group_id.get(), e.id(), e.instance_id(), e.board_instance_mask()) < (group_id, id, instance_id, board_instance_mask) {
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
    /// Inserts the given entry data at the right spot.
    /// Precondition: Caller already increased the group size by entry_allocation.
    pub(crate) fn insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, entry_allocation: u16, context_type: ContextType, payload: &[u8], priority_mask: u8) -> Result<()> {
        let payload_size = payload.len();
        // Make sure that move_insertion_point_before does not notice the new uninitialized entry
        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_allocation as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "ENTRY_HEADER::entry_size"))?;
        self.move_insertion_point_before(group_id, entry_id, instance_id, board_instance_mask)?;

        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        self.buf.copy_within(0..self.remaining_used_size, entry_allocation as usize);

        let mut buf = &mut self.buf[..(self.remaining_used_size + entry_allocation as usize)];
        let header = take_header_from_collection_mut::<ENTRY_HEADER>(&mut buf).ok_or_else(|| Error::FileSystemError("could not read header of Entry", ""))?;
        *header = ENTRY_HEADER::default();
        header.group_id.set(group_id);
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
        let unit_size = header.unit_size;

        // Note: The following is settable by the user via EntryMutItem set-accessors: context_type, context_format, unit_size, priority_mask, key_size, key_pos
        header.board_instance_mask.set(board_instance_mask);
        let body = take_body_from_collection_mut(&mut buf, payload_size.into(), ENTRY_ALIGNMENT).ok_or_else(|| Error::FileSystemError("could not read body of Entry", ""))?;
        body.copy_from_slice(payload);
        self.remaining_used_size = self.remaining_used_size.checked_add(entry_allocation as usize).ok_or_else(|| Error::OutOfSpaceError)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct GroupMutItem<'a> {
    pub header: &'a mut GROUP_HEADER,
    pub(crate) buf: Buffer<'a>, // FIXME: private
    pub(crate) used_size: usize, // FIXME: private
}

impl<'a> GroupMutItem<'a> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> u16 {
        self.header.group_id.get()
    }

    /// Finds the first EntryMutItem with the given id, if any, and returns it.
    pub fn entry_mut(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Option<EntryMutItem<'_>> {
        for entry in self.entries_mut() {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() == board_instance_mask {
                return Some(entry);
            }
        }
        None
    }
    pub(crate) fn delete_entry(&mut self, entry_id: u16, instance_id: u16, board_instance_mask: u16) -> Result<u32> {
        let mut remaining_used_size = self.used_size;
        let mut offset = 0usize;
        let mut buf = &mut self.buf[..remaining_used_size];
        loop {
            if buf.len() == 0 {
                break;
            }
            let entry = GroupMutIter::next_item(&mut buf)?;

            let entry_size = entry.header.entry_size.get() as usize;
            if entry.header.entry_id.get() == entry_id && entry.header.instance_id.get() == instance_id && entry.header.board_instance_mask.get() == board_instance_mask {
                self.buf.copy_within(offset.checked_add(entry_size).unwrap()..remaining_used_size, offset);
                return Ok(entry_size as u32);
            } else {
                offset = offset.checked_add(entry_size).unwrap();
                remaining_used_size = remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "ENTRY_HEADER::entry_size"))?;
            }
        }
        Ok(0u32)
    }
    /// Resizes the given entry by SIZE_DIFF.
    /// Precondition: If SIZE_DIFF > 0, caller needs to have expanded the group by SIZE_DIFF already.
    pub(crate) fn resize_entry_by(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, size_diff: i64) -> Result<EntryMutItem<'_>> {
        let mut remaining_used_size = self.used_size;
        let mut offset = 0usize;
        let mut buf = &mut self.buf[..remaining_used_size];
        loop {
            if buf.len() == 0 {
                break;
            }
            let entry = GroupMutIter::next_item(&mut buf)?;

            let entry_size = entry.header.entry_size.get();
            if entry.header.entry_id.get() == entry_id && entry.header.instance_id.get() == instance_id && entry.header.board_instance_mask.get() == board_instance_mask {
                let old_used_size = self.used_size;
                let old_entry_size = entry_size;
                let new_entry_size: u16 = if size_diff > 0 {
                    let size_diff = size_diff as u64;
                    old_entry_size.checked_add(size_diff.try_into().unwrap()).ok_or_else(|| Error::OutOfSpaceError)?
                } else {
                    let size_diff = (-size_diff) as u64;
                    old_entry_size.checked_sub(size_diff.try_into().unwrap()).ok_or_else(|| Error::OutOfSpaceError)?
                };
                let new_used_size = if size_diff > 0 {
                    old_used_size.checked_add(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "ENTRY_HEADER::entry_size"))?
                } else {
                    old_used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "ENTRY_HEADER::entry_size"))?
                };
                entry.header.entry_size.set(new_entry_size);
                if size_diff > 0 {
                    // Increase used size
                    if self.buf.len() >= new_used_size {
                    } else {
                        return Err(Error::OutOfSpaceError);
                    }
                }
                self.buf.copy_within(offset.checked_add(old_entry_size as usize).unwrap()..old_used_size, offset.checked_add(new_entry_size as usize).unwrap());
                self.used_size = new_used_size;
                let entry = self.entry_mut(entry_id, instance_id, board_instance_mask).ok_or_else(|| Error::EntryNotFoundError)?;
                return Ok(entry)
            } else {
                offset = offset.checked_add(entry_size as usize).unwrap();
                remaining_used_size = remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "ENTRY_HEADER::entry_size"))?;
            }
        }
        Err(Error::EntryNotFoundError)
    }
    /// Inserts the given token.
    pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  This is bad when iterating over the group members until the end--it will iterate over garbage.
        // Therefore, mask the new area out for the iterator--and reinstate it only after resize_entry_by (which has been adapted specially) is finished with Ok.
        let mut entry = self.resize_entry_by(group_id, entry_id, instance_id, board_instance_mask, ContextType::Tokens, (token_size as i64).into())?;
        entry.insert_token(token_id, token_value)
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
            Err(e) => {
                None
            },
        }
    }
}
