# Builds the app bundle for Intel Macs.
# Used by the release workflow and by the workflow that tests signing.
set -e

cd $(git rev-parse --show-toplevel)

rustc --version --verbose
cargo install cargo-bundle
cargo bundle --release
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist

otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
# The app must not depend on anything from Homebrew (#810)
if otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante | grep /usr/local/; then
    echo "The app links a library from Homebrew"
    exit 1
fi
