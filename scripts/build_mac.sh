# Builds the app bundle for Apple Silicon Macs.
# Used by the release workflow and by the workflow that tests signing.
set -e

cd $(git rev-parse --show-toplevel)

rustc --version --verbose
export MACOSX_DEPLOYMENT_TARGET=10.15
cargo install cargo-bundle --quiet
# for the assembly in libjpeg-turbo and the AV1 decoder
brew install nasm --quiet

cargo bundle --release
file target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist

otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
# The app must not depend on anything from Homebrew, it would crash on
# machines without it (#810).
if otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante | grep /opt/homebrew; then
    echo "The app links a library from Homebrew"
    exit 1
fi
