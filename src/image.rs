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

use ssmarshal::deserialize;
use serde::de::DeserializeOwned;
use crate::ondisk::APCB_V2_HEADER;

pub struct APCB<S: Read + Write + Seek> {
    backing_store: S,
    header: APCB_V2_HEADER,
    beginning_of_groups_position: u64,
}

#[derive(Debug)]
pub enum Error {
    IOError(IOError),
    MarshalError(ssmarshal::Error),
}

impl From<IOError> for Error {
    fn from(error: IOError) -> Self {
        Self::IOError(error)
    }
}

impl From<ssmarshal::Error> for Error {
    fn from(error: ssmarshal::Error) -> Self {
        Self::MarshalError(error)
    }
}

type Result<Q> = core::result::Result<Q, Error>;

fn read_struct<T : Sized + DeserializeOwned, R: Read>(mut read: R) -> Result<T> {
    let num_bytes = mem::size_of::<T>();
    unsafe {
        let mut s: T = mem::uninitialized();
        let buffer = slice::from_raw_parts_mut(&mut s as *mut T as *mut u8, num_bytes);
        match read.read_exact(buffer) {
            Ok(()) => {
                let (result, size) = deserialize::<T>(&buffer)?;
                assert!(size == size_of::<T>());
                Ok(result)
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
        assert!(header.version == 0x30);
        assert!(header.apcb_size >= header.header_size.into());

        // TODO: parse V3 header

        // Skip V3 header
        backing_store.seek(SeekFrom::Current(i64::from(header.header_size) - (size_of::<APCB_V2_HEADER>() as i64)))?;

        let beginning_of_groups_position = backing_store.seek(SeekFrom::Current(0))? as u64;
        Ok(Self {
            backing_store,
            header,
            beginning_of_groups_position,
        })
    }
}
