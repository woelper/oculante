export MACOSX_DEPLOYMENT_TARGET=10.15
cargo bundle --release
# build universal binary
rustup target add aarch64-apple-darwin
cargo universal2
# copy universal binary into .app
cp target/universal2-apple-darwin/release/oculante target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
#cp res/info.plist target/debug/bundle/osx/oculante.app/Contents/Info.plist
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist