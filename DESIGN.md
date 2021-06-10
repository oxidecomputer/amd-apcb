# Implementation design decisions

* It's meant to run in a no_std, no_alloc environment.  That means that almost all serialization crates are disqualifi
ed, leaving only `ssmarshal` and maybe `packed_struct`.  `binread` would be otherwise nice, but it is `alloc`.

# Observations

APCB V3 TOKEN type_id are NOT unique per group.  Maybe unique (board_mask, type_id)--maybe not.
The other APCB elements have unique type_id.

# Modification actions that need to be supported

* Creating a new group; order doesn't matter--although it's usually ascending by group_id (TODO)
* Inserting an entry into a group (type_id alone DOES NOT have to be unique); order doesn't matter--although it's usually ascending by (type_id, board_mask) (TODO)
* Growing an existing entry; especially for adding tokens (which have to be sorted) (TODO)
* Querying/adding/REMOVEing tokens in entry (by token_id and entry type (Bool, DWord etc)) (TODO)
  * Hardcode and check: unit_size = 8, key_size = 4, key_pos = 0
* Deleteing group from APCB (TODO)
* Deleteing entry from group (TODO)

# Limitations

In order to keep the sort order the same, the key of an existing entry cannot be changed. That includes:

* group_id of existing groups (TODO: Make group header read-only)
* type_id and board_mask of existing entries (TODO: Make entry header read-only)
* token_id of existing tokens (TODO)
