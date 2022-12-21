// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//!
//! Build driver for pico host boot loader.
//!
use clap;
use duct::cmd;
use std::env;
use std::path::Path;
use std::process;

/// BuildProfile defines whether we build in release or
/// debug mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    /// Returns a new BuildProfile constructed from the
    /// given args.
    fn new(matches: &clap::ArgMatches) -> BuildProfile {
        if matches.contains_id("release") {
            BuildProfile::Release
        } else {
            BuildProfile::Debug
        }
    }

    /// Returns the subdirectory component corresponding
    /// to the build type.
    fn _dir(self) -> &'static Path {
        Path::new(match self {
            Self::Debug => "debug",
            Self::Release => "release",
        })
    }

    /// Yields the appropriate cargo argument for the given
    /// build profile.
    fn build_type(self) -> Option<&'static str> {
        match self {
            Self::Release => Some("--release"),
            Self::Debug => None,
        }
    }
}

/// Build arguments including path to the compressed
/// cpio archive we use as a "ramdisk
#[derive(Clone, Debug)]
struct BuildArgs {
    profile: BuildProfile,
}

impl BuildArgs {
    /// Extracts the build profile type from the given matched
    /// arguments.  Debug is the default.
    fn new(matches: &clap::ArgMatches) -> BuildArgs {
        let profile = BuildProfile::new(matches);
        BuildArgs { profile }
    }
}

fn main() {
    let matches = parse_args();
    match matches.subcommand() {
        Some(("build", m)) => build(BuildArgs::new(m), m.contains_id("locked")),
        Some(("test", m)) => {
            test(BuildProfile::new(m), m.contains_id("locked"))
        }
        Some(("expand", _m)) => expand(),
        Some(("clippy", m)) => clippy(m.contains_id("locked")),
        Some(("clean", _m)) => clean(),
        _ => {
            println!("Unknown command");
            process::exit(1);
        }
    }
}

/// Parse program arguments and return the match structure.
fn parse_args() -> clap::ArgMatches {
    clap::Command::new("xtask")
        .version("0.1.0")
        .author("Oxide Computer Company")
        .about("xtask build tool")
        .subcommand(
            clap::Command::new("build").about("Builds").args(&[
                clap::arg!(--locked "Build locked to Cargo.lock"),
                clap::arg!(--release "Build optimized version")
                    .conflicts_with("debug"),
                clap::arg!(--debug "Build debug version (default)")
                    .conflicts_with("release"),
            ]),
        )
        .subcommand(
            clap::Command::new("test").about("Run unit tests").args(&[
                clap::arg!(--locked "Build or test locked to Cargo.lock"),
                clap::arg!(--release "Test optimized version")
                    .conflicts_with("debug"),
                clap::arg!(--debug "Test debug version (default)")
                    .conflicts_with("release"),
            ]),
        )
        .subcommand(clap::Command::new("expand").about("Expand macros"))
        .subcommand(
            clap::Command::new("clippy")
                .about("Run cargo clippy linter")
                .args(&[clap::arg!(--locked "Lint locked to Cargo.lock")]),
        )
        .subcommand(clap::Command::new("clean").about("cargo clean"))
        .get_matches()
}

/// Runs a cross-compiled build.
fn build(args: BuildArgs, with_locked: bool) {
    let build_type = args.profile.build_type().unwrap_or("");
    let locked = with_locked.then_some("--locked").unwrap_or("");
    let args = format!(
        "build {locked} {build_type}"
    );
    cmd(cargo(), args.split_whitespace()).run().expect("build successful");
}

/// Runs tests.
fn test(profile: BuildProfile, with_locked: bool) {
    let build_type = profile.build_type().unwrap_or("");
    let locked = with_locked.then_some("--locked").unwrap_or("");
    let args = format!("test {locked} {build_type} --tests --lib");
    cmd(cargo(), args.split_whitespace()).run().expect("test successful");
    let args = format!("build {locked} {build_type} --features serde");
    cmd(cargo(), args.split_whitespace()).run().expect("test successful");
    let args = format!("build {locked} {build_type} --features serde,schemars,serde-hex");
    cmd(cargo(), args.split_whitespace()).run().expect("test successful");
    let args = format!("build {locked} {build_type} --features serde,schemars --example fromyaml");
    cmd(cargo(), args.split_whitespace()).run().expect("test successful");
    let args = format!("test {locked} {build_type} --test * --features serde,schemars");
    cmd(cargo(), args.split_whitespace()).run().expect("test successful");
}

/// Expands macros.
fn expand() {
    cmd!(cargo(), "rustc", "--", "-Zunpretty=expanded")
        .run()
        .expect("expand successful");
}

/// Runs the Clippy linter.
fn clippy(with_locked: bool) {
    let locked = with_locked.then_some("--locked").unwrap_or("");
    let args = format!("clippy {locked}");
    cmd(cargo(), args.split_whitespace()).run().expect("clippy successful");
}

/// Runs clean on the project.
fn clean() {
    cmd!(cargo(), "clean").run().expect("clean successful");
}

/// Returns the value of the given environment variable,
/// or the default if unspecified.
fn env_or(var: &str, default: &str) -> String {
    env::var(var).unwrap_or(default.into())
}

/// Returns the name of the cargo binary.
fn cargo() -> String {
    env_or("CARGO", "cargo")
}
