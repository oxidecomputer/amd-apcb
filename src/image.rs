use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_ALIGNMENT;
use crate::ondisk::APCB_TYPE_HEADER;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
pub use crate::ondisk::{ContextFormat, ContextType, take_header_from_collection, take_header_from_collection_mut, take_body_from_collection, take_body_from_collection_mut};
use core::mem::{size_of};
use num_traits::FromPrimitive;

type
    Buffer<'a> = &'a mut [u8];

type
    ReadOnlyBuffer<'a> = &'a [u8];

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

#[derive(Debug)]
pub enum Error {
    MarshalError,
    OutOfSpaceError,
    GroupNotFoundError,
}

type Result<Q> = core::result::Result<Q, Error>;

#[derive(Debug)]
pub struct Entry<'a> {
    pub header: &'a mut APCB_TYPE_HEADER,
    body: Buffer<'a>,
}

impl Entry<'_> {
    // pub fn group_id(&self) -> u16  ; suppressed--replaced by an assert on read.
    pub fn id(&self) -> u16 {
        self.header.type_id.get()
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        FromPrimitive::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        FromPrimitive::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> u8 {
        self.header.priority_mask
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <= unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> u16 {
        self.header.board_instance_mask.get()
    }

    /* Not seen in the wild anymore.
        /// If the value is a Parameter, returns its time point
        pub fn parameter_time_point(&self) -> u8 {
            assert!(self.context_type() == ContextType::Parameter);
            self.body[0]
        }

        /// If the value is a Parameter, returns its token
        pub fn parameter_token(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            value & 0x1FFF
        }

        // If the value is a Parameter, returns its size
        pub fn parameter_size(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            (value >> 13) + 1
        }
    */

    pub fn set_context_type(&mut self, value: ContextType) -> &mut Self {
        self.header.context_type = value as u8;
        self
    }

    pub fn set_context_format(&mut self, value: ContextFormat) -> &mut Self {
        self.header.context_format = value as u8;
        self
    }

    pub fn set_unit_size(&mut self, value: u8) -> &mut Self {
        self.header.unit_size = value;
        self
    }

    pub fn set_priority_mask(&mut self, value: u8) -> &mut Self {
        self.header.priority_mask = value;
        self
    }

    /// Note: Applicable iff context_format() != ContextFormat::Raw.  value <= unit_size.
    pub fn set_key_size(&mut self, value: u8) -> &mut Self {
        self.header.key_size = value;
        self
    }

    pub fn set_key_pos(&mut self, value: u8) -> &mut Self {
        self.header.key_pos = value;
        self
    }

    // Note: Because type_id, instance_id, group_id and board_instance_mask are sort keys, these cannot be mutated.
}

#[derive(Debug)]
pub struct EntryItem<'a> {
    pub header: &'a APCB_TYPE_HEADER,
    body: ReadOnlyBuffer<'a>,
}

impl EntryItem<'_> {
    // pub fn group_id(&self) -> u16  ; suppressed--replaced by an assert on read.
    pub fn id(&self) -> u16 {
        self.header.type_id.get()
    }
    pub fn instance_id(&self) -> u16 {
        self.header.instance_id.get()
    }
    pub fn context_type(&self) -> ContextType {
        FromPrimitive::from_u8(self.header.context_type).unwrap()
    }
    pub fn context_format(&self) -> ContextFormat {
        FromPrimitive::from_u8(self.header.context_format).unwrap()
    }
    /// Note: Applicable iff context_type() == 2.  Usual value then: 8.  If inapplicable, value is 0.
    pub fn unit_size(&self) -> u8 {
        self.header.unit_size
    }
    pub fn priority_mask(&self) -> u8 {
        self.header.priority_mask
    }
    /// Note: Applicable iff context_format() != ContextFormat::Raw. Result <= unit_size.
    pub fn key_size(&self) -> u8 {
        self.header.key_size
    }
    pub fn key_pos(&self) -> u8 {
        self.header.key_pos
    }
    pub fn board_instance_mask(&self) -> u16 {
        self.header.board_instance_mask.get()
    }

    /* Not seen in the wild anymore.
        /// If the value is a Parameter, returns its time point
        pub fn parameter_time_point(&self) -> u8 {
            assert!(self.context_type() == ContextType::Parameter);
            self.body[0]
        }

        /// If the value is a Parameter, returns its token
        pub fn parameter_token(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            value & 0x1FFF
        }

        // If the value is a Parameter, returns its size
        pub fn parameter_size(&self) -> u16 {
            assert!(self.context_type() == ContextType::Parameter);
            let value = self.body[1] as u16 | ((self.body[2] as u16) << 8);
            (value >> 13) + 1
        }
    */
}

#[derive(Debug)]
pub struct GroupItem<'a> {
    pub header: &'a APCB_GROUP_HEADER,
    buf: ReadOnlyBuffer<'a>,
    remaining_used_size: usize,
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
            return Err(Error::MarshalError);
        }
        let header = match take_header_from_collection::<APCB_TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        assert!(type_size >= size_of::<APCB_TYPE_HEADER>());
        let payload_size = type_size - size_of::<APCB_TYPE_HEADER>();
        let body = match take_body_from_collection(&mut *buf, payload_size, APCB_TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
            },
        };

        Ok(EntryItem {
            header: header,
            body: body,
        })
    }

/* TODO:
    /// Finds the first Entry with the given id, if any, and returns it.
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
    buf: Buffer<'a>,
    remaining_used_size: usize,
}

impl GroupMutItem<'_> {
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
    fn next_item<'a>(buf: &mut Buffer<'a>) -> Result<Entry<'a>> {
        if buf.len() == 0 {
            return Err(Error::MarshalError);
        }
        let header = match take_header_from_collection_mut::<APCB_TYPE_HEADER>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
            }
        };
        ContextFormat::from_u8(header.context_format).unwrap();
        ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        assert!(type_size >= size_of::<APCB_TYPE_HEADER>());
        let payload_size = type_size - size_of::<APCB_TYPE_HEADER>();
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, APCB_TYPE_ALIGNMENT) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
            },
        };

        Ok(Entry {
            header: header,
            body: body,
        })
    }

    /// Finds the first Entry with the given id, if any, and returns it.
    pub fn first_entry_mut(&mut self, id: u16) -> Option<Entry> {
        for entry in self {
            if entry.id() == id {
                return Some(entry);
            }
        }
        None
    }
    fn delete_entry(&mut self, id: u16, instance_id: u16, board_instance_mask: u16) -> Result<u32> {
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
                assert!(self.remaining_used_size >= entry_size);
                self.remaining_used_size -= entry_size;
            }
        }
        Ok(0u32)
    }
    /// First the place BEFORE which the entry (GROUP_ID, ID, INSTANCE_ID, BOARD_INSTANCE_MASK) is supposed to go.
    fn move_insertion_point_before(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16) -> Result<()> {
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
                        self.remaining_used_size -= entry_size;
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
}

impl<'a> Iterator for GroupMutItem<'a> {
    type Item = Entry<'a>;

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

impl<'a> ApcbIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.beginning_of_groups.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(buf: &mut Buffer<'b>) -> Result<GroupMutItem<'b>> {
        if buf.len() == 0 {
            return Err(Error::MarshalError);
        }
        let header = match take_header_from_collection_mut::<APCB_GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::MarshalError);
             },
        };
        let group_size = header.group_size.get() as usize;
        assert!(group_size >= size_of::<APCB_GROUP_HEADER>());
        let payload_size = group_size - size_of::<APCB_GROUP_HEADER>();
        let body = match take_body_from_collection_mut(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
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
                    let new_group_size = old_group_size.checked_sub(entry_size).ok_or_else(|| Error::MarshalError)?;
                    group.header.group_size.set(new_group_size);

                    let apcb_size = self.header.apcb_size.get();
                    self.beginning_of_groups.copy_within((old_group_size as usize)..(apcb_size as usize), new_group_size as usize);
                    self.header.apcb_size.set(apcb_size.checked_sub(entry_size as u32).ok_or_else(|| Error::MarshalError)?);

                    self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::MarshalError)?;
                }
                break 'outer;
            }
            let group = Self::next_item(&mut self.beginning_of_groups)?;
            let group_size = group.header.group_size.get() as usize;
            assert!(self.remaining_used_size >= group_size);
            self.remaining_used_size -= group_size;
        }
        Ok(())
    }
    pub fn insert_entry(&mut self, group_id: u16, id: u16, instance_id: u16, board_instance_mask: u16, payload_size: u16) -> Result<Entry> {
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
                group.remaining_used_size = group.remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::MarshalError)?;
                group.move_insertion_point_before(group_id, id, instance_id, board_instance_mask)?;

                // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
                group.buf.copy_within(0..group.remaining_used_size, entry_size as usize);

                group.remaining_used_size = group.remaining_used_size.checked_add(entry_size as usize).ok_or_else(|| Error::OutOfSpaceError)?;

                let header = take_header_from_collection_mut::<APCB_TYPE_HEADER>(&mut group.buf).ok_or_else(|| Error::MarshalError)?;
                *header = APCB_TYPE_HEADER::default();
                header.group_id.set(group_id);
                header.type_id.set(id);
                header.type_size.set(entry_size);
                header.instance_id.set(instance_id);
                // Note: The following is settable by the user via Entry set-accessors: context_type, context_format, unit_size, priority_mask, key_size, key_pos
                header.board_instance_mask.set(board_instance_mask);
                let body = take_body_from_collection_mut(&mut group.buf, payload_size.into(), APCB_TYPE_ALIGNMENT).ok_or_else(|| Error::MarshalError)?;
                return Ok(Entry { header, body });
            }
            let group = Self::next_item(&mut self.beginning_of_groups)?;
            let group_size = group.header.group_size.get() as usize;
            assert!(self.remaining_used_size >= group_size);
            self.remaining_used_size -= group_size;
        }
        Err(Error::GroupNotFoundError)
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
            return Err(Error::MarshalError);
        }
        let header = match take_header_from_collection::<APCB_GROUP_HEADER>(&mut *buf) {
             Some(item) => item,
             None => {
                 return Err(Error::MarshalError);
             },
        };
        let group_size = header.group_size.get() as usize;
        assert!(group_size >= size_of::<APCB_GROUP_HEADER>());
        let payload_size = group_size - size_of::<APCB_GROUP_HEADER>();
        let body = match take_body_from_collection(&mut *buf, payload_size, 1) {
            Some(item) => item,
            None => {
                return Err(Error::MarshalError);
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
    pub fn groups(&mut self) -> ApcbIter {
        ApcbIter {
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            beginning_of_groups: self.beginning_of_groups,
            remaining_used_size: self.remaining_used_size,
        }
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
        assert!(self.beginning_of_groups.len() >= self.remaining_used_size);

        let mut beginning_of_group = &mut self.beginning_of_groups[remaining_used_size..self.remaining_used_size];

        let mut header = take_header_from_collection_mut::<APCB_GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::MarshalError)?;
        *header = APCB_GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.into();
        let body = take_body_from_collection_mut(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::MarshalError)?;
        let body_len = body.len();

        Ok(GroupMutItem {
            header: header,
            buf: body,
            remaining_used_size: body_len,
        })
    }
    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = ApcbIterMut::next_item(&mut beginning_of_groups)?;
            if group.header.group_id.get() == group_id {
                let group_size = group.header.group_size.get();

                let apcb_size = self.header.apcb_size.get();
                assert!(apcb_size >= group_size);
                self.beginning_of_groups.copy_within((group_size as usize)..(apcb_size as usize), 0);
                self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::MarshalError)?);

                self.remaining_used_size = self.remaining_used_size.checked_sub(group_size as usize).ok_or_else(|| Error::MarshalError)?;
                break;
            } else {
                let group = ApcbIterMut::next_item(&mut self.beginning_of_groups)?;
                let group_size = group.header.group_size.get() as usize;
                assert!(self.remaining_used_size >= group_size);
                self.remaining_used_size -= group_size;
            }
        }
        Ok(())
    }
    pub fn load(backing_store: Buffer<'a>) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection_mut::<APCB_V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::MarshalError)?;

        assert!(usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>());
        assert!(header.version.get() == 0x30);

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()
        {
            let value = take_header_from_collection_mut::<APCB_V3_HEADER_EXT>(&mut backing_store)
                .ok_or_else(|| Error::MarshalError)?;
            assert!(value.signature == *b"ECB2");
            assert!(value.struct_version.get() == 0x12);
            assert!(value.data_version.get() == 0x100);
            assert!(value.ext_header_size.get() == 96);
            assert!(u32::from(value.data_offset.get()) == 88);
            assert!(value.signature_ending == *b"BCBA");
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        assert!(header.apcb_size.get() >= header.header_size.get().into());
        let remaining_used_size = (header.apcb_size.get() - u32::from(header.header_size)) as usize;
        assert!(backing_store.len() >= remaining_used_size);

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
                    .ok_or_else(|| Error::MarshalError)?;
            *header = Default::default();

            let v3_header_ext = take_header_from_collection_mut::<APCB_V3_HEADER_EXT>(&mut backing_store)
                    .ok_or_else(|| Error::MarshalError)?;
            *v3_header_ext = Default::default();

            header
                .header_size
                .set((size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()) as u16);
            header.apcb_size = (header.header_size.get() as u32).into();
        }

        Self::load(backing_store)
    }
}

#[cfg(test)]
mod tests {
    use super::APCB;
    use super::Error;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        APCB::load(&mut buffer[0..]).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        let groups = apcb.groups();
        let mut count = 0;
        for _item in groups {
            count += 1;
        }
        assert!(count == 1);
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() == *b"PSPG");
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() == *b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_first_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1701)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1704)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x4711)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.delete_group(0x1701)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _group in groups {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn delete_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 96, 0, 0xFFFF, 48)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.delete_entry(0x1701, 96, 0, 0xFFFF)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        for _entry in group {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn insert_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 96, 0, 0xFFFF, 48)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 97, 0, 0xFFFF, 1)?;

        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let mut group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let entry = group.next().ok_or_else(|| Error::MarshalError)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = group.next().ok_or_else(|| Error::MarshalError)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(group.next(), None));

        let mut group = groups.next().ok_or_else(|| Error::MarshalError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }
}
