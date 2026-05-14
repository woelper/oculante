@REM vcpkg install libde265:x64-windows 
@REM vcpkg install libheif:x64-windows

set "current_dir=%CD%"
for /f "tokens=*" %%i in ('git rev-parse --show-toplevel') do cd /d "%%i"

git clone https://github.com/Microsoft/vcpkg.git
cd vcpkg
./bootstrap-vcpkg.bat
./vcpkg integrate install
@REM ./vcpkg install libheif
@REM cd ..

cd "%current_dir%"
