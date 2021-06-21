
use crate::types::{Buffer, ReadOnlyBuffer, Result, Error};
use crate::ondisk::{TOKEN_ENTRY, TokenType, take_header_from_collection, take_header_from_collection_mut};
use num_traits::FromPrimitive;
use core::mem::size_of;

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
            return Err(Error::FileSystemError("unit_size of token is unknown", "TYPE_HEADER::unit_size"));
        }
        Ok(Self {
            unit_size,
            type_id: TokenType::from_u16(type_id).ok_or_else(|| Error::FileSystemError("type_id of token is unknown", "TYPE_HEADER::type_id"))?,
            buf,
            used_size,
        })
    }
}

#[derive(Debug)]
pub struct TokensEntryItemMut<'a> {
    type_id: TokenType,
    entry: &'a mut TOKEN_ENTRY,
}

impl<'a> TokensEntryItemMut<'a> {
    pub fn id(&self) -> u32 {
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

    // Since the id is a sort key, it cannot be mutated.

    pub fn set_value(&mut self, value: u32) {
        self.entry.value.set(value)
    }
}

impl<'a> TokensEntryIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(type_id: TokenType, buf: &mut Buffer<'b>) -> Result<TokensEntryItemMut<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Token Entry", ""));
        }
        let entry = match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read Token Entry", ""));
            }
        };
        Ok(TokensEntryItemMut {
            type_id,
            entry: entry,
        })
    }

    /// Find the place BEFORE which the entry TOKEN_ID is supposed to go.
    pub(crate) fn move_insertion_point_before(&mut self, token_id: u32) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            match Self::next_item(self.type_id, &mut buf) {
                Ok(e) => {
                    if e.entry.key.get() < token_id {
                        let entry = Self::next_item(self.type_id, &mut self.buf).unwrap();
                        let entry_size = size_of::<TOKEN_ENTRY>();
                        assert!(self.remaining_used_size >= entry_size);
                        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size).ok_or_else(|| Error::FileSystemError("Entry is bigger than remaining
 Iterator size", ""))?;
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
    pub(crate) fn insert_token(&mut self, token_id: u32, token_value: u32) -> Result<TokensEntryItemMut<'a>> {
        let entry_size = size_of::<TOKEN_ENTRY>();

        // Make sure that move_insertion_point_before does not notice the new uninitialized entry
        self.remaining_used_size = self.remaining_used_size.checked_sub(entry_size as usize).ok_or_else(|| Error::FileSystemError("Tokens Entry is bigger than remaining iterator size", ""))?;
        self.move_insertion_point_before(token_id)?;
        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        self.buf.copy_within(0..self.remaining_used_size, entry_size as usize);

        self.remaining_used_size = self.remaining_used_size.checked_add(entry_size as usize).ok_or_else(|| Error::FileSystemError("Tokens Entry is bigger than remaining iterator size", ""))?;
        let entry = match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut self.buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystemError("could not read Token Entry", ""));
            }
        };
        entry.key.set(token_id);
        entry.value.set(token_value);
        Ok(TokensEntryItemMut {
            type_id: self.type_id,
            entry,
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
    entry: &'a TOKEN_ENTRY,
}

impl<'a> TokensEntryItem<'a> {
    pub fn id(&self) -> u32 {
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
}

impl TokensEntryIter<'_> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'a>(type_id: TokenType, buf: &mut ReadOnlyBuffer<'a>) -> Result<TokensEntryItem<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystemError("unexpected EOF while reading header of Token Entry", ""));
        }
        let header = match take_header_from_collection::<TOKEN_ENTRY>(&mut *buf) {
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

    pub(crate) fn insert_token(&mut self, token_id: u32, token_value: u32) -> Result<()> {
        match self.iter_mut().insert_token(token_id, token_value) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
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
