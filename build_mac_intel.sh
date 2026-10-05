# Builds the app bundle for Intel Macs, with the libraries it needs inside.
# Used by the release workflow and by the workflow that tests signing.
set -e

# Room in the header of the binary for the library paths that are rewritten
# below. Without it install_name_tool fails once the new paths no longer fit.
export RUSTFLAGS="-C link-arg=-Wl,-headerpad_max_install_names"

rustc --version --verbose
brew install libheif
# x265 changes its library name with every release, so look it up
X265=$(basename /usr/local/opt/x265/lib/libx265.[0-9]*.dylib)
cargo install cargo-bundle
cargo bundle --release --features "notan/shaderc heif"
otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
mkdir target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/libheif/lib/libheif.1.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/x265/lib/$X265 target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/libde265/lib/libde265.0.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/aom/lib/libaom.3.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/webp/lib/libsharpyuv.0.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
cp /usr/local/opt/libvmaf/lib/libvmaf.3.dylib target/release/bundle/osx/oculante.app/Contents/Frameworks/
install_name_tool -change /usr/local/opt/libheif/lib/libheif.1.dylib "@executable_path/../Frameworks/libheif.1.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/$X265" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libde265.0.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libaom.3.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libsharpyuv.0.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -add_rpath "@executable_path/../Frameworks/libvmaf.3.dylib" target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
install_name_tool -change /usr/local/opt/x265/lib/$X265 "@executable_path/../Frameworks/$X265" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /usr/local/opt/libde265/lib/libde265.0.dylib "@executable_path/../Frameworks/libde265.0.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /usr/local/opt/aom/lib/libaom.3.dylib "@executable_path/../Frameworks/libaom.3.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /usr/local/opt/webp/lib/libsharpyuv.0.dylib "@executable_path/../Frameworks/libsharpyuv.0.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
install_name_tool -change /usr/local/opt/libvmaf/lib/libvmaf.3.dylib "@executable_path/../Frameworks/libvmaf.3.dylib" target/release/bundle/osx/oculante.app/Contents/Frameworks/libaom.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libvmaf.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libsharpyuv.0.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libaom.3.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libde265.0.dylib
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/$X265
codesign -s "-" -fv target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib
otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante
# The app must not depend on anything from Homebrew that is not in the bundle (#810)
if otool -L target/release/bundle/osx/oculante.app/Contents/MacOS/oculante | grep /usr/local/; then echo "The app still links a Homebrew library that is not bundled"; exit 1; fi
if otool -L target/release/bundle/osx/oculante.app/Contents/Frameworks/libheif.1.dylib | tail -n +3 | grep /usr/local/; then echo "libheif still links a Homebrew library that is not bundled"; exit 1; fi
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist
