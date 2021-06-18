
use crate::types::{Buffer, ReadOnlyBuffer, Result, Error};
use crate::ondisk::{APCB_TOKEN_ENTRY, TokenType, take_header_from_collection, take_header_from_collection_mut};
use num_traits::FromPrimitive;

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    unit_size: u8,
    type_id: TokenType,
    buf: BufferType,
    used_size: usize,
}

pub struct TokensEntryIter<'a> {
    type_id: TokenType,
    buf: ReadOnlyBuffer<'a>,
    remaining_used_size: usize,
}

pub struct TokensEntryIterMut<'a> {
    type_id: TokenType,
    buf: Buffer<'a>,
    remaining_used_size: usize,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(unit_size: u8, type_id: u16, buf: BufferType, used_size: usize) -> Result<Self> {
        if unit_size != 8 {
            return Err(Error::FileSystemError("unit_size of token is unknown", "APCB_TYPE_HEADER::unit_size"));
        }
        Ok(Self {
            unit_size,
            type_id: TokenType::from_u16(type_id).ok_or_else(|| Error::FileSystemError("type_id of token is unknown", "APCB_TYPE_HEADER::type_id"))?,
            buf,
            used_size,
        })
    }
}

#[derive(Debug)]
pub struct TokensEntryItemMut<'a> {
    type_id: TokenType,
    entry: &'a mut APCB_TOKEN_ENTRY,
}

impl<'a> TokensEntryItemMut<'a> {
    pub fn key(&self) -> u32 {
        self.entry.key.get()
    }
    pub fn value(&self) -> u32 { // TODO: Clamp.
        self.entry.value.get() & match self.type_id {
            TokenType::Bool => 0x1,
            TokenType::Byte => 0xFF,
            TokenType::Word => 0xFFFF,
            TokenType::DWord => 0xFFFF_FFFF,
        }
    }

    pub fn set_key(&mut self, value: u32) {
        self.entry.key.set(value)
    }
    pub fn set_value(&mut self, value: u32) {
        self.entry.value.set(value)
    }
}

impl TokensEntryIterMut<'_> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'a>(type_id: TokenType, buf: &mut Buffer<'a>) -> Result<TokensEntryItemMut<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Token Entry", ""));
        }
        let header = match take_header_from_collection_mut::<APCB_TOKEN_ENTRY>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Token Entry", ""));
            }
        };
        Ok(TokensEntryItemMut {
            type_id,
            entry: header,
        })
    }
}

impl<'a> Iterator for TokensEntryIterMut<'a> {
    type Item = TokensEntryItemMut<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(self.type_id, &mut self.buf) {
            Ok(e) => {
                assert!(self.remaining_used_size >= 8);
                self.remaining_used_size -= 8;
                Some(e)
            },
            Err(e) => {
                None
            },
        }
    }
}

#[derive(Debug)]
pub struct TokensEntryItem<'a> {
    type_id: TokenType,
    entry: &'a APCB_TOKEN_ENTRY,
}

impl TokensEntryIter<'_> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'a>(type_id: TokenType, buf: &mut ReadOnlyBuffer<'a>) -> Result<TokensEntryItem<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Token Entry", ""));
        }
        let header = match take_header_from_collection::<APCB_TOKEN_ENTRY>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read header of Token Entry", ""));
            }
        };
        Ok(TokensEntryItem {
            type_id,
            entry: header,
        })
    }
}

impl<'a> Iterator for TokensEntryIter<'a> {
    type Item = TokensEntryItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(self.type_id, &mut self.buf) {
            Ok(e) => {
                assert!(self.remaining_used_size >= 8);
                self.remaining_used_size -= 8;
                Some(e)
            },
            Err(e) => {
                None
            },
        }
    }
}

impl<'a> TokensEntryBodyItem<Buffer<'a>> {
    pub fn iter(&self) -> TokensEntryIter {
        TokensEntryIter {
            type_id: self.type_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }

    pub fn iter_mut(&mut self) -> TokensEntryIterMut {
        TokensEntryIterMut {
            type_id: self.type_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}

impl<'a> TokensEntryBodyItem<ReadOnlyBuffer<'a>> {
    pub fn iter(&self) -> TokensEntryIter {
        TokensEntryIter {
            type_id: self.type_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}
