# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

To iterate, you can do:

    let mut buffer: [u8; 8*1024] = ... load from file;
    let apcb = APCB::load(&mut buffer[0..]).unwrap();
    for group in apcb {
        for entry in group {
            ...
        }
    }

To insert a new group:

    apcb.insert_group(0x1701, *b"PSPG")?;

To delete a group:

    apcb.delete_group(0x1701)?;

To insert a new entry:

    apcb.insert_entry(0x1701, 0x0000, 0, 0xFFFF)?;

To delete an entry:

    apcb.delete_entry(0x1701, 0xFFFF)?;

Note that all the mutators also move the iterator--so you might want to load the APCB anew before iterating.

# Testing

Run

    cargo test --features std

# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualified, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.  Right now, using `zerocopy` crate instead--but it's very verbose and quirky.  It will be reassessed in the future whether `ssmarshal` is better after all.
