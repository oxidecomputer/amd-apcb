#[cfg(test)]
mod tests {
    use crate::APCB;
    use crate::Error;
    use crate::ondisk::ContextType;

    #[test]
    #[should_panic]
    fn load_garbage_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        APCB::load(&mut buffer[0..]).unwrap();
    }

    #[test]
    fn create_empty_image() {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let apcb = APCB::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _item in groups {
            assert!(false);
        }
    }

    #[test]
    #[should_panic]
    fn create_empty_too_small_image() {
        let mut buffer: [u8; 1] = [0];
        let apcb = APCB::create(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _ in groups {
            assert!(false);
        }
    }

    #[test]
    fn create_image_with_one_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
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
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() == *b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() == *b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_first_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1701)?;
        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_second_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x1704)?;
        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_two_groups_delete_unknown_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        apcb.delete_group(0x4711)?;
        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        assert!(matches!(groups.next(), None));
        Ok(())
    }

    #[test]
    fn create_image_with_group_delete_group() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.delete_group(0x1701)?;
        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let groups = apcb.groups();
        for _group in groups {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn delete_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, 48)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.delete_entry(0x1701, 96, 0, 0xFFFF)?;
        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();
        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");
        for _entry in group {
            assert!(false);
        }

        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn insert_entries() -> Result<(), Error> {
        let mut buffer: [u8; 8 * 1024] = [0xFF; 8 * 1024];
        let mut apcb = APCB::create(&mut buffer[0..]).unwrap();
        apcb.insert_group(0x1701, *b"PSPG")?;
        apcb.insert_group(0x1704, *b"MEMG")?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 96, 0, 0xFFFF, ContextType::Struct, 48)?;
        let mut apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups_mut();
        groups.insert_entry(0x1701, 97, 0, 0xFFFF, ContextType::Struct, 1)?;

        let apcb = APCB::load(&mut buffer[0..]).unwrap();
        let mut groups = apcb.groups();

        let mut group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1701);
        assert!(group.signature() ==*b"PSPG");

        let entry = group.next().ok_or_else(|| Error::EntryNotFoundError)?;
        assert!(entry.id() == 96);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        let entry = group.next().ok_or_else(|| Error::EntryNotFoundError)?;
        assert!(entry.id() == 97);
        assert!(entry.instance_id() == 0);
        assert!(entry.board_instance_mask() == 0xFFFF);

        assert!(matches!(group.next(), None));

        let group = groups.next().ok_or_else(|| Error::GroupNotFoundError)?;
        assert!(group.id() == 0x1704);
        assert!(group.signature() ==*b"MEMG");
        for _entry in group {
            assert!(false);
        }

        assert!(matches!(groups.next(), None));
        Ok(())
    }
}