# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob), directly in an image (u8 slice).

# Usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

For details, see the generated documentation.

# Testing

Run

    cargo xtask test
