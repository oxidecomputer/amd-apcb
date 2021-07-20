# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

To iterate, you can do:

    let mut buffer: [u8; 8*1024] = ... load from file;
    let apcb = APCB::load(&mut buffer[0..]).unwrap();
    for group in apcb.groups() {
        for entry in group.entries() {
            ...
        }
    }

To insert a new group:

    apcb.insert_group(0x1701, *b"PSPG")?;

To delete a group:

    apcb.delete_group(0x1701)?;

To insert a new entry:

    apcb.insert_entry(0x1701, 0x0000, 0, 0xFFFF, 0)?;

To delete an entry:

    apcb.delete_entry(0x1701, 0x0000, 0, 0xFFFF)?;

To insert a new token:

    apcb.insert_token(0x1701, 0x0000, 0, 0xFFFF, 0x42, 1)?;

To delete a token:

    apcb.delete_token(0x1701, 0x0000, 0, 0xFFFF, 0x42)?;

In order to update the checksum (you should do that after any insertion/deletion/mutation):

    Apcb::update_checksum(&mut buffer[0..])?;

# Testing

Run

    cargo test --features std
