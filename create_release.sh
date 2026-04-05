# This is a script to be locally run in order to release.

# deny releasing from non-main branches
branch="$(git symbolic-ref --short HEAD)"
if [ $branch != "master" ]
then
    echo "You must be on master branch"
    exit
fi

echo "You are on $branch, releasing!"
cargo install cargo-bump
cargo install cargo-get
cargo check --no-default-features --features notan/shaderc
cargo test shortcuts
cargo bump patch
cargo build
cargo test flathub
VERSION=$(cargo pkgid | cut -d# -f2 | cut -d: -f2)
git add README.md
git add Cargo.toml
git add Cargo.lock
git add PKGBUILD
git add res/flathub/io.github.woelper.Oculante.metainfo.xml
echo "\# $VERSION" > tmp
kokai release --ref HEAD | grep -Ev '^(# HEAD)' >> tmp
cat CHANGELOG.md >> tmp
mv tmp CHANGELOG.md
git add CHANGELOG.md
# tag the commit with current version
git commit -m "Release version $VERSION"
git tag $VERSION
git push --tags
git push
# this needs no-verify as we modify the plist during the build, and cargo does not accept that.
cargo publish --no-verify
