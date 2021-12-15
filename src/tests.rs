#[cfg(test)]
mod tests {
    use crate::ondisk::{
        memory::ConsoleOutControl, memory::DimmInfoSmbusElement,
        memory::ExtVoltageControl, psp::BoardIdGettingMethodEeprom,
        psp::IdRevApcbMapping, psp::RevAndFeatureValue, BaudRate,
        BoardInstances, CcxEntryId, ContextType, DfEntryId, EntryId, GroupId,
        MemoryEntryId, PriorityLevels, PspEntryId, TokenEntryId,
    };
    use crate::types::PriorityLevel;
    use crate::Apcb;
    use crate::ApcbIoOptions;
    use crate::EntryItemBody;
    use crate::{Error, FileSystemError};
    use core::default::Default;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        let groups = apcb.groups();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        let groups = apcb.groups();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
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
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
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
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.delete_group(GroupId::Psp)?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.delete_group(GroupId::Memory)?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error>
    {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        match apcb.delete_group(GroupId::Token) {
            Err(Error::GroupNotFound) => {}
            _ => {
                panic!("test failed")
            }
        }
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
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
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.delete_group(GroupId::Psp)?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let groups = apcb.groups();
        for _group in groups {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn delete_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();

        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[1u8; 48],
        )?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::Unknown(97)),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 48],
        )?;
        //let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.delete_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
        )?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Unknown(97)));

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn insert_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::Unknown(97)),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 4],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Unknown(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_struct_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        apcb.insert_struct_entry(
            EntryId::Memory(MemoryEntryId::ConsoleOutControl),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &ConsoleOutControl::default(),
            &[],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups_mut();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        assert!(matches!(entries.next(), None));

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries_mut();

        let mut entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(
            entry.id() == EntryId::Memory(MemoryEntryId::ConsoleOutControl)
        );
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let (console_out_control, _) =
            entry.body_as_struct_mut::<ConsoleOutControl>().unwrap();
        assert!(*console_out_control == ConsoleOutControl::default());
        if console_out_control
            .abl_console_out_control
            .enable_console_logging()?
        {
            console_out_control
                .abl_console_out_control
                .set_enable_console_logging(false);
        }

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_incompatible_struct_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        match apcb.insert_struct_entry(
            EntryId::Memory(MemoryEntryId::ConsoleOutControl),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &ExtVoltageControl::new_disabled(),
            &[],
        ) {
            Err(Error::EntryTypeMismatch) => Ok(()),
            _ => {
                panic!("should not be reached");
            }
        }
    }

    #[test]
    fn insert_headered_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        let header = BoardIdGettingMethodEeprom::new(1, 2, 3, 4);
        let items = [
            IdRevApcbMapping::new(5, 4, RevAndFeatureValue::Value(9), 3)
                .unwrap(),
            IdRevApcbMapping::new(8, 7, RevAndFeatureValue::Value(10), 6)
                .unwrap(),
        ];
        apcb.insert_struct_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &header,
            &items,
        )?;
        let control = ExtVoltageControl::default();
        apcb.insert_struct_entry(
            EntryId::Memory(MemoryEntryId::ExtVoltageControl),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &control,
            &[(), ()],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let (header, elements) = entry
            .body_as_struct::<BoardIdGettingMethodEeprom>()
            .ok_or_else(|| Error::EntryTypeMismatch)?;
        assert!(*header == BoardIdGettingMethodEeprom::new(1, 2, 3, 4));

        let mut elements = elements.iter();

        assert!(
            *elements.next().ok_or_else(|| Error::EntryTypeMismatch)?
                == IdRevApcbMapping::new(5, 4, RevAndFeatureValue::Value(9), 3)
                    .unwrap()
        );
        assert!(
            *elements.next().ok_or_else(|| Error::EntryTypeMismatch)?
                == IdRevApcbMapping::new(
                    8,
                    7,
                    RevAndFeatureValue::Value(10),
                    6
                )
                .unwrap()
        );
        assert!(matches!(elements.next(), None));

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        let (control, _) = entry
            .body_as_struct::<ExtVoltageControl>()
            .ok_or_else(|| Error::EntryTypeMismatch)?;
        assert!(*control == ExtVoltageControl::default());
        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        let items = [
            DimmInfoSmbusElement::new_slot(
                2,
                3,
                4,
                5,
                Some(6),
                Some(7),
                Some(8),
            )
            .unwrap(),
            DimmInfoSmbusElement::new_slot(
                10,
                11,
                12,
                13,
                Some(14),
                Some(15),
                Some(16),
            )
            .unwrap(),
        ];
        apcb.insert_struct_array_as_entry(
            EntryId::Memory(MemoryEntryId::DimmInfoSmbus),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &items,
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Memory(MemoryEntryId::DimmInfoSmbus));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        match entry.body {
            EntryItemBody::<_>::Struct(buf) => {
                assert_eq!(
                    *buf,
                    [1, 2, 3, 4, 5, 6, 7, 8, 1, 10, 11, 12, 13, 14, 15, 16]
                );
            }
            _ => {
                panic!("wrong thing");
            }
        }

        let elements = entry
            .body_as_struct_array::<DimmInfoSmbusElement>()
            .ok_or_else(|| Error::EntryTypeMismatch)?;
        let mut elements = elements.iter();

        assert_eq!(
            *elements.next().ok_or_else(|| Error::EntryTypeMismatch)?,
            DimmInfoSmbusElement::new_slot(
                2,
                3,
                4,
                5,
                Some(6),
                Some(7),
                Some(8)
            )
            .unwrap()
        );
        assert_eq!(
            *elements.next().ok_or_else(|| Error::EntryTypeMismatch)?,
            DimmInfoSmbusElement::new_slot(
                10,
                11,
                12,
                13,
                Some(14),
                Some(15),
                Some(16)
            )
            .unwrap()
        );
        assert!(matches!(elements.next(), None));

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_wrong_struct_array_entries() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        let items = [
            DimmInfoSmbusElement::new_slot(
                2,
                3,
                4,
                5,
                Some(6),
                Some(7),
                Some(8),
            )
            .unwrap(),
            DimmInfoSmbusElement::new_slot(
                10,
                11,
                12,
                13,
                Some(14),
                Some(15),
                Some(16),
            )
            .unwrap(),
        ];
        match apcb.insert_struct_array_as_entry(
            EntryId::Memory(MemoryEntryId::ConsoleOutControl),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &items,
        ) {
            Err(Error::EntryTypeMismatch) => Ok(()),
            _ => {
                panic!("should fail with EntryTypeMismatch");
            }
        }
    }

    #[test]
    fn insert_tokens() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Df, *b"DFG ")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        apcb.insert_entry(
            EntryId::Df(DfEntryId::SlinkConfig),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        apcb.insert_entry(
            EntryId::Df(DfEntryId::XgmiPhyOverride),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 1],
        )?;

        // Insert empty "Token Entry"
        apcb.insert_entry(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        )?;

        // pub(crate) fn insert_token(&mut self, entry_id: EntryId, instance_id:
        // u16, board_instance_mask: BoardInstances, token_id: u32, token_value:
        // u32) -> Result<()> {
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0xae46_cea4,
            2,
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups_mut();

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Df);
        assert!(group.signature() == *b"DFG ");

        let mut entries = group.entries_mut();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Df(DfEntryId::SlinkConfig));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Df(DfEntryId::XgmiPhyOverride));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        assert!(matches!(entries.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");
        for _entry in group.entries() {
            assert!(false);
        }

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Token);
        assert!(group.signature() == *b"TOKN");

        let mut entries = group.entries_mut();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Token(TokenEntryId::Byte));

        match entry.body {
            EntryItemBody::Tokens(ref tokens) => {
                let mut tokens = tokens.iter();

                let token =
                    tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0xae46_cea4);
                assert!(token.value() == 2);

                assert!(matches!(tokens.next(), None));
            }
            _ => panic!("no tokens"),
        }

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        let tokens = apcb
            .tokens(0, BoardInstances::from_instance(0).unwrap())
            .unwrap();
        assert!(tokens.abl_serial_baud_rate().unwrap() == BaudRate::_4800Baud);

        let mut tokens = apcb
            .tokens_mut(
                0,
                BoardInstances::from_instance(0).unwrap(),
                PriorityLevels::from_level(PriorityLevel::Normal),
            )
            .unwrap();
        let mut tokens = apcb
            .tokens_mut(
                0,
                BoardInstances::from_instance(0).unwrap(),
                PriorityLevels::from_level(PriorityLevel::Normal),
            )
            .unwrap();
        assert!(tokens.abl_serial_baud_rate().unwrap() == BaudRate::_4800Baud);
        tokens
            .set_abl_serial_baud_rate(BaudRate::_9600Baud)
            .unwrap();
        assert!(tokens.abl_serial_baud_rate().unwrap() == BaudRate::_9600Baud);
        Ok(())
    }

    #[test]
    fn insert_tokens_wrong() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Df, *b"DFG ")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        apcb.insert_entry(
            EntryId::Df(DfEntryId::SlinkConfig),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        apcb.insert_entry(
            EntryId::Df(DfEntryId::XgmiPhyOverride),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 1],
        )?;

        // Insert empty "Token Entry"
        apcb.insert_entry(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        )?;

        // pub(crate) fn insert_token(&mut self, entry_id: EntryId, instance_id:
        // u16, board_instance_mask: BoardInstances, token_id: u32, token_value:
        // u32) -> Result<()> {
        match apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::all(),
            0xae46_cea4,
            2,
        ) {
            Err(Error::EntryNotFound) => Ok(()),
            Err(e) => Err(e),
            _ => {
                panic!("should not succeed");
            }
        }
    }

    #[test]
    fn insert_tokens_easy() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::Unknown(97)),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 1],
        )?;

        // let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();

        // Insert empty "Token Entry"
        apcb.insert_entry(
            EntryId::Token(TokenEntryId::Bool),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16,
        // instance_id: u16, board_instance_mask: BoardInstances, token_id: u32,
        // token_value: u32) -> Result<()> {
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Bool),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x014FBF20,
            1,
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Unknown(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

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
        match entry.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let mut tokens = tokens.iter();
                let token =
                    tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x014FBF20);
                assert!(token.value() == 1);
                assert!(matches!(tokens.next(), None));
            }
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
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;

        // Insert empty "Token Entry"
        match apcb.insert_entry(
            EntryId::Ccx(CcxEntryId::Unknown(0)),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        ) {
            Ok(_) => {
                panic!("insert_entry should not succeed");
            }
            Err(Error::GroupNotFound) => Ok(()),
            Err(s) => Err(s),
        }
    }

    #[test]
    fn insert_two_tokens() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::Unknown(97)),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 1],
        )?;

        // let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();

        // Insert empty "Token Entry"
        // insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id:
        // u16, board_instance_mask: BoardInstances, context_type: ContextType,
        // payload: &[u8], priority_mask: u8
        apcb.insert_entry(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16,
        // instance_id: u16, board_instance_mask: BoardInstances, token_id: u32,
        // token_value: u32) -> Result<()> {
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x014FBF20,
            1,
        )?;
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x42,
            2,
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Unknown(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

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
        match entry.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let mut tokens = tokens.iter();

                let token =
                    tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x42);
                assert!(token.value() == 2);

                let token =
                    tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x014FBF20);
                assert!(token.value() == 1);

                assert!(matches!(tokens.next(), None));
            }
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
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_group(GroupId::Token, *b"TOKN")?;
        //let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        // makes it work let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::Unknown(97)),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[2u8; 1],
        )?;

        // let mut apcb = Apcb::load(&mut buffer[0..],
        // &ApcbIoOptions::default()).unwrap();

        // Insert empty "Token Entry"
        // insert_entry(&mut self, group_id: u16, entry_id: u16, instance_id:
        // u16, board_instance_mask: BoardInstances, context_type: ContextType,
        // payload: &[u8], priority_mask: u8
        apcb.insert_entry(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            ContextType::Tokens,
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        // pub(crate) fn insert_token(&mut self, group_id: u16, entry_id: u16,
        // instance_id: u16, board_instance_mask: BoardInstances, token_id: u32,
        // token_value: u32) -> Result<()> {
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x014FBF20,
            1,
        )?;
        apcb.insert_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x42,
            2,
        )?;

        apcb.delete_token(
            EntryId::Token(TokenEntryId::Byte),
            0,
            BoardInstances::from_instance(0).unwrap(),
            0x42,
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();

        let mut groups = apcb.groups();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::Unknown(97)));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

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
        match entry.body {
            EntryItemBody::<_>::Tokens(tokens) => {
                let mut tokens = tokens.iter();

                let token =
                    tokens.next().ok_or_else(|| Error::TokenNotFound)?;
                assert!(token.id() == 0x014FBF20);
                assert!(token.value() == 1);

                assert!(matches!(tokens.next(), None));
            }
            _ => {
                panic!("unexpected entry type");
            }
        }
        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_platform_specific_overrides() -> Result<(), Error> {
        use crate::memory::platform_specific_override::{
            ChannelIds, DimmSlots, DimmSlotsSelection, LvDimmForce1V5,
            MutElementRef, SocketIds, SolderedDownSodimm,
        };
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Psp, *b"PSPG")?;
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        apcb.insert_entry(
            EntryId::Psp(PspEntryId::BoardIdGettingMethod),
            0,
            BoardInstances::all(),
            ContextType::Struct,
            PriorityLevels::from_level(PriorityLevel::Low),
            &[1u8; 48],
        )?;
        apcb.insert_struct_sequence_as_entry(
            EntryId::Memory(MemoryEntryId::PlatformSpecificOverride),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[
                &LvDimmForce1V5::new(
                    SocketIds::ALL,
                    ChannelIds::Any,
                    DimmSlots::Any,
                ),
                &SolderedDownSodimm::new(
                    SocketIds::ALL,
                    ChannelIds::Any,
                    DimmSlots::Specific(
                        DimmSlotsSelection::new().with_dimm_slot_2(true),
                    ),
                ),
            ],
        )?;

        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups_mut();

        let group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Psp);
        assert!(group.signature() == *b"PSPG");

        let mut entries = group.entries();

        let entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(entry.id() == EntryId::Psp(PspEntryId::BoardIdGettingMethod));
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        assert!(matches!(entries.next(), None));

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries_mut();

        let mut entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(
            entry.id()
                == EntryId::Memory(MemoryEntryId::PlatformSpecificOverride)
        );
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let mut platform_specific_overrides = entry
            .body_as_struct_sequence_mut::<MutElementRef<'_>>()
            .unwrap();
        let platform_specific_overrides =
            platform_specific_overrides.iter_mut().unwrap();
        let mut lvdimm_count = 0;
        let mut sodimm_count = 0;
        for item in platform_specific_overrides {
            match item {
                MutElementRef::LvDimmForce1V5(item) => {
                    lvdimm_count += 1;
                    assert!(item.sockets().unwrap() == SocketIds::ALL);
                    assert!(item.channels().unwrap() == ChannelIds::Any);
                    //assert!(item.dimms().unwrap() == DimmSlots::Any);
                }
                MutElementRef::SolderedDownSodimm(item) => {
                    sodimm_count += 1;
                    assert!(item.sockets().unwrap() == SocketIds::ALL);
                    assert!(item.channels().unwrap() == ChannelIds::Any);
                    //assert!(item.dimms().unwrap() ==
                    // DimmSlots::Specific(DimmSlotsSelection::new().
                    // with_dimm_slot_2(true)));
                }
                _ => {
                    panic!(
                        "did not expect unknown elements in platform_specific_overrides ({:?})",
                        item
                    );
                }
            }
        }
        assert!(lvdimm_count == 1);
        assert!(sodimm_count == 1);

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn checksum_invalid() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut _apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        // Break checksum
        buffer[16] = buffer[16].wrapping_add(1);
        match Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()) {
            Ok(_) => {
                panic!("should not be reached");
            }
            Err(Error::FileSystem(
                FileSystemError::InconsistentHeader,
                "V2_HEADER::checksum_byte",
            )) => Ok(()),
            _ => {
                panic!("should not be reached");
            }
        }
    }

    #[test]
    fn insert_cad_bus_element() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        use crate::memory::{Ddr4DimmRanks, DdrRates, RdimmDdr4CadBusElement};
        let element = RdimmDdr4CadBusElement::new(
            2,
            DdrRates::new().with_ddr3200(true),
            Ddr4DimmRanks::new()
                .with_single_rank(true)
                .with_dual_rank(true),
            Ddr4DimmRanks::new()
                .with_single_rank(true)
                .with_dual_rank(true),
            0x2a2d2d,
        )
        .unwrap();
        apcb.insert_struct_array_as_entry(
            EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[element],
        )?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups_mut();

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries_mut();

        let mut entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(
            entry.id() == EntryId::Memory(MemoryEntryId::PsRdimmDdr4CadBus)
        );
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let mut items = entry
            .body_as_struct_array_mut::<RdimmDdr4CadBusElement>()
            .unwrap();
        let mut items = items.iter_mut();
        let item = items.next().ok_or_else(|| Error::EntryNotFound)?;

        assert!(item.dimm_slots_per_channel().unwrap() == 2);
        assert!(
            item.ddr_rates().unwrap() == DdrRates::new().with_ddr3200(true)
        );
        assert!(item.ddr_rates().unwrap() != DdrRates::new());
        assert!(
            item.dimm0_ranks().unwrap()
                == Ddr4DimmRanks::new()
                    .with_single_rank(true)
                    .with_dual_rank(true)
        );
        assert!(item.dimm0_ranks().unwrap() != Ddr4DimmRanks::new());
        assert!(
            item.dimm1_ranks().unwrap()
                == Ddr4DimmRanks::new()
                    .with_single_rank(true)
                    .with_dual_rank(true)
        );
        assert!(item.address_command_control().unwrap() == 0x2a2d2d);
        assert!(matches!(items.next(), None));

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn insert_data_bus_element() -> Result<(), Error> {
        let mut buffer: [u8; Apcb::MAX_SIZE] = [0xFF; Apcb::MAX_SIZE];
        let mut apcb =
            Apcb::create(&mut buffer[0..], 42, &ApcbIoOptions::default())
                .unwrap();
        apcb.insert_group(GroupId::Memory, *b"MEMG")?;
        use crate::memory::{
            Ddr4DataBusElement, Ddr4DimmRanks, DdrRates, RttNom, RttPark,
            RttWr, VrefDq, VrefDqRange1,
        };
        let element = Ddr4DataBusElement::new(
            2,
            DdrRates::new().with_ddr3200(true),
            Ddr4DimmRanks::new()
                .with_single_rank(true)
                .with_dual_rank(true),
            Ddr4DimmRanks::new()
                .with_single_rank(true)
                .with_dual_rank(true),
            RttNom::Off,
            RttWr::Off,
            RttPark::_48Ohm,
            91,
            VrefDq::Range1(VrefDqRange1::_74_95P),
        )
        .unwrap();
        apcb.insert_struct_array_as_entry(
            EntryId::Memory(MemoryEntryId::PsRdimmDdr4DataBus),
            0,
            BoardInstances::all(),
            PriorityLevels::from_level(PriorityLevel::Normal),
            &[element],
        )?;
        Apcb::update_checksum(&mut buffer[0..]).unwrap();
        let mut apcb =
            Apcb::load(&mut buffer[0..], &ApcbIoOptions::default()).unwrap();
        let mut groups = apcb.groups_mut();

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFound)?;
        assert!(group.id() == GroupId::Memory);
        assert!(group.signature() == *b"MEMG");

        let mut entries = group.entries_mut();

        let mut entry = entries.next().ok_or_else(|| Error::EntryNotFound)?;
        assert!(
            entry.id() == EntryId::Memory(MemoryEntryId::PsRdimmDdr4DataBus)
        );
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == BoardInstances::all());

        let mut items = entry
            .body_as_struct_array_mut::<Ddr4DataBusElement>()
            .unwrap();
        let mut items = items.iter_mut();
        let item = items.next().ok_or_else(|| Error::EntryNotFound)?;

        assert!(item.dimm_slots_per_channel().unwrap() == 2);
        assert!(
            item.ddr_rates().unwrap() == DdrRates::new().with_ddr3200(true)
        );
        assert!(
            item.dimm0_ranks().unwrap()
                == Ddr4DimmRanks::new()
                    .with_single_rank(true)
                    .with_dual_rank(true)
        );
        assert!(
            item.dimm1_ranks().unwrap()
                == Ddr4DimmRanks::new()
                    .with_single_rank(true)
                    .with_dual_rank(true)
        );
        assert!(item.rtt_nom().unwrap() == RttNom::Off);
        assert!(item.rtt_wr().unwrap() == RttWr::Off);
        assert!(item.rtt_park().unwrap() == RttPark::_48Ohm);
        assert!(item.pmu_phy_vref().unwrap() == 91);
        // TODO: assert!(item.vref_dq().unwrap().to_u64().unwrap() == 23);

        assert!(matches!(items.next(), None));

        assert!(matches!(entries.next(), None));

        assert!(matches!(groups.next(), None));
        Ok(())
    }
}
