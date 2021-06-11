use crate::ondisk::APCB_GROUP_HEADER;
use crate::ondisk::APCB_TYPE_ALIGNMENT;
use crate::ondisk::APCB_TYPE_HEADER;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
pub use crate::ondisk::{ContextFormat, ContextType};
use core::mem::{replace, size_of};
use num_traits::FromPrimitive;
use zerocopy::LayoutVerified;

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// If the item cannot be parsed, returns None and does not advance.
fn take_header_from_collection<'a, T: Sized>(buf: &mut &'a mut [u8]) -> Option<LayoutVerified::<&'a mut [u8], T>> {
    let xbuf = replace(&mut *buf, &mut []);
    match LayoutVerified::<_, T>::new_from_prefix(xbuf) {
        Some((item, xbuf)) => {
            *buf = xbuf;
            Some(item)
        }
        None => None,
    }
}

/// Given *BUF (a collection of multiple items), retrieves the first of the items and returns it after advancing *BUF to the next item.
/// Assuming that *BUF had been aligned to ALIGNMENT before the call, it also ensures that *BUF is aligned to ALIGNMENT after the call.
/// If the item cannot be parsed, returns None and does not advance.
fn take_body_from_collection<'a>(buf: &mut &'a mut [u8], size: usize, alignment: usize) -> Option<&'a mut [u8]> {
    let xbuf = replace(&mut *buf, &mut []);
    if xbuf.len() >= size {
        let (item, xbuf) = xbuf.split_at_mut(size);
        if size % alignment != 0 && xbuf.len() >= alignment - (size % alignment) {
            let (_, b) = xbuf.split_at_mut(alignment - (size % alignment));
            *buf = b;
        } else {
            *buf = xbuf;
        }
        Some(item)
    } else {
        None
    }
}

pub struct APCB<'a> {
    header: LayoutVerified<&'a mut [u8], APCB_V2_HEADER>,
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
    pub header: LayoutVerified<&'a mut [u8], APCB_TYPE_HEADER>,
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
    pub header: LayoutVerified<&'a mut [u8], APCB_GROUP_HEADER>,
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
        if self.buf.len() == 0 {
            return None;
        }
        let header = take_header_from_collection::<APCB_TYPE_HEADER>(&mut self.buf)?;
        let _: ContextFormat = FromPrimitive::from_u8(header.context_format).unwrap();
        let _: ContextType = FromPrimitive::from_u8(header.context_type).unwrap();
        assert!(header.group_id.get() == self.header.group_id.get());
        let type_size = header.type_size.get() as usize;

        assert!(type_size >= size_of::<APCB_TYPE_HEADER>());
        let payload_size = type_size - size_of::<APCB_TYPE_HEADER>();
        let body = take_body_from_collection(&mut self.buf, payload_size, APCB_TYPE_ALIGNMENT)?;

        Some(Entry {
            header: header,
            body: body,
        })
    }
}

impl<'a> Iterator for APCB<'a> {
    type Item = Group<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.beginning_of_groups.len() == 0 {
            return None;
        }
        let header = take_header_from_collection::<APCB_GROUP_HEADER>(&mut self.beginning_of_groups)?;
        let group_size = header.group_size.get() as usize;
        assert!(group_size >= size_of::<APCB_GROUP_HEADER>());
        let payload_size = group_size - size_of::<APCB_GROUP_HEADER>();
        let body = take_body_from_collection(&mut self.beginning_of_groups, payload_size, 1)?;
        self.remaining_used_size -= group_size;

        Some(Group {
            header: header,
            buf: body,
        })
    }
}

impl<'a> APCB<'a> {
    pub fn group_mut(&mut self, group_id: u16) -> Option<Group> {
        for group in self {
            if group.id() == group_id {
                return Some(group);
            }
        }
        None
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
            let backing_store = &mut *backing_store;
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
