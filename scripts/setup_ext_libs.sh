current_dir=$PWD
cd $(git rev-parse --show-toplevel)

rm -rf libheif
git clone https://github.com/strukturag/libheif.git
cd libheif
mkdir build
cd build
cmake --preset=release ..
make



# mac
# brew install cmake make pkg-config x265 libde265 libjpeg libtool
# mkdir build
# cd build
# cmake --preset=release ..
# ./configure
# make

# win
# git clone https://github.com/Microsoft/vcpkg.git
# cd vcpkg
# ./bootstrap-vcpkg.bat
# ./vcpkg integrate install
# ./vcpkg install libheif

cd $current_dir
