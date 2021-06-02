# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualifi
ed, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.

# Observations

APCB V3 TOKEN type_id are NOT unique per group.  Maybe unique (board_mask, type_id)--maybe not.
The other APCB elements have unique type_id.
