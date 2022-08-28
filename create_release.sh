# This is a script to be locally run in order to release.
cargo install cargo-bump
cargo bump patch --git-tag
git add Cargo.toml
cargo update
cargo check
git add Cargo.lock
git commit -m "Bumped version"
git push --tags
git push
# this needs no-verify as we modify the plist during the build, and cargo does not accept that.
cargo publish --no-verify