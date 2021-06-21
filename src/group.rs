use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::GROUP_HEADER;
use crate::ondisk::TYPE_ALIGNMENT;
use crate::ondisk::TYPE_HEADER;
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
    pub(crate) remaining_used_size: usize, // FIXME: private
}

impl<'a> Iterator for GroupItem<'a> {
    type Item = EntryItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                assert!(e.header.group_id.get() == self.header.group_id.get());
                let entry_size = e.header.type_size.get() as usize;
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

impl GroupItem<'_> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> u16 {
        self.header.group_id.get()
    }

    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'a>(buf: &mut ReadOnlyBuffer<'a>) -> Result<EntryItem<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Entry", ""));
        }
        let header = match take_header_from_collection::<TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        let payload_size = type_size.checked_sub(size_of::<TYPE_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Entry", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        let unit_size = header.unit_size;
        let type_id = header.type_id.get();
        Ok(EntryItem {
            header: header,
            body: EntryItemBody::<ReadOnlyBuffer>::from_slice(unit_size, type_id, context_type, body)?,
        })
    }

    /// Side effect: Moves the iterator!
    pub fn entry(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Option<EntryItem> {
        for entry in self {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() == board_instance_mask {
                return Some(entry);
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct GroupMutItem<'a> {
    pub header: &'a mut GROUP_HEADER,
    pub(crate) buf: Buffer<'a>, // FIXME: private
    pub(crate) remaining_used_size: usize, // FIXME: private
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

    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut Buffer<'b>) -> Result<EntryMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Entry", ""));
        }
        let header = match take_header_from_collection_mut::<TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        let payload_size = type_size.checked_sub(size_of::<TYPE_HEADER>()).ok_or_else(|| Error::FileSystemError("unexpected EOF while locating body of Entry", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        let unit_size = header.unit_size;
        let type_id = header.type_id.get();
        Ok(EntryMutItem {
            header: header,
            body: EntryItemBody::<Buffer>::from_slice(unit_size, type_id, context_type, body)?,
        })
    }

    /// Finds the first EntryMutItem with the given id, if any, and returns it.
    pub fn entry_mut(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Option<EntryMutItem> {
        for entry in self {
            if entry.id() == id && entry.instance_id() == instance_id && entry.board_instance_mask() == board_instance_mask {
                return Some(entry);
            }
        }
        None
    }
    pub(crate) fn delete_entry(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Result<u32> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            let entry = Self::next_item(&mut buf)?;

            if entry.header.type_id.get() == id && entry.header.instance_id.get() == instance_id && entry.header.board_instance_mask.get() == board_instance_mask {
                let type_size = entry.header.type_size.get();
                self.buf.copy_within((type_size as usize)..self.remaining_used_size, 0);

                return Ok(type_size as u32);
            } else {
                let entry = Self::next_item(&mut self.buf)?;
                let entry_size = entry.header.type_size.get() as usize;
                self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "TYPE_HEADER::type_size"))?;
            }
        }
        Ok(0u32)
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
                        let entry = Self::next_item(&mut self.buf).unwrap();
                        let entry_size = entry.header.type_size.get() as usize;
                        assert!(self.remaining_used_size >= entry_size);
                        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", ""))?;
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
    /// Precondition: Caller already increased the group size by entry_size.
    pub(crate) fn insert_entry(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, entry_size: u16, context_type: ContextType, payload_size: u16, priority_mask: u8) -> Result<EntryMutItem<'a>> {
        let remaining_used_size = self.remaining_used_size;
        // Make sure that move_insertion_point_before does not notice the new uninitialized entry
        self.remaining_used_size = remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "TYPE_HEADER::entry_size"))?;
        self.move_insertion_point_before(group_id, id, instance_id, board_instance_mask)?;

        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        self.buf.copy_within(0..self.remaining_used_size, entry_size as usize);

        let header = take_header_from_collection_mut::<TYPE_HEADER>(&mut self.buf).ok_or_else(|| Error::FileSystemError("could not read header of Entry", ""))?;
        *header = TYPE_HEADER::default();
        header.group_id.set(group_id);
        header.type_id.set(id);
        header.type_size.set(entry_size);
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
        let body = take_body_from_collection_mut(&mut self.buf, payload_size.into(), TYPE_ALIGNMENT).ok_or_else(|| Error::FileSystemError("could not read body of Entry", ""))?;
        self.remaining_used_size = remaining_used_size;
        Ok(EntryMutItem { header, body: EntryItemBody::<Buffer>::from_slice(unit_size, id, context_type, body)? })
    }
    /// Resizes the given entry by SIZE_DIFF.
    /// Precondition: If SIZE_DIFF > 0, caller needs to have expanded the group by SIZE_DIFF already.
    pub(crate) fn resize_entry_by(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, size_diff: i64) -> Result<EntryMutItem<'a>> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                return Err(Error::EntryNotFoundError);
            }
            let entry = Self::next_item(&mut buf)?;
            if entry.header.group_id.get() == group_id && entry.header.type_id.get() == id && entry.header.instance_id.get() == instance_id && entry.header.board_instance_mask.get() == board_instance_mask {
                let type_size = entry.header.type_size.get();
                let new_type_size = if size_diff > 0 {
                    let size_diff = size_diff as u64;
                    type_size.checked_add(size_diff.try_into().unwrap()).ok_or_else(|| Error::OutOfSpaceError)?
                } else {
                    let size_diff = (-size_diff) as u64;
                    type_size.checked_sub(size_diff.try_into().unwrap()).ok_or_else(|| Error::OutOfSpaceError)?
                };
                if size_diff > 0 {
                    // Increase used size
                    let remaining_used_size = self.remaining_used_size.checked_add(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "TYPE_HEADER::entry_size"))?;

                    if self.buf.len() >= remaining_used_size {
                    } else {
                        return Err(Error::OutOfSpaceError);
                    }
                    self.buf.copy_within((type_size as usize)..self.remaining_used_size, new_type_size as usize);
                    self.remaining_used_size = remaining_used_size;
                } else if size_diff < 0 {
                    let size_diff = -size_diff;
                    self.buf.copy_within((type_size as usize)..self.remaining_used_size, new_type_size as usize);
                    // Decrease used size
                    self.remaining_used_size = self.remaining_used_size.checked_sub(size_diff as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "TYPE_HEADER::entry_size"))?;
                }
                let header = take_header_from_collection_mut::<TYPE_HEADER>(&mut self.buf).ok_or_else(|| Error::FileSystemError("could not read header of Entry", ""))?;                let unit_size = header.unit_size;
                let payload_size = new_type_size.checked_sub(size_of::<TYPE_HEADER>() as u16).ok_or_else(|| Error::FileSystemError("Entry is smaller than Entry header", "TYPE_HEADER::entry_size"))?;
                let body = take_body_from_collection_mut(&mut self.buf, payload_size.into(), TYPE_ALIGNMENT).ok_or_else(|| Error::FileSystemError("could not read body ff Entry", ""))?;
                return Ok(EntryMutItem { header, body: EntryItemBody::<Buffer>::from_slice(unit_size, id, context_type, body)? });
            } else {
                let entry = Self::next_item(&mut self.buf)?;
                let entry_size = entry.header.type_size.get() as usize;
                self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "TYPE_HEADER::type_size"))?;
            }
        }
    }
    /// Inserts the given token.
    pub(crate) fn insert_token(&mut self, group_id: u16, type_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        let token_size = size_of::<TOKEN_ENTRY>();
        // Note: Now, GroupMutItem.buf includes space for the token, claimed by no entry so far.  This is bad when iterating over the group members until the end--it will iterate over garbage.
        // Therefore, mask the new area out for the iterator--and reinstate it only after resize_entry_by (which has been adapted specially) is finished with Ok.
        self.remaining_used_size = self.remaining_used_size.checked_sub(token_size).ok_or_else(|| Error::FileSystemError("Cannot iterate over old Entries", ""))?;
        let mut entry = self.resize_entry_by(group_id, type_id, instance_id, board_instance_mask, ContextType::Tokens, (token_size as i64).into())?;
        self.remaining_used_size = self.remaining_used_size.checked_add(token_size).ok_or_else(|| Error::FileSystemError("Cannot iterate over old Entries", ""))?;
        entry.insert_token(token_id, token_value)
    }
}

impl<'a> Iterator for GroupMutItem<'a> {
    type Item = EntryMutItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.buf) {
            Ok(e) => {
                assert!(e.header.group_id.get() == self.header.group_id.get());
                let entry_size = e.header.type_size.get() as usize;
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

