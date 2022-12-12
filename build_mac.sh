export MACOSX_DEPLOYMENT_TARGET=10.15
cargo bundle --release
#cargo bundle
#cp res/info.plist target/debug/bundle/osx/oculante.app/Contents/Info.plist
cp Info.plist target/release/bundle/osx/oculante.app/Contents/Info.plist