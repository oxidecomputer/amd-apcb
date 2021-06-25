#[cfg(test)]
mod tests {
    use crate::Apcb;
    use crate::Error;
    use crate::ondisk::ContextType;
    use crate::ondisk::TokenType;
    use crate::EntryItemBody;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        Apcb::load(&mut buffer[0..]).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let apcb = Apcb::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let apcb = Apcb::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
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
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() == *b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() == *b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_first_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1701)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1704)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        match apcb.delete_group(0x4711) {
            Err(Error::GroupNotFound) => {
            },
            _ => {
                panic!("test failed")
            }
        }
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.delete_group(0x1701)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _group in groups {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn delete_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, &[1u8; 48], 33)?;
        apcb.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, &[2u8; 48], 31)?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.delete_entry(0x1701, 96, 0, 0xFFFF)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 97);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn insert_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, &[1u8; 48], 33)?;
        apcb.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, &[2u8; 4], 32)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_tokens() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1001, *b"TOKN")?; // this group id should be 0x3000--but I want this test to test a complicated case even should we ever change insert_group to automatically sort.
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, &[1u8; 48], 33)?;
        apcb.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, &[2u8; 1], 32)?;

        // Insert empty "Token Entry"
        apcb.insert_entry(0x1001, 0, 0, 1, ContextType::Tokens, &[], 32)?;

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(0x1001, TokenType::Bool as u16, 0, 1, 0x014FBF20, 1)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1001);
        assert!(group.signature() ==*b"TOKN");

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_tokens_easy() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.insert_group(0x3000, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, &[1u8; 48], 33)?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, &[2u8; 1], 32)?;

        // let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // Insert empty "Token Entry"
        apcb.insert_entry(0x3000, TokenType::Bool as u16, 0, 1, ContextType::Tokens, &[], 32)?; // breaks

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(0x3000, TokenType::Bool as u16, 0, 1, 0x014FBF20, 1)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x3000);
        assert!(group.signature() ==*b"TOKN");
        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        match entry.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let mut tokens = tokens.iter();
                let token = tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x014FBF20);
                assert!(token.value() == 1);
                assert!(matches!(tokens.next(), None));
            },
            _ => {
                panic!("unexpected entry type");
            }
        }
        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_tokens_group_not_found() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.insert_group(0x3000, *b"TOKN")?;

        // Insert empty "Token Entry"
        match apcb.insert_entry(0x1001, 0, 0, 1, ContextType::Tokens, &[], 32) {
            Ok(_) => {
               panic!("insert_entry should not succeed");
            },
            Err(Error::GroupNotFound) => {
                Ok(())
            },
            Err(s) => {
                Err(s)
            },
        }
    }

    #[test]
    fn delete_tokens() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.insert_group(0x3000, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, &[1u8; 48], 33)?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, &[2u8; 1], 32)?;

        // let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // Insert empty "Token Entry"
        // insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload: &[u8], priority_mask: u8
        apcb.insert_entry(0x3000, TokenType::Byte as u16, 0, 1, ContextType::Tokens, &[], 32)?;

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(0x3000, TokenType::Byte as u16, 0, 1, 0x014FBF20, 1)?;
        apcb.insert_token(0x3000, TokenType::Byte as u16, 0, 1, 0x42, 2)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == 0x3000);
        assert!(group.signature() ==*b"TOKN");
        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        match entry.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let mut tokens = tokens.iter();

                let token = tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x42);
                assert!(token.value() == 2);

                let token = tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x014FBF20);
                assert!(token.value() == 1);

                assert!(matches!(tokens.next(), None));
            },
            _ => {
                panic!("unexpected entry type");
            }
        }
        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

}
