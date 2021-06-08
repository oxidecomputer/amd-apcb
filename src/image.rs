use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_ALIGNMENT;
use crate::ondisk::APCB_TYPE_HEADER;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
pub use crate::ondisk::{ContextFormat, ContextType};
use core::mem::replace;
use core::mem::size_of;
use num_traits::FromPrimitive;
use zerocopy::LayoutVerified;

pub struct APCB<'a> {
    header: &'a APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: &'a mut [u8],
    remaining_used_size: usize,
}

#[derive(Debug)]
pub enum Error {
    MarshalError,
}

type Result<Q> = core::result::Result<Q, Error>;

#[derive(Debug)]
pub struct Entry<'a> {
    pub header: APCB_TYPE_HEADER,
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
    pub fn board_instance_mask(&self) -> u8 {
        self.header.board_instance_mask
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
    pub header: APCB_GROUP_HEADER,
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

    /// Finds the first Entry with the given id, if any, and returns it.
    pub fn first_entry_mut(&mut self, id: u16) -> Option<Entry> {
        for entry in self {
            if entry.id() == id {
                return Some(entry);
            }
        }
        None
    }
}

impl<'a> Iterator for Group<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = {
            let buf = &self.buf[0..];
            let (header, _) =
                LayoutVerified::<_, APCB_TYPE_HEADER>::new_unaligned_from_prefix(&*buf)?;
            let header = header.into_ref();
            *header
        };
        let _: ContextFormat = FromPrimitive::from_u8(header.context_format).unwrap();
        let _: ContextType = FromPrimitive::from_u8(header.context_type).unwrap();
        assert!(header.group_id.get() == self.header.group_id.get());
        let type_size = header.type_size.get() as usize;
        assert!(type_size >= size_of::<APCB_TYPE_HEADER>());

        let buf = replace(&mut self.buf, &mut []);
        let (item, mut buf) = buf.split_at_mut(type_size);
        if type_size % APCB_TYPE_ALIGNMENT != 0 {
            // Align to APCB_TYPE_ALIGNMENT.
            let (_, b) = buf.split_at_mut(APCB_TYPE_ALIGNMENT - (type_size % APCB_TYPE_ALIGNMENT));
            buf = b;
        }
        self.buf = buf;

        Some(Entry {
            header: header,
            body: &mut item[size_of::<APCB_TYPE_HEADER>()..],
        })
    }
}

impl<'a> Iterator for APCB<'a> {
    type Item = Group<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = {
            let beginning_of_groups = &self.beginning_of_groups[0..self.remaining_used_size];
            let (header, _) = LayoutVerified::<_, APCB_GROUP_HEADER>::new_unaligned_from_prefix(
                &*beginning_of_groups,
            )?;
            let header = header.into_ref();
            *header
        };
        let group_size = header.group_size.get() as usize;
        assert!(group_size >= size_of::<APCB_GROUP_HEADER>());

        let beginning_of_groups = replace(&mut self.beginning_of_groups, &mut []);
        let (item, beginning_of_groups) = beginning_of_groups.split_at_mut(group_size);
        self.beginning_of_groups = beginning_of_groups;
        self.remaining_used_size -= group_size;

        //let body = &mut self.beginning_of_groups[self.position+size_of::<APCB_GROUP_HEADER>()..group_size];
        Some(Group {
            header: header,
            buf: &mut item[size_of::<APCB_GROUP_HEADER>()..],
        })
    }
}

impl<'a> APCB<'a> {
    pub fn load(backing_store: &'a mut [u8]) -> Result<Self> {
        let (header, mut rest) =
            LayoutVerified::<_, APCB_V2_HEADER>::new_unaligned_from_prefix(&mut *backing_store)
                .ok_or_else(|| Error::MarshalError)?;
        let header = header.into_ref();

        assert!(usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>());
        assert!(header.version.get() == 0x30);
        assert!(header.apcb_size.get() >= header.header_size.get().into());

        let v3_header_ext = if usize::from(header.header_size)
            == size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()
        {
            let (value, r) =
                LayoutVerified::<_, APCB_V3_HEADER_EXT>::new_unaligned_from_prefix(rest)
                    .ok_or_else(|| Error::MarshalError)?;
            rest = r;
            let value = value.into_ref();
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

        Ok(Self {
            header: header,
            v3_header_ext: v3_header_ext,
            beginning_of_groups: rest,
            remaining_used_size: (header.apcb_size.get() - u32::from(header.header_size)) as usize,
        })
    }
    pub fn create(backing_store: &'a mut [u8]) -> Result<Self> {
        for i in 0..backing_store.len() {
            backing_store[i] = 0xFF;
        }
        {
            let (mut layout, rest) =
                LayoutVerified::<_, APCB_V2_HEADER>::new_unaligned_from_prefix(&mut *backing_store)
                    .ok_or_else(|| Error::MarshalError)?;
            let header = &mut *layout;
            *header = Default::default();

            let (mut layout, _rest) =
                LayoutVerified::<_, APCB_V3_HEADER_EXT>::new_unaligned_from_prefix(rest)
                    .ok_or_else(|| Error::MarshalError)?;
            let v3_header_ext = &mut *layout;
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
    use crate::APCB;

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
        for item in groups {
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
}
