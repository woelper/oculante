@REM vcpkg install libde265:x64-windows 
@REM vcpkg install libheif:x64-windows

git clone https://github.com/Microsoft/vcpkg.git
cd vcpkg
./bootstrap-vcpkg.bat
./vcpkg integrate install
@REM ./vcpkg install libheif
@REM cd ..
