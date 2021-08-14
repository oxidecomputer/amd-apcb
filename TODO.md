# Features

* "array of raw structs" is (unfortunately) possible, so we need to support it
  * insert_struct_array with padding?!
  * Implement body_as_headered_struct_array_mut
* Apcb: Dirty-type, original-type; automate calling update_checksum
  * My own idea: Just implement Drop and have a flag you refer to.

# Cleanup

* Maybe get rid of Error::Internal entirely ?
* TokensEntryItemMut::set_value just asserts!!!  Maybe not so great.

# Security

* Sanity-check non-clone in body_as_struct_mut
* Add unit test for token entries!!  mutation
* insert_*: Check for duplicate key
* Fuzzing!
  * https://rust-fuzz.github.io/book/cargo-fuzz.html

# Unimportant/later

* apcb::insert_entry: Replace by shifts and masks (if not compile time)
* insert_token: "&" instead of "%"
* Check error handling crate "failure" or "anyhow". `#[source]`
* https://docs.rs/zerovec/0.2.3/zerovec/ ; also has serde serializers
  Also has https://docs.rs/zerovec/0.2.3/zerovec/ule/index.html
  https://docs.rs/zerovec/0.2.3/zerovec/ule/trait.ULE.html can handle arrays
* Entry alignment relative to containing group instead??
