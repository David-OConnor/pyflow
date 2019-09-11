# Release Checklist

This is a list of steps to complete when making a new release.

1. Review the commit and PR history since last release. Ensure that all relevant
changes are included in `CHANGELOG.md`
1. Ensure  the readme and homepage website reflects API changes. This includes changing the download
links to reflect the latest version.
1. Ensure the version listed in `Cargo.toml` is updated
1. Update Rust tools: `rustup update`
1. Run `cargo test`, `cargo fmt` and `cargo clippy`
1. Commit and push the repo
1. Check that CI pipeline passed
1. Run `cargo package` and `cargo publish` (Allows installation via cargo)
1. Run `cargo build --release` on Windows and Linux (to build binaries)
1. Run `cargo deb` on Ubuntu 18.04 (perhaps earlier; for example, one built on 18.04
works on 19.04, but not vice-versa) (to build an installer)
1. Run `cargo wix` on Windows (to build an installer)
1. Add a release on [Github](https://github.com/David-OConnor/seed/releases), following the format of previous releases.
1. Upload the following binaries to the release page: Windows binary, Linux binary, Msi, Deb