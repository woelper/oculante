cargo install cargo-bump
cargo bump patch --git-tag
#VERSION=`cargo pkgid | cut -d# -f2 | cut -d: -f2`
#echo $VERSION
git add Cargo.toml
cargo update
cargo check
git add Cargo.lock
git push --tags
git push