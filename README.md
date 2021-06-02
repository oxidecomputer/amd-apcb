# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), either directly on flash or in an image file.
# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = ["no_std"] }

to the `[dependencies]` block in your `Cargo.toml`.

# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualified, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.
