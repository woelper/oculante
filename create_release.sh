# This is a script to be locally run in order to release.
cargo install cargo-bump
cargo install cargo-get
cargo bump patch
cargo build
git add Cargo.toml
git add Cargo.lock
git commit -m "Release version `cargo get version`"
# tag the commit with current version
git tag `cargo get version`
# create changelog
kokai release --ref `cargo get version` > tmp
cat CHANGELOG.md >> tmp
mv tmp CHANGELOG.md
git add CHANGELOG.md
git commit -m "Update changelog for `cargo get version`"
git push --tags
git push
# this needs no-verify as we modify the plist during the build, and cargo does not accept that.
cargo publish --no-verify