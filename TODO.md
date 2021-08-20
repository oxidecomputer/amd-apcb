# Features

* Check checksum on load?
* Apcb: Dirty-type, original-type; automate calling update_checksum
  * My own idea: Just implement Drop and have a flag you refer to.
  * Give a reference to the flag to all the iterators that would need to change it
    * If there are &mut to struct that doesn't work, now does it?
* OdtPatElement: Availability of dimm0_rank, dimm1_rank should be conditional.
* Enums for stuff with a "|" or "one of" comment.
  * ddr_rate
  * dimm ranks
  * dimm rank
  * rtt_nom
  * rtt_wr
  * rtt_park
  * dq_vref
  * port size
* rustdoc

# (Future) Compatibility

* Fix dimm_rank_bitmap (make it private; provide accessors)
* Allow/implement insert_headered_struct_array_entry with padding?!  Check what AMD says here
* Also make_accessors! the ENTRY_HEADER, GROUP_HEADER, V2_HEADER and so on
* Maybe remove PartialEq from structs

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

* Add "new" functions to the structs.  That's nicer than Default.
* apcb::insert_entry: Replace by shifts and masks (if not compile time)
* insert_token: "&" instead of "%"
* Check error handling crate "failure" or "anyhow". `#[source]`
* Entry alignment relative to containing group instead??

# Alternate Bitfield implementations

* Most important would be to partially evaluate at compile time!
* https://crates.io/crates/modular-bitfield
