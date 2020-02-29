# Release Checklist

This is a list of steps to complete when making a new release.

System prereqs:
- Ubuntu 16.04: `sudo apt update`, `sudo apt install build-essential`, 
`pip3 install maturin twine`, `cargo install cargo-deb`, `sudo apt install snapcraft`.
- Centos 7: `yum update`, `yum install gcc gcc-c++ make`,
 `cargo install cargo-rpm`, `yum install rpm-build`.
- Windows: Install Visual Studio Community, and Wix. `cargo install cargo-wix`

`Ubuntu` below shall refer to Ubuntu 16.04. Builds tend to be more foward-compatible
than backwards.

## Preliminary
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

## Build binaries
1. Run `cargo build --release` on Windows and Ubuntu.
1. Run `cargo deb` on Ubuntu.
1. Run `cargo rpm build` on Centos 7. Remove the unecessary `-1` in the filename.
 (This allows easy installation for Red Hat, Fedora, and CentOs.
Also note that the standalone Linux binary may not work on these distros.)
users, and binaries built on other OSes appear not to work on these due to OpenSSL issues.
1. Run `cargo wix` on Windows.
1. Zip the Windows `.exe`, along with `README.md` and `LICENSE`.
1. Run `maturin build` on Windows and Ubuntu.
1. Run `snapcraft` on Ubuntu.

## Publish binaries
1. Run `cargo package` and `cargo publish` (Any os).
1. Run `snapcraft login`, then `snapcraft push --release=stable pyflow_x.x.x_amd64.snap` on Ubuntu.
1. For the Windows and Ubuntu wheels, run `twine upload (wheelname)`.
1. Add a release on [Github](https://github.com/David-OConnor/seed/releases), following the format of previous releases.
1. Upload the following binaries to the release page: zipped Windows binary (This is all `Scoop` needs),
 Linux binary, Msi, Deb, Rpm.
