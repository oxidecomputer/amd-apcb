# Features

* Maybe remove DimmInfoSmbusElement::not_present()
* Sanity-check "new" fns that exist
* Add "new" fn for the others, too?
* Convert board_instance_mask to bitfield?!
* IdRevApcbMapping
  * id_and_feature_mask: bit 7: 1=user controlled; 0=normal
* OdtPatElement: Availability of dimm0_rank, dimm1_rank should be conditional.
* Enums for stuff with a "|" or "one of" comment.
  * ref_dq
  * a lot of platform_specific_override::* enums
    * Cs

# Bugs

* Check delete_entry (postcondition!)
* Check delete_token (postcondition!)
  * Range end index of out range for delete_entry

# Tests

* Add unit test for token entries!!  mutation
* Add test to insert wrong-type struct using insert_struct*entry
* bitfield out-of-range
* struct field out-of-range
* bitfield in-range
* struct field in-range

# (Future) Compatibility

* Allow/implement insert_headered_struct_array_entry with padding?!  Check what AMD says here
* insert_struct_sequence_as_entry, insert_struct_array_as_entry: Automatically pad with 0es until size is aligned (to be bug-compatible with AMD)
* Also make_accessors! the ENTRY_HEADER, GROUP_HEADER, V2_HEADER and so on
* Maybe remove PartialEq from structs

# Security

* Sanity-check non-clone in body_as_struct_mut
* Check for unique key on load
  * Check globally unique (group_id) on load.
  * Check globally unique (group_id, entry_id, instance_id, board_instance_mask) on load.
  * Check globally-unique (group_id, instance_id, token_id) on load.
    * insert_token: group_id==Token.  entry_id should be datatype-dependent, so only one makes sense per token_id anyway--so entry_id is not part of the unique key.
* Fuzzing!
  * https://rust-fuzz.github.io/book/cargo-fuzz.html
  * Fuzz after:
    * apcb header
    * group header
    * entry header
    * token header

# Unimportant/later

* Make builder pattern constructors
* apcb::insert_entry: Replace by shifts and masks (if not compile time)
* insert_token: "&" instead of "%"
* Check error handling crate "failure" or "anyhow". `#[source]`
* Entry alignment relative to containing group instead??
* AMD# 55483 4.1.3 SPD_Info DIMM_INFO_ARRAY does not seem to exist
* TX EQ struct; bitfield for sockets; bitfield for dies; bitfield for lanes; lane data (variable-length body!)
* Apcb: Dirty-type, original-type; automate calling update_checksum
  * My own idea: Just implement Drop and have a flag you refer to.
  * Give a reference to the flag to all the iterators that would need to change it
    * If there are &mut to struct that doesn't work, now does it?
* insert_struct_sequence_entry(EntryId::Quux).with(A {p : 1}).with(B {q: 42}).finish() (You can do pretty complex construction of structures using that pattern, debug_struct in std is a good example)
  * https://doc.rust-lang.org/std/fmt/struct.Formatter.html#method.debug_struct
* Move skip_step from EntryCompatible to SequenceElementFromBytes (right now, the latter is only implemented by the enum macro; but skip_step would also be implemented by the enum--but outside the macro.  That's too complicated)
* Maybe remove "EventControl" id

# Alternate Bitfield implementations

* Most important would be to partially evaluate at compile time!
* bitstruct
