cargo install cargo-bump
cargo bump minor --git-tag
#VERSION=`cargo pkgid | cut -d# -f2 | cut -d: -f2`
#echo $VERSION
git add Cargo.toml