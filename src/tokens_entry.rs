
use crate::types::{Buffer, ReadOnlyBuffer, Result, Error};
use crate::ondisk::{TOKEN_ENTRY, TokenType, take_header_from_collection, take_header_from_collection_mut};
use num_traits::FromPrimitive;
use core::mem::size_of;

#[derive(Debug)]
pub struct TokensEntryBodyItem<BufferType> {
    unit_size: u8,
    entry_id: TokenType,
    buf: BufferType,
    used_size: usize,
}

pub struct TokensEntryIter<'a> {
    entry_id: TokenType,
    buf: ReadOnlyBuffer<'a>,
    remaining_used_size: usize,
}

pub struct TokensEntryIterMut<'a> {
    entry_id: TokenType,
    buf: Buffer<'a>,
    remaining_used_size: usize,
}

impl<BufferType> TokensEntryBodyItem<BufferType> {
    pub(crate) fn new(unit_size: u8, entry_id: u16, buf: BufferType, used_size: usize) -> Result<Self> {
        if unit_size != 8 {
            return Err(Error::FileSystem("unit_size of token is unknown", "ENTRY_HEADER::unit_size"));
        }
        Ok(Self {
            unit_size,
            entry_id: TokenType::from_u16(entry_id).ok_or_else(|| Error::FileSystem("entry_id of token is unknown", "ENTRY_HEADER::entry_id"))?,
            buf,
            used_size,
        })
    }
}

#[derive(Debug)]
pub struct TokensEntryItemMut<'a> {
    entry_id: TokenType,
    token: &'a mut TOKEN_ENTRY,
}

impl<'a> TokensEntryItemMut<'a> {
    pub fn id(&self) -> u32 {
        self.token.key.get()
    }
    pub fn value(&self) -> u32 { // TODO: Clamp.
        self.token.value.get() & match self.entry_id {
            TokenType::Bool => 0x1,
            TokenType::Byte => 0xFF,
            TokenType::Word => 0xFFFF,
            TokenType::DWord => 0xFFFF_FFFF,
        }
    }

    // Since the id is a sort key, it cannot be mutated.

    pub fn set_value(&mut self, value: u32) {
        assert!(value == (value & match self.entry_id {
            TokenType::Bool => 0x1,
            TokenType::Byte => 0xFF,
            TokenType::Word => 0xFFFF,
            TokenType::DWord => 0xFFFF_FFFF,
        }));
        self.token.value.set(value)
    }
}

impl<'a> TokensEntryIterMut<'a> {
    /// It's useful to have some way of NOT mutating self.buf.  This is what this function does.
    /// Note: The caller needs to manually decrease remaining_used_size for each call if desired.
    fn next_item<'b>(entry_id: TokenType, buf: &mut Buffer<'b>) -> Result<TokensEntryItemMut<'b>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem("unexpected EOF while reading header of Token Entry", ""));
        }
        let token = match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem("could not read Token Entry", ""));
            }
        };
        Ok(TokensEntryItemMut {
            entry_id,
            token: token,
        })
    }

    /// Find the place BEFORE which the entry TOKEN_ID is supposed to go.
    pub(crate) fn move_insertion_point_before(&mut self, token_id: u32) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                break;
            }
            match Self::next_item(self.entry_id, &mut buf) {
                Ok(e) => {
                    if e.id() < token_id {
                        self.next().ok_or_else(|| Error::Internal)?;
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
    /// Find the place BEFORE which the entry TOKEN_ID is supposed to go.
    pub(crate) fn move_point_to(&mut self, token_id: u32) -> Result<()> {
        loop {
            let mut buf = &mut self.buf[..self.remaining_used_size];
            if buf.len() == 0 {
                return Err(Error::TokenNotFound);
            }
            match Self::next_item(self.entry_id, &mut buf) {
                Ok(e) => {
                    if e.id() != token_id {
                        self.next().ok_or_else(|| Error::Internal)?;
                    } else {
                        return Ok(());
                    }
                },
                Err(e) => {
                    return Err(e);
                },
            }
        }
    }

    /// Inserts the given entry data at the right spot.
    /// Precondition: Caller already increased the group size by entry_size.
    pub(crate) fn insert_token(&mut self, token_id: u32, token_value: u32) -> Result<TokensEntryItemMut<'a>> {
        let token_size = size_of::<TOKEN_ENTRY>();

        // Make sure that move_insertion_point_before does not notice the new uninitialized token
        self.remaining_used_size = self.remaining_used_size.checked_sub(token_size as usize).ok_or_else(|| Error::FileSystem("Tokens Entry is bigger than remaining iterator size", ""))?;
        self.move_insertion_point_before(token_id)?;
        // Move the entries from after the insertion point to the right (in order to make room before for our new entry).
        self.buf.copy_within(0..self.remaining_used_size, token_size as usize);

        self.remaining_used_size = self.remaining_used_size.checked_add(token_size as usize).ok_or_else(|| Error::FileSystem("Tokens Entry is bigger than remaining iterator size", ""))?;
        let token = match take_header_from_collection_mut::<TOKEN_ENTRY>(&mut self.buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem("could not read Token Entry", ""));
            }
        };

        token.key.set(token_id);
        token.value.set(0); // always valid
        let mut result = TokensEntryItemMut {
            entry_id: self.entry_id,
            token,
        };

        // This has better error checking
        result.set_value(token_value);
        Ok(result)
    }

    /// Delete the token TOKEN_ID from the entry.
    /// Postcondition: Caller resizes the containers afterwards.
    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        self.move_point_to(token_id)?;
        let token_size = size_of::<TOKEN_ENTRY>();
        // Move the tokens behind this one to the left
        self.buf.copy_within(token_size..self.remaining_used_size, 0);
        self.remaining_used_size = self.remaining_used_size.checked_sub(token_size as usize).ok_or_else(|| Error::FileSystem("Tokens Entry is bigger than remaining iterator size", ""))?;
        Ok(())
    }
}

impl<'a> Iterator for TokensEntryIterMut<'a> {
    type Item = TokensEntryItemMut<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_used_size == 0 {
            return None;
        }
        match Self::next_item(self.entry_id, &mut self.buf) {
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
    entry_id: TokenType,
    entry: &'a TOKEN_ENTRY,
}

impl<'a> TokensEntryItem<'a> {
    pub fn id(&self) -> u32 {
        self.entry.key.get()
    }
    pub fn value(&self) -> u32 { // TODO: Clamp.
        self.entry.value.get() & match self.entry_id {
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
    fn next_item<'a>(entry_id: TokenType, buf: &mut ReadOnlyBuffer<'a>) -> Result<TokensEntryItem<'a>> {
        if buf.len() == 0 {
            return Err(Error::FileSystem("unexpected EOF while reading header of Token Entry", ""));
        }
        let header = match take_header_from_collection::<TOKEN_ENTRY>(&mut *buf) {
            Some(item) => item,
            None => {
                return Err(Error::FileSystem("could not read header of Token Entry", ""));
            }
        };
        Ok(TokensEntryItem {
            entry_id,
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
        match Self::next_item(self.entry_id, &mut self.buf) {
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
    pub fn iter(&self) -> TokensEntryIter<'_> {
        TokensEntryIter {
            entry_id: self.entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }

    pub fn iter_mut(&mut self) -> TokensEntryIterMut<'_> {
        TokensEntryIterMut {
            entry_id: self.entry_id,
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

    pub(crate) fn delete_token(&mut self, token_id: u32) -> Result<()> {
        self.iter_mut().delete_token(token_id)
    }
}

impl<'a> TokensEntryBodyItem<ReadOnlyBuffer<'a>> {
    pub fn iter(&self) -> TokensEntryIter<'_> {
        TokensEntryIter {
            entry_id: self.entry_id,
            buf: self.buf,
            remaining_used_size: self.used_size,
        }
    }
}
