# Purpose

This crate allows you to manipulate APCB (AGESA PSP Configuration Blob),
directly in an image (u8 slice).

# Usage

## Full-featured usage

Add

    amd-apcb = { path = "../amd-apcb", features = ["serde", "schemars"] }

to the `[dependencies]` block in your `Cargo.toml`.

This way, you get JSON serialization and JSON schema support.
That means when you have an `Apcb` instance in variable `apcb`, you can
do `serde_json::to_string_pretty(&apcb)` to get matching JSON out.
Likewise, you can also deserialize from JSON into a new Apcb instance
(using `serde_json::from_str`, for example).

Enabling these features slightly changes the signature of some functions (like
`Apcb::load`) to take copy-on-write buffers (in order to allow
deserialization).

In order to load an existing blob, do this:

    let mut apcb = Apcb::load(std::borrow::Cow::Borrowed(&mut buffer[..]),
                              &ApcbIoOptions::default())?

For details, see the generated documentation.

## Minimal usage

Add

    amd-apcb = { path = "../amd-apcb", default_features = false, features = [] }

to the `[dependencies]` block in your `Cargo.toml`.

This gives you a minimal interface without serialization support. It's
intended for embedded use.

In order to load an existing blob, do this:

    let mut apcb = Apcb::load(&mut buffer[..], &ApcbIoOptions::default())?

There are (about four) groups in the blob. Inside each group there is a
variable number of entries. There are a few different entry types for
different purposes.

You can use

    apcb.groups()? // or apcb.groups_mut()?

to iterate over the groups.

Alternatively, you can use

    apcb.group(GroupId::xxx)? // or apcb.group_mut(GroupId::xxx)?

to immediately get a specific group (a useful example for `xxx` is `Memory`).

When you have a group in variable `group`, you can use

    group.entries() // or group.entries_mut()

to iterate over the entries of that group. Alternatively, you can use

    group.entry_compatible(EntryId::xxx, instance_id, BoardInstances::yyy)

in order to get an entry compatible with the given instance and boards.

Alternatively, you can use

    group.entry_exact(EntryId::xxx, instance_id, BoardInstances::yyy)

in order to get an entry that is exactly specified for the given instance and
boards.

When you have an entry in variable `entry`, you can use

    entry.body_as_struct::<X>()

in order to interpret the entry as the given struct `X`. A useful example for
`X` is `ErrorOutControl116`.

In order to interpret the entry as an array of a given struct `X`, you can do:

    entry.body_as_struct_array::<X>()

In order to interpret the entry as an array of differently-sized records
(collected into the enum `X`), you can do:

    entry.body_as_struct_sequence:<X>()

But on a modern AMD platform, the most useful entries are token entries:

In order to get an interface to the the token entries in a user-friendly form,
you can use:

    let tokens = apcb.tokens(instance_id, BoardInstances::new())?

It's then possible to use a lot of different getters and setters (one each
per token) in order to access the respective token. For example, you can do
`tokens.abl_serial_baud_rate()` to get the baud rate for the serial port
and/or `tokens.set_abl_serial_baud_rate(BaudRate::_9600Baud)` in order to
set the baud rate for the serial port.

For more dynamic access, you can do `tokens.get(entry_id, token_id)` directly,
where `entry_id: u16` and `token_id: u32`.

For details, see the generated documentation.

# Testing

Run

    cargo xtask test
