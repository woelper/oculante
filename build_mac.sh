
rustup target list | grep installed
TOOLCHAIN=$(rustc --version --verbose | grep host | cut -f2 -d":" | tr -d "[:space:]")
echo we are using $TOOLCHAIN
export MACOSX_DEPLOYMENT_TARGET=10.15
cargo install cargo-bundle
brew install libheif
brew install nasm
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
cargo bundle --release


cargo build --release --target aarch64-apple-darwin --features "notan/shaderc heif"
cargo build --release --target x86_64-apple-darwin --features notan/shaderc
echo otool for aarch64:
otool -L target/aarch64-apple-darwin/release/oculante
lipo -create -output target/release/bundle/osx/oculante.app/Contents/MacOS/oculante target/x86_64-apple-darwin/release/oculante target/aarch64-apple-darwin/release/oculante 
file target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
echo otool for universal binary:
otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante

otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
mkdir target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/libheif/lib/libheif.1.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/x265/lib/libx265.209.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/libde265/lib/libde265.0.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/aom/lib/libaom.3.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/webp/lib/libsharpyuv.0.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /opt/homebrew/opt/libvmaf/lib/libvmaf.3.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
install_name_tool -change /opt/homebrew/opt/libheif/lib/libheif.1.dylib "@executable_path/../Frameworks/libheif.1.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libx265.209.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libde265.0.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libaom.3.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libsharpyuv.0.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libvmaf.3.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -change /opt/homebrew/opt/x265/lib/libx265.209.dylib "@executable_path/../Frameworks/libx265.209.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /opt/homebrew/opt/libde265/lib/libde265.0.dylib "@executable_path/../Frameworks/libde265.0.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /opt/homebrew/opt/aom/lib/libaom.3.dylib "@executable_path/../Frameworks/libaom.3.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /opt/homebrew/opt/webp/lib/libsharpyuv.0.dylib "@executable_path/../Frameworks/libsharpyuv.0.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /opt/homebrew/opt/libvmaf/lib/libvmaf.3.dylib "@executable_path/../Frameworks/libvmaf.3.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libaom.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libvmaf.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libsharpyuv.0.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libaom.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libde265.0.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libx265.209.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante



cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist



