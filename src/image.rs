use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_ALIGNMENT;
use crate::ondisk::APCB_TYPE_HEADER;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
pub use crate::ondisk::{ContextFormat, ContextType, take_header_from_collection, take_body_from_collection};
use core::mem::{size_of};
use num_traits::FromPrimitive;

pub struct APCB<'a> {
    header: &'a mut APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: &'a mut [u8],
    remaining_used_size: usize,
}

#[derive(Debug)]
pub enum Error {
    MarshalError,
    OutOfSpaceError,
}

type Result<Q> = core::result::Result<Q, Error>;

#[derive(Debug)]
pub struct Entry<'a> {
    pub header: &'a mut APCB_TYPE_HEADER,
    body: &'a mut [u8],
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
    /// Note: Applicable iff context_format() != 0. Result <= unit_size.
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
pub struct Group<'a> {
    pub header: &'a mut APCB_GROUP_HEADER,
    buf: &'a mut [u8],
}

impl Group<'_> {
    /// Note: ASCII
    pub fn signature(&self) -> [u8; 4] {
        self.header.signature
    }
    /// Note: See ondisk::GroupId
    pub fn id(&self) -> u16 {
        self.header.group_id.get()
    }

    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    fn next_item<'a>(buf: &mut &'a mut [u8]) -> Option<Entry<'a>> {
        if buf.len() == 0 {
            return None;
        }
        let header = take_header_from_collection::<APCB_TYPE_HEADER>(&mut *buf)?;
        ContextFormat::from_u8(header.context_format).unwrap();
        ContextType::from_u8(header.context_type).unwrap();
        let type_size = header.type_size.get() as usize;

        assert!(type_size >= size_of::<APCB_TYPE_HEADER>());
        let payload_size = type_size - size_of::<APCB_TYPE_HEADER>();
        let body = take_body_from_collection(&mut *buf, payload_size, APCB_TYPE_ALIGNMENT)?;

        Some(Entry {
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
    fn delete_entry(&mut self, id: u16) -> Result<u32> {
        loop {
            let mut buf = &mut self.buf[..];
            if buf.len() == 0 {
                break;
            }
            let entry = Self::next_item(&mut buf).ok_or_else(|| Error::MarshalError)?;

            if entry.header.type_id.get() == id {
                let type_size = entry.header.type_size.get();
                self.buf.copy_within((type_size as usize)..self.buf.len(), 0);

                //self.remaining_used_size = self.remaining_used_size.checked_sub(group_size as usize).ok_or_else(|| Error::MarshalError)?;
                return Ok(type_size as u32);
            } else {
                Self::next_item(&mut self.buf);
            }
        }
        Ok(0u32)
    }
}

impl<'a> Iterator for Group<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match Self::next_item(&mut self.buf) {
            Some(e) => {
                assert!(e.header.group_id.get() == self.header.group_id.get());
                Some(e)
            },
            None => None,
        }
    }
}

impl<'a> Iterator for APCB<'a> {
    type Item = Group<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(&mut self.beginning_of_groups) {
            Some(e) => {
                let group_size = e.header.group_size.get() as usize;
                self.remaining_used_size -= group_size;
                Some(e)
            },
            None => None,
        }
    }
}

impl<'a> APCB<'a> {
    /// It's useful to have some way of NOT mutating self.beginning_of_groups.  This is what this function does.
    fn next_item<'b>(buf: &mut &'b mut [u8]) -> Option<Group<'b>> {
        if buf.len() == 0 {
            return None;
        }
        let header = take_header_from_collection::<APCB_GROUP_HEADER>(&mut *buf)?;
        let group_size = header.group_size.get() as usize;
        assert!(group_size >= size_of::<APCB_GROUP_HEADER>());
        let payload_size = group_size - size_of::<APCB_GROUP_HEADER>();
        let body = take_body_from_collection(&mut *buf, payload_size, 1)?;

        Some(Group {
            header: header,
            buf: body,
        })
    }
    pub fn group_mut(&mut self, group_id: u16) -> Option<Group> {
        for group in self {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
    }
    pub fn insert_group(&mut self, group_id: u16, signature: [u8; 4]) -> Result<Group> {
        // TODO: insert sorted.
        let size = size_of::<APCB_GROUP_HEADER>();
        let apcb_size = self.header.apcb_size.get();
        self.header.apcb_size.set(apcb_size.checked_add(size as u32).ok_or_else(|| Error::OutOfSpaceError)?);
        let remaining_used_size = self.remaining_used_size;
        self.remaining_used_size = remaining_used_size.checked_add(size).ok_or_else(|| Error::OutOfSpaceError)?;
        assert!(self.beginning_of_groups.len() >= self.remaining_used_size);

        let mut beginning_of_group = &mut self.beginning_of_groups[remaining_used_size..(remaining_used_size + size)];

        let mut header = take_header_from_collection::<APCB_GROUP_HEADER>(&mut beginning_of_group).ok_or_else(|| Error::MarshalError)?;
        *header = APCB_GROUP_HEADER::default();
        header.signature = signature;
        header.group_id = group_id.into();
        let body = take_body_from_collection(&mut beginning_of_group, 0, 1).ok_or_else(|| Error::MarshalError)?;

        Ok(Group {
            header: header,
            buf: body,
        })
    }
    pub fn delete_group(&mut self, group_id: u16) -> Result<()> {
        loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = Self::next_item(&mut beginning_of_groups).ok_or_else(|| Error::MarshalError)?;
            if group.header.group_id.get() == group_id {
                let group_size = group.header.group_size.get();

                let apcb_size = self.header.apcb_size.get();
                assert!(apcb_size >= group_size);
                self.beginning_of_groups.copy_within((group_size as usize)..(apcb_size as usize), 0);
                self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::MarshalError)?);

                self.remaining_used_size = self.remaining_used_size.checked_sub(group_size as usize).ok_or_else(|| Error::MarshalError)?;
                break;
            } else {
                let group = Self::next_item(&mut self.beginning_of_groups).ok_or_else(|| Error::MarshalError)?;
                let group_size = group.header.group_size.get() as usize;
                self.remaining_used_size -= group_size;
            }
        }
        Ok(())
    }
    pub fn delete_entry(&mut self, group_id: u16, entry_id: u16) -> Result<()> {
        'outer: loop {
            let mut beginning_of_groups = &mut self.beginning_of_groups[..self.remaining_used_size];
            if beginning_of_groups.len() == 0 {
                break;
            }
            let group = Self::next_item(&mut beginning_of_groups).ok_or_else(|| Error::MarshalError)?;
            if group.header.group_id.get() == group_id {
                let mut group = Self::next_item(&mut beginning_of_groups).ok_or_else(|| Error::MarshalError)?;
                let entry_size = group.delete_entry(entry_id)?;
                if entry_size > 0 {
                    let group_size = group.header.group_size.get() - entry_size;
                    group.header.group_size.set(group_size);

                    let apcb_size = self.header.apcb_size.get();
                    assert!(apcb_size >= group_size);
                    self.beginning_of_groups.copy_within((group_size as usize)..(apcb_size as usize), 0);
                    self.header.apcb_size.set(apcb_size.checked_sub(group_size as u32).ok_or_else(|| Error::MarshalError)?);

                    self.remaining_used_size = self.remaining_used_size.checked_sub(group_size as usize).ok_or_else(|| Error::MarshalError)?;
                    break 'outer;
                }
            }
            let group = Self::next_item(&mut self.beginning_of_groups).ok_or_else(|| Error::MarshalError)?;
            let group_size = group.header.group_size.get() as usize;
            self.remaining_used_size -= group_size;
        }
        Ok(())
    }
    pub fn load(backing_store: &'a mut [u8]) -> Result<Self> {
        let mut backing_store = &mut *backing_store;
        let header = take_header_from_collection::<APCB_V2_HEADER>(&mut backing_store)
            .ok_or_else(|| Error::MarshalError)?;

        assert!(usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>());
        assert!(header.version.get() == 0x30);
        assert!(header.apcb_size.get() >= header.header_size.get().into());

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()
        {
            let value = take_header_from_collection::<APCB_V3_HEADER_EXT>(&mut backing_store)
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

        let remaining_used_size = (header.apcb_size.get() - u32::from(header.header_size)) as usize;
        assert!(backing_store.len() >= remaining_used_size);

        Ok(Self {
            header: header,
            v3_header_ext: v3_header_ext,
            beginning_of_groups: backing_store,
            remaining_used_size: remaining_used_size,
        })
    }
    pub fn create(backing_store: &'a mut [u8]) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let mut backing_store = &mut *backing_store;
            let header = take_header_from_collection::<APCB_V2_HEADER>(&mut backing_store)
                    .ok_or_else(|| Error::MarshalError)?;
            *header = Default::default();

            let v3_header_ext = take_header_from_collection::<APCB_V3_HEADER_EXT>(&mut backing_store)
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
        let groups = APCB::create(&mut buffer[0..]).unwrap();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let groups = APCB::create(&mut buffer[0..]).unwrap();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
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
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
        groups.insert_group(0x1704, *b"MEMG")?;
        let mut count = 0;
        let groups = APCB::load(&mut buffer[0..]).unwrap();
        for group in groups {
            match count {
                0 => {
                    assert!(group.id() == 0x1701);
                    assert!(group.signature() ==*b"PSPG");
                },
                1 => {
                    assert!(group.id() == 0x1704);
                    assert!(group.signature() ==*b"MEMG");
                },
                _ => {
                    assert!(false);
                }
            }
            count += 1;
        }
        assert!(count == 2);
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_first_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
        groups.insert_group(0x1704, *b"MEMG")?;
        groups.delete_group(0x1701)?;
        let mut count = 0;
        let groups = APCB::load(&mut buffer[0..]).unwrap();
        for group in groups {
            match count {
                0 => {
                    assert!(group.id() == 0x1704);
                    assert!(group.signature() ==*b"MEMG");
                },
                _ => {
                    assert!(false);
                }
            }
            count += 1;
        }
        assert!(count == 1);
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
        groups.insert_group(0x1704, *b"MEMG")?;
        groups.delete_group(0x1704)?;
        let mut count = 0;
        let groups = APCB::load(&mut buffer[0..]).unwrap();
        for group in groups {
            match count {
                0 => {
                    assert!(group.id() == 0x1701);
                    assert!(group.signature() ==*b"PSPG");
                },
                _ => {
                    assert!(false);
                }
            }
            count += 1;
        }
        assert!(count == 1);
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
        groups.insert_group(0x1704, *b"MEMG")?;
        groups.delete_group(0x4711)?;
        let mut count = 0;
        let groups = APCB::load(&mut buffer[0..]).unwrap();
        for group in groups {
            match count {
                0 => {
                    assert!(group.id() == 0x1701);
                    assert!(group.signature() ==*b"PSPG");
                },
                1 => {
                    assert!(group.id() == 0x1704);
                    assert!(group.signature() ==*b"MEMG");
                },
                _ => {
                    assert!(false);
                }
            }
            count += 1;
        }
        assert!(count == 2);
        Ok(())
    }

    #[test]
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut groups = APCB::create(&mut buffer[0..]).unwrap();
        groups.insert_group(0x1701, *b"PSPG")?;
        groups.delete_group(0x1701)?;
        let groups = APCB::load(&mut buffer[0..]).unwrap();
        for group in groups {
            assert!(false);
        }
        Ok(())
    }

    // TODO: delete_entry tests.
}
