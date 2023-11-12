export MACOSX_DEPLOYMENT_TARGET=10.15
cargo install cargo-bundle
cargo bundle --release
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
brew install nasm
# arch -x86_64 brew install nasm

cargo build --release --target x86_64-apple-darwin --features heif
cargo build --release --target aarch64-apple-darwin --features heif
lipo -create -output target/release/bundle/osx/oculante.app/Contents/MacOS/oculante x86_64-apple-darwin/release/oculante aarch64-apple-darwin/release/oculante 
file target/release/bundle/osx/oculante.app/Contents/MacOS/oculante

# build universal binary
# cargo universal2
# copy universal binary into .app
# cp target/universal2-apple-darwin/release/oculante target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
#cp res/info.plist target/debug/bundle/osx/oculante.app/Contents/Info.plist
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist



