# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

To iterate, you can do:

    let mut buffer: [u8; 8*1024] = ... load from file;
    let apcb = Apcb::load(&mut buffer[0..]).unwrap();
    for group in apcb.groups() {
        for entry in group.entries() {
            ...
        }
    }

To insert a new group:

    apcb.insert_group(GroupId::Psp, *b"PSPG")?;

To delete a group:

    apcb.delete_group(GroupId::Psp)?;

To insert a new entry:

    apcb.insert_entry(EntryId::Psp(PspEntryId::...), 0, 0xFFFF, 0)?;

To delete an entry:

    apcb.delete_entry(EntryId::Psp(PspEntryId::...), 0, 0xFFFF)?;

To insert a new token:

    apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42, 1)?;

To delete a token:

    apcb.delete_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42)?;

In order to update the checksum (you should do that once after any insertion/deletion/mutation):

    Apcb::update_checksum(&mut buffer[0..])?;

Note that this also changes unique_apcb_instance.

If the entry is a struct entry, you can use something like

    let entry = entry.body_as_struct::<memory::DimmInfoSmbus>(0, 0xFFFF)?;
    entry.dimm_slot_present

in order to have the entry represented as a Rust struct.

# Testing

Run

    cargo test --features std
