# Release Checklist

This is a list of steps to complete when making a new release.

1. Review the commit and PR history since last release. Ensure that all relevant
changes are included in `CHANGELOG.md`
1. Ensure  the readme and homepage website reflects API changes. This includes changing the download
links to reflect the latest version.
1. Run `python update_version.py v.v.v`.
1. Update Rust tools: `rustup update`
1. Run `cargo test`, `cargo fmt`
1. Run `cargo clippy -- -W clippy::pedantic -W clippy::nursery -W clippy::cargo`
1. Commit and push the repo
1. Check that CI pipeline passed
1. Run `cargo package` and `cargo publish` on Ubuntu 16.04 (Allows installation via cargo)
1. Run `cargo build --release` on Windows and Ubuntu 16.04 (to build standalone binaries)
1. Run `cargo deb` on Ubuntu 16.04 (one built on 18.04
works on 19.04, but not vice-versa)
1. Run `cargo rpm build` on Centos 7. (This allows easy installation for Red Hat, Fedora, and CentOs
users, and binaries built on other OSes appear not to work on these due to OpenSSL issues.
1. Run `cargo wix` on Windows
1. Zip the Windows `.exe`, along with `README.md` and `LICENSE`.
1. Change the name in `Cargo.toml` on Windows to `pflow`. Run `maturin build` on Windows.
rename the executable (`data subfolder`) to `pyflow[.exe]`
1. On Ubuntu 16.04, do the same, and also swap comments on the `reqwest` dependency.
1. For both wheels, Run `twine upload (wheelname)`.
1. Run `snapcraft`, `snapcraft login`, then
`snapcraft push --release=stable pyflow_x.x.x_amd64.snap` on Ubuntu.
1. Add a release on [Github](https://github.com/David-OConnor/seed/releases), following the format of previous releases.
1. Upload the following binaries to the release page: zipped Windows binary, Linux binary, Msi, Deb, Rpm.
