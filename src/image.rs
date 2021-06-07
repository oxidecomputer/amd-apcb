use core::mem::size_of;
use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
use crate::ondisk::APCB_GROUP_HEADER;
use zerocopy::LayoutVerified;

pub struct APCB<'a> {
    header: &'a APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: &'a mut [u8],
}

#[derive(Debug)]
pub enum Error {
    MarshalError,
}

type Result<Q> = core::result::Result<Q, Error>;

pub struct Groups<'a> {
    header: &'a APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups: &'a mut [u8],
}

impl<'a> Iterator for Groups<'a> {
    type Item = APCB_GROUP_HEADER;

    fn next(&mut self) -> Option<APCB_GROUP_HEADER> {
        let (header, mut rest) = LayoutVerified::<_, APCB_GROUP_HEADER>::new_unaligned_from_prefix(&mut *self.beginning_of_groups)?;
        Some(*header)
    }
}

impl<'a> APCB<'a> {
    pub fn load(backing_store: &'a mut [u8]) -> Result<Self> {
        let (header, mut rest) = LayoutVerified::<_, APCB_V2_HEADER>::new_unaligned_from_prefix(&mut *backing_store).ok_or_else(|| Error::MarshalError)?;
        let header = header.into_ref();

        assert!(usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>());
        assert!(header.version.get() == 0x30);
        assert!(header.apcb_size.get() >= header.header_size.get().into());

        let v3_header_ext = if usize::from(header.header_size) == size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>() {
            let (value, r) = LayoutVerified::<_, APCB_V3_HEADER_EXT>::new_unaligned_from_prefix(rest).ok_or_else(|| Error::MarshalError)?;
            rest = r;
            let value = value.into_ref();
            assert!(value.signature == *b"ECB2");
            assert!(value.struct_version.get() == 0x12);
            assert!(value.data_version.get() == 0x100);
            assert!(value.ext_header_size.get() == 88);
            assert!(u32::from(value.data_offset.get()) == value.ext_header_size.get());
            assert!(value.signature_ending == *b"BCPA");
            Some(*value)
        } else {
            //// TODO: Maybe skip weird header
            None
        };

        Ok(Self {
            header: header,
            v3_header_ext: v3_header_ext,
            beginning_of_groups: rest,
        })
    }
    pub fn create(backing_store: &'a mut [u8], size: usize) -> Result<Self> {
        assert!(size >= 8 * 1024);
        for i in 0..size {
            backing_store[i] = 0xFF;
        }
        {
            let (mut layout, rest) = LayoutVerified::<_, APCB_V2_HEADER>::new_unaligned_from_prefix(&mut *backing_store).ok_or_else(|| Error::MarshalError)?;
            let header = &mut *layout;
            *header = Default::default();

            let (mut layout, rest) = LayoutVerified::<_, APCB_V3_HEADER_EXT>::new_unaligned_from_prefix(rest).ok_or_else(|| Error::MarshalError)?;
            let v3_header_ext = &mut *layout;
            *v3_header_ext = Default::default();

            header.apcb_size = ((size_of::<APCB_V2_HEADER>() + size_of::<APCB_V3_HEADER_EXT>()) as u32).into();
        }

        Self::load(backing_store)
    }
    pub fn into_groups(self) -> Groups<'a> {
        Groups {
            header: self.header,
            v3_header_ext: self.v3_header_ext,
            beginning_of_groups: self.beginning_of_groups
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::APCB;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; 8*1024] = [0xFF; 8*1024];
        APCB::load(&mut buffer[0..]).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; 8*1024] = [0xFF; 8*1024];
        let groups = APCB::create(&mut buffer[0..], 8*1024).unwrap().into_groups();
        for item in groups {
            assert!(false);
        }
    }
}
