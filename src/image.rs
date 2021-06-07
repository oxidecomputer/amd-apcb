use core::mem;
use core::mem::size_of;
use core::slice;

#[cfg(feature="no_std")]
use core_io::{Read, Seek, SeekFrom, Write};
#[cfg(feature="no_std")]
use core_io::Error as IOError;

#[cfg(feature="std")]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(feature="std")]
use std::io::Error as IOError;

use crate::ondisk::APCB_V2_HEADER;
use crate::ondisk::APCB_V3_HEADER_EXT;
use zerocopy::{FromBytes, LayoutVerified, Unaligned};

pub struct APCB<S: Read + Write + Seek> {
    backing_store: S,
    header: APCB_V2_HEADER,
    v3_header_ext: Option<APCB_V3_HEADER_EXT>,
    beginning_of_groups_position: u64,
}

#[derive(Debug)]
pub enum Error {
    IOError(IOError),
    MarshalError,
}

impl From<IOError> for Error {
    fn from(error: IOError) -> Self {
        Self::IOError(error)
    }
}

type Result<Q> = core::result::Result<Q, Error>;

fn read_struct<T: FromBytes + Unaligned + Copy, R: Read>(mut read: R) -> Result<T> {
    let num_bytes = mem::size_of::<T>();
    unsafe {
        let mut s: T = mem::uninitialized();
        let buffer = slice::from_raw_parts_mut(&mut s as *mut T as *mut u8, num_bytes);
        match read.read_exact(buffer) {
            Ok(()) => {
                let r = LayoutVerified::new_unaligned(buffer).map(LayoutVerified::into_ref).expect("slice of incorrect length or alignment");
                Ok(*r)
            },
            Err(e) => {
                mem::forget(s);
                Err(Error::IOError(e))
            }
        }
    }
}

impl<S: Read + Write + Seek> APCB<S> {
    pub fn create(mut backing_store: S, size: usize) -> Result<Self> {
        assert!(size >= 8 * 1024);
        let old_position = backing_store.seek(SeekFrom::Current(0))? as u64;
        for _i in 0..size {
            backing_store.write(&[0xFF])?;
        }
        backing_store.seek(SeekFrom::Start(old_position))?;
        Self::load(backing_store)
    }
    pub fn load(mut backing_store: S) -> Result<Self> {
        let header = read_struct::<APCB_V2_HEADER, _>(&mut backing_store)?;
        assert!(usize::from(header.header_size) >= size_of::<APCB_V2_HEADER>());
        assert!(header.version.get() == 0x30);
        assert!(header.apcb_size.get() >= header.header_size.get().into());

        let v3_header_ext = if usize::from(header.header_size) > size_of::<APCB_V2_HEADER>() {
            let value = read_struct::<APCB_V3_HEADER_EXT, _>(&mut backing_store)?;
            assert!(value.signature == *b"ECB2");
            assert!(value.struct_version.get() == 0x12);
            assert!(value.data_version.get() == 0x100);
            assert!(value.ext_header_size.get() == 88);
            assert!(u32::from(value.data_offset.get()) == value.ext_header_size.get());
            assert!(value.signature_ending == *b"BCPA");
            //// TODO: Maybe skip weird header
            //backing_store.seek(SeekFrom::Current(i64::from(header.header_size) - (size_of::<APCB_V2_HEADER>() as i64)))?;
            Some(value)
        } else {
            None
        };


        let beginning_of_groups_position = backing_store.seek(SeekFrom::Current(0))? as u64;
        Ok(Self {
            backing_store,
            header,
            v3_header_ext,
            beginning_of_groups_position,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::APCB;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; 8*1024] = [0xFF; 8*1024];
        APCB::load(&mut std::io::Cursor::new(&mut buffer[0..])).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; 8*1024] = [0xFF; 8*1024];
        APCB::create(&mut std::io::Cursor::new(&mut buffer[0..]), 8*1024).unwrap();
    }
}
