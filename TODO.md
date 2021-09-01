# Features

* AMD# 55483 6.3 AGESA Boot Loader Debug
* Sanity-check "new" fns that exist
* Add "new" fn for the others, too?
* AMD# 55483 4.1.7 ABL Error Reporting Configuration Items
* AMD# 55483 PTO 4.1.5.6 UMC Category
* TX EQ struct; bitfield for sockets; bitfield for dies; bitfield for lanes; lane data (variable-length body!)
* body_as_struct_sequence; body_as_struct_array: Where is the check whether it is EntryCompatible ?
* Convert board_instance_mask to bitfield?!
* Make "weird" struct array iterator (sequence of different types iterator)
* IdRevApcbMapping
  * rev_and_feature_value: 0xff for NA
  * id_and_feature_mask: bit 7: 1=user controlled; 0=normal
* Add test to insert wrong-type struct using insert_struct*entry
* Check checksum on load?
* Apcb: Dirty-type, original-type; automate calling update_checksum
  * My own idea: Just implement Drop and have a flag you refer to.
  * Give a reference to the flag to all the iterators that would need to change it
    * If there are &mut to struct that doesn't work, now does it?
* OdtPatElement: Availability of dimm0_rank, dimm1_rank should be conditional.
* Enums for stuff with a "|" or "one of" comment.
  * ddr_rates (bitfield done; need unit tests!)
  * ref_dq
  * a lot of platform_specific_override::* enums
    * Cs
* rustdoc

# Accessor tests

* bitfield out-of-range
* struct field out-of-range
* bitfield in-range
* struct field in-range

# (Future) Compatibility

* Allow/implement insert_headered_struct_array_entry with padding?!  Check what AMD says here
* Also make_accessors! the ENTRY_HEADER, GROUP_HEADER, V2_HEADER and so on
* Maybe remove PartialEq from structs
* Maybe remove "EventControl" id

# Security

* Sanity-check non-clone in body_as_struct_mut
* Add unit test for token entries!!  mutation
* insert_*: Check for duplicate key
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

# Alternate Bitfield implementations

* Most important would be to partially evaluate at compile time!
* https://crates.io/crates/modular-bitfield
