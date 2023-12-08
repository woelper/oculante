
rustup target list | grep installed

export MACOSX_DEPLOYMENT_TARGET=10.15
cargo install cargo-bundle
cargo bundle --release
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
brew install nasm
# arch -x86_64 brew install nasm


# rm -rf libheif
# git clone https://github.com/strukturag/libheif.git
# cd libheif
# mkdir build
# cd build
# cmake --preset=release ..
# make install
# cd ..
# cd ..

target=`rustup target list | grep installed | cut -d' ' -f1`

if [[ $target == "x86_64-apple-darwin" ]]; then
    cargo build --release --target x86_64-apple-darwin --features heif
    cargo build --release --target aarch64-apple-darwin
else
    cargo build --release --target x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin --features heif
fi

# cargo build --release --target x86_64-apple-darwin
# cargo build --release --target aarch64-apple-darwin --features heif
lipo -create -output target/release/bundle/osx/oculante.app/Contents/MacOS/oculante target/x86_64-apple-darwin/release/oculante target/aarch64-apple-darwin/release/oculante 
file target/release/bundle/osx/oculante.app/Contents/MacOS/oculante

# build universal binary
# cargo universal2
# copy universal binary into .app
# cp target/universal2-apple-darwin/release/oculante target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
#cp res/info.plist target/debug/bundle/osx/oculante.app/Contents/Info.plist
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist



