# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualifi
ed, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.

# Observations

APCB V3 TOKEN type_id are NOT unique per group.  Maybe unique (board_mask, type_id)--maybe not.
The other APCB elements have unique type_id.

# Modification actions that are supported

* Creating a new group
* Deleting a group from APCB
* Resizing a group
  * Inserting entry into group
  * Deleting entry from group
* Querying/modifying existing tokens in entry (by token_id and entry type (Bool, DWord etc))
  * Hardcode and check: unit_size = 8

# Modification actions that need to be supported

* Creating a new group; order doesn't matter--although it's usually ascending by group_id (TODO)
* Growing an existing entry; especially for adding tokens (which have to be sorted) (TODO)
* Adding/REMOVEing tokens in entry (by token_id and entry type (Bool, DWord etc)) (TODO)
  * Hardcode and check: unit_size = 8, key_size = 4, key_pos = 0

# Limitations

In order to keep the sort order the same, the key of an existing entry cannot be changed. That includes:

* group_id of existing groups (TODO: Make group header read-only)
* (group_id, type_id, instance_id, board_instance_mask) of existing entries (TODO: Make entry header read-only)
* token_id of existing tokens
