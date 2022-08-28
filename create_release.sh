# This is a script to be locally run in order to release.
cargo install cargo-bump
cargo install cargo-get
cargo bump patch --git-tag
git add Cargo.toml
git add Cargo.lock
cargo check
git commit -m "Release version `cargo get version`"
git push --tags
git push
# this needs no-verify as we modify the plist during the build, and cargo does not accept that.
cargo publish --no-verify