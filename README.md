# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

Then

    let mut buffer: [u8; 8*1024] = ... load from file;
    let groups = APCB::load(&mut buffer[0..]).unwrap();
    for group in groups {
        for entry in group {
            ...
        }
    }

# Testing

Run

    cargo test --features std

# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualified, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.  Right now, using `zerocopy` crate instead--but it's very verbose and quirky.  It will be reassessed in the future whether `ssmarshal` is better after all.
