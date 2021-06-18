use crate::types::{Error, Result, Buffer, ReadOnlyBuffer};

use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_ALIGNMENT;
use crate::ondisk::APCB_TYPE_HEADER;
pub use crate::ondisk::{ContextFormat, ContextType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::mem::{size_of};
use num_traits::FromPrimitive;

use crate::entry::{EntryItem, EntryMutItem, EntryItemBody};

#[derive(Debug)]
pub struct GroupItem<'a> {
    pub header: &'a APCB_GROUP_HEADER,
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
        let header = match take_header_from_collection::<APCB_TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        let payload_size = type_size.checked_sub(size_of::<APCB_TYPE_HEADER>()).ok_or_else(|| Error::FileSystemError("could not locate body of Entry", ""))?;
        let body = match take_body_from_collection(&mut *buf, payload_size, APCB_TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        Ok(EntryItem {
            header: header,
            body: EntryItemBody::<ReadOnlyBuffer>::from_slice(context_type, body),
        })
    }

/* TODO:
    /// Finds the first EntryMutItem with the given id, if any, and returns it.
    pub fn first_entry(&self, id: u16) -> Option<EntryItem> {
        for entry in self {
            if entry.id() == id {
                return Some(entry);
            }
        }
        None
    }
*/
}

#[derive(Debug)]
pub struct GroupMutItem<'a> {
    pub header: &'a mut APCB_GROUP_HEADER,
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
        let header = match take_header_from_collection_mut::<APCB_TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Entry", ""));
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        let context_type = ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        let payload_size = type_size.checked_sub(size_of::<APCB_TYPE_HEADER>()).ok_or_else(|| Error::FileSystemError("unexpected EOF while locating body of Entry", ""))?;
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, APCB_TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read body of Entry", ""));
            },
        };

        Ok(EntryMutItem {
            header: header,
            body: EntryItemBody::<Buffer>::from_slice(context_type, body),
        })
    }

    /// Finds the first EntryMutItem with the given id, if any, and returns it.
    pub fn first_entry_mut(&mut self, id: u16) -> Option<EntryMutItem> {
        for entry in self {
            if entry.id() == id {
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
                self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining Iterator size", "APCB_TYPE_HEADER::type_size"))?;
            }
        }
        Ok(0u32)
    }
    /// First the place BEFORE which the entry (GROUP_ID, ID, INSTANCE_ID, BOARD_INSTANCE_MASK) is supposed to go.
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
    pub(crate) fn insert_entry(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, entry_size: u16, context_type: ContextType, payload_size: u16) -> Result<EntryMutItem<'a>> {
        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining iterator size", "APCB_TYPE_HEADER::entry_size"))?;
        self.move_insertion_point_before(group_id, id, instance_id, board_instance_mask)?;

        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        self.buf.copy_within(0..self.remaining_used_size, entry_size as usize);

        self.remaining_used_size = self.remaining_used_size.checked_add(entry_size as usize).ok_or_else(|| Error::OutOfSpaceError)?;
        let header = take_header_from_collection_mut::<APCB_TYPE_HEADER>(&mut self.buf).ok_or_else(|| Error::FileSystemError("could not read header of Entry", ""))?;
        *header = APCB_TYPE_HEADER::default();
        header.group_id.set(group_id);
        header.type_id.set(id);
        header.type_size.set(entry_size);
        header.instance_id.set(instance_id);
        header.context_type = context_type as u8;
        header.context_format = match context_type {
            ContextType::Struct => ContextFormat::Raw,
            ContextType::Parameters => ContextFormat::Raw, // TODO: verify
            ContextType::Tokens => ContextFormat::SortAscending,
        } as u8;

        // Note: The following is settable by the user via EntryMutItem set-accessors: context_type, context_format, unit_size, priority_mask, key_size, key_pos
        header.board_instance_mask.set(board_instance_mask);
        let body = take_body_from_collection_mut(&mut self.buf, payload_size.into(), APCB_TYPE_ALIGNMENT).ok_or_else(|| Error::FileSystemError("could not read body of Entry", ""))?;
        Ok(EntryMutItem { header, body: EntryItemBody::<Buffer>::from_slice(context_type, body) })
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

