#[cfg(test)]
mod tests {
    use core::default::Default;
    use crate::Apcb;
    use crate::Error;
    use crate::ondisk::{ContextType, CcxEntryId, DfEntryId, PspEntryId, MemoryEntryId, TokenEntryId, EntryId, GroupId, memory::ConsoleOutControl, memory::DimmInfoSmbusElement, psp::BoardIdGettingMethodEeprom, psp::IdRevApcbMapping};
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
        let apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        let groups = apcb.groups();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        let groups = apcb.groups();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
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
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_first_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.delete_group(GroupId::Psp)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.delete_group(GroupId::Memory)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        match apcb.delete_group(GroupId::Token) {
            Err(Error::GroupNotFound) => {
            },
            _ => {
                panic!("test failed")
            }
        }
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.delete_group(GroupId::Psp)?;
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
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        apcb.insert_entry(EntryId::Psp(PspEntryId::Raw(97)), 0, 0xFFFF, ContextType::Struct, 31, &[2u8; 48])?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.delete_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF)?;
        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Raw(97)));

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn insert_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        apcb.insert_entry(EntryId::Psp(PspEntryId::Raw(97)), 0, 0xFFFF, ContextType::Struct, 32, &[2u8; 4])?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Raw(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_struct_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        apcb.insert_struct_entry(EntryId::Memory(MemoryEntryId::ConsoleOutControl), 0, 0xFFFF, 32, &ConsoleOutControl::default())?;

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");

        let mut entries = group.entries_mut();

        let mut entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Memory(MemoryEntryId::ConsoleOutControl));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let console_out_control = entry.body_as_struct_mut::<ConsoleOutControl>().unwrap();
        assert!(*console_out_control == ConsoleOutControl::default());
        if console_out_control.abl_console_out_control.enable_console_logging()? {
            console_out_control.abl_console_out_control.set_enable_console_logging(false);
        }

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_headered_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        let header = BoardIdGettingMethodEeprom::new(1, 2, 3, 4);
        let items = [
            IdRevApcbMapping::new(5, 4, 9, 3),
            IdRevApcbMapping::new(8, 7, 10, 6),
        ];
        apcb.insert_headered_struct_array_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, &header, 32, &items)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let (header, elements) = entry.body_as_headered_struct_array::<BoardIdGettingMethodEeprom, IdRevApcbMapping>().ok_or_else(|| Error::EntryTypeMismatch)?;
        assert!(*header == BoardIdGettingMethodEeprom::new(1,2,3,4));

        let mut elements = elements.iter();

        assert!(*elements.next().ok_or_else(|| Error::EntryTypeMismatch)? == IdRevApcbMapping::new(5, 4, 9, 3));
        assert!(*elements.next().ok_or_else(|| Error::EntryTypeMismatch)? == IdRevApcbMapping::new(8, 7, 10, 6));
        assert!(matches!(elements.next(), None));

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");

        let mut entries = group.entries();

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        let items = [
            DimmInfoSmbusElement::new(2, 3, 4, 5, 6, 7, 8),
            DimmInfoSmbusElement::new(10, 11, 12, 13, 14, 15, 16),
        ];
        apcb.insert_struct_array_entry(EntryId::Memory(MemoryEntryId::DimmInfoSmbus), 0, 0xFFFF, 32, &items)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Memory(MemoryEntryId::DimmInfoSmbus));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        match entry.body {
            EntryItemBody::<_>::Struct(buf) => {
                assert_eq!(*buf, [1, 2, 3, 4, 5, 6, 7, 8, 1, 10, 11, 12, 13, 14, 15, 16]);
            },
            _ => {
                panic!("wrong thing");
            }
        }

        let elements = entry.body_as_struct_array::<DimmInfoSmbusElement>().ok_or_else(|| Error::EntryTypeMismatch)?;
        let mut elements = elements.iter();

        assert_eq!(*elements.next().ok_or_else(|| Error::EntryTypeMismatch)?, DimmInfoSmbusElement::new(2, 3, 4, 5, 6, 7, 8));
        assert_eq!(*elements.next().ok_or_else(|| Error::EntryTypeMismatch)?, DimmInfoSmbusElement::new(10, 11, 12, 13, 14, 15, 16));
        assert!(matches!(elements.next(), None));

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_wrong_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        let items = [
            DimmInfoSmbusElement::new(2, 3, 4, 5, 6, 7, 8),
            DimmInfoSmbusElement::new(10, 11, 12, 13, 14, 15, 16),
        ];
        match apcb.insert_struct_array_entry(EntryId::Memory(MemoryEntryId::ConsoleOutControl), 0, 0xFFFF, 32, &items) {
            Err(Error::EntryTypeMismatch) => {
                Ok(())
            }
            _ => {
                panic!("should fail with EntryTypeMismatch");
            }
        }
    }

    #[test]
    fn insert_tokens() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Df, *b"DFG ")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        apcb.insert_entry(EntryId::Df(DfEntryId::SlinkConfig), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        apcb.insert_entry(EntryId::Df(DfEntryId::XgmiPhyOverride), 0, 0xFFFF, ContextType::Struct, 32, &[2u8; 1])?;

        // Insert empty "Token Entry"
        apcb.insert_entry(EntryId::Token(TokenEntryId::Bool), 0, 1, ContextType::Tokens, 32, &[])?;

        // pub(crate) fn insert_token(&mut self, entry_id: EntryId, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(EntryId::Token(TokenEntryId::Bool), 0, 1, 0x014FBF20, 1)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Df);
        assert!(group.signature() == *b"DFG ");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Df(DfEntryId::SlinkConfig));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Df(DfEntryId::XgmiPhyOverride));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Token);
        assert!(group.signature() == *b"TOKN");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Token(TokenEntryId::Bool));

        match entry.body {
            EntryItemBody::Tokens(tokens) => {
                 let mut tokens = tokens.iter();

                 let token = tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                 assert!(token.id() == 0x014FBF20);
                 assert!(token.value() == 1);

                 assert!(matches!(tokens.next(), None));
            },
            _ => panic!("no tokens"),
        }

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_tokens_easy() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::Raw(97)), 0, 0xFFFF, ContextType::Struct, 32, &[2u8; 1])?;

        // let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // Insert empty "Token Entry"
        apcb.insert_entry(EntryId::Token(TokenEntryId::Bool), 0, 1, ContextType::Tokens, 32, &[])?;

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(EntryId::Token(TokenEntryId::Bool), 0, 1, 0x014FBF20, 1)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Raw(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Token);
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
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;

        // Insert empty "Token Entry"
        match apcb.insert_entry(EntryId::Ccx(CcxEntryId::Raw(0)), 0, 1, ContextType::Tokens, 32, &[]) {
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
    fn insert_two_tokens() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::Raw(97)), 0, 0xFFFF, ContextType::Struct, 32, &[2u8; 1])?;

        // let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // Insert empty "Token Entry"
        // insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload: &[u8], priority_mask: u8
        apcb.insert_entry(EntryId::Token(TokenEntryId::Byte), 0, 1, ContextType::Tokens, 32, &[])?;

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 1, 0x014FBF20, 1)?;
        apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 1, 0x42, 2)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Raw(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Token);
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

    #[test]
    fn delete_tokens() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = Apcb::create(&mut buffer[0..], 42).unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::BoardIdGettingMethod), 0, 0xFFFF, ContextType::Struct, 33, &[1u8; 48])?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();
        apcb.insert_entry(EntryId::Psp(PspEntryId::Raw(97)), 0, 0xFFFF, ContextType::Struct, 32, &[2u8; 1])?;

        // let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // Insert empty "Token Entry"
        // insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, context_type: ContextType, payload: &[u8], priority_mask: u8
        apcb.insert_entry(EntryId::Token(TokenEntryId::Byte), 0, 1, ContextType::Tokens, 32, &[])?;

        let mut apcb = Apcb::load(&mut buffer[0..]).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16, instance_id: u16, board_instance_mask: u16, token_id: u32, token_value: u32) -> Result<()> {
        apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 1, 0x014FBF20, 1)?;
        apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 1, 0x42, 2)?;

        apcb.delete_token(EntryId::Token(TokenEntryId::Byte), 0, 1, 0x42)?;

        let apcb = Apcb::load(&mut buffer[0..]).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() ==*b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Raw(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Token);
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

}
