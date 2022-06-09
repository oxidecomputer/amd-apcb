//! serde_yaml has a limitation such that from_str cannot borrow parts of the original string in the returned struct. That means that all the structures in question of amd-apcb have to implement DeserializeOwned, which means none of them are allowed to have lifetime parameters. If they had, the Rust compiler would emit a borrow checker error.
//! We did modify amd-apcb in this way recently, so it works now. However, it can potentially be silently broken by future changes to amd-apcb since the property is only ensured by convention.
//! Therefore, add an example that is built by the Makefile.

fn main() {
    let _foo: amd_apcb::Apcb = serde_yaml::from_str("").unwrap();
}
