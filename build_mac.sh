
rustup target list | grep installed
TOOLCHAIN=$(rustc --version --verbose | grep host | cut -f2 -d":" | tr -d "[:space:]")
echo we are using $TOOLCHAIN
export MACOSX_DEPLOYMENT_TARGET=10.15
cargo install cargo-bundle
cargo bundle --release --features notan/shaderc
mkdir target/release/bundle/osx/oculante.app/Contents/Frameworks/
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
brew install nasm
brew install libheif
cp /opt/homebrew/opt/libheif/lib/libheif.1.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
# arch -x86_64 brew install nasm





cargo build --release --target aarch64-apple-darwin --features notan/shaderc --features heif
install_name_tool -change /opt/homebrew/opt/libheif/lib/libheif.1.dylib "@executable_path/../Frameworks/libheif.1.dylib" target/aarch64-apple-darwin/release/oculante
# install_name_tool -change /opt/homebrew/opt/libheif/lib/libheif.1.dylib "@executable_path/../Frameworks/libheif.1.dylib" target/release/bundle/osx/oculante.app/Contents/oculante
cargo build --release --target x86_64-apple-darwin --features notan/shaderc
echo otool for aarch64:
otool -L target/aarch64-apple-darwin/release/oculante
lipo -create -output target/release/bundle/osx/oculante.app/Contents/MacOS/oculante target/x86_64-apple-darwin/release/oculante target/aarch64-apple-darwin/release/oculante 
file target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
echo otool for universal binary:
otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante

cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist



