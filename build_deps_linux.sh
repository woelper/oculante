rm -rf libheif
git clone --branch v1.17.3 https://github.com/strukturag/libheif.git
cd libheif
mkdir build
cd build
cmake --preset=release ..
make
cd ../../

export PKG_CONFIG_PATH=$(pwd)/libheif/build
#cargo build --release --features heif
#rm -rf libheif
