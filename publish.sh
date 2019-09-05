cargo build --release
cargo package
cargo publish
cargo deb

# On Windows:
# cargo build --release
# cargo wix