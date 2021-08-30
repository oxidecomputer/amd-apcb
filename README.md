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

(Note: There are convenience functions that you can use to insert raw struct data: `insert_struct_entry`, `insert_struct_sequence_as_entry`)

To delete an entry:

    apcb.delete_entry(EntryId::Psp(PspEntryId::...), 0, 0xFFFF)?;

To insert a new token:

    apcb.insert_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42, 1)?;

To delete a token:

    apcb.delete_token(EntryId::Token(TokenEntryId::Byte), 0, 0xFFFF, 0x42)?;

If the entry is a struct entry, you can use something like

    let mut entry = entry.body_as_struct_mut::<memory::DimmInfoSmbus>()?;
    entry.dimm_slot_present

to have the entry represented as a Rust struct.  This is only useful for structs whose name doesn't contain "Element" (since those are the ones without a variable-length payload).

If the entry is a struct array entry (variable-length array), then you can use something like

    let mut entry = entry.body_as_struct_array_mut::<memory::DimmInfoSmbusElement>()?;
    for element in entry {
        ...
    }

to iterate over it.  This is only useful for structs whose name contains "Element".

In order to update the checksum (you should do that once after any insertion/deletion/mutation):

    Apcb::update_checksum(&mut buffer[0..])?;

Note that this also changes unique_apcb_instance.

AMD currently specifies a size limit of 0x2000 Byte (8 KiB) for the entire APCB.

# Testing

Run

    cargo test --features std
