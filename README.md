<h1 align="center">
    <img alt="banner" src="res/banner.avif">
</h1>

 _A no-nonsense hardware-accelerated image viewer_

[<img src="res/download.svg" height="50">](https://github.com/woelper/oculante/releases/latest)

Oculante's vision is to be a fast, unobtrusive, portable image viewer with a wide range of supported image formats while also offering image analysis and basic editing tools.
- Free of charge, bloat-free, ad-free, privacy-respecting open source application
- Fast opening of images, fast startup
- Available for Windows, Mac, Linux, FreeBSD, and NetBSD
- Supports a wide range of image formats
- Caches images for faster reloading
- Can display unassociated channels correctly (If your image uses alpha and color channels to encode data in a special way)
- Lets you pick pixels to see their location and color values.
- Offers basic nondestructive editing such as cropping, resizing, painting, rotating, blur, and more!
- SIMD-accelerated image editing

[![](https://dcbadge.limes.pink/api/server/https://discord.gg/2Q6cF5ZWe7)](https://discord.gg/https://discord.gg/2Q6cF5ZWe7)
---
[![OSX](https://github.com/woelper/oculante/actions/workflows/check_osx.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/check_osx.yml)
[![NetBSD](https://github.com/woelper/oculante/actions/workflows/check_netbsd_minimal.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/check_netbsd_minimal.yml)
[![Ubuntu](https://github.com/woelper/oculante/actions/workflows/check_ubuntu_no_default_features.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/check_ubuntu_no_default_features.yml)
[![Check Windows](https://github.com/woelper/oculante/actions/workflows/check_windows.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/check_windows.yml)
[![ARM](https://github.com/woelper/oculante/actions/workflows/check_arm7.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/check_arm7.yml)
---
![GitHub all releases](https://img.shields.io/github/downloads/woelper/oculante/total?label=release%20downloads)
![Crates.io](https://img.shields.io/crates/d/oculante?label=crates.io%20downloads)
![Screenshot](res/screenshot_1.png "Screenshot")

## Flipbook
With configurable caching, Oculante can quickly step through image sequences:

![Screenshot](res/flipbook.gif "Screenshot")

## Inspection
Get information about pixel values and position, with precise picking:

![Screenshot](res/picker.gif "Screenshot")

## Network
Oculante can load raw image data no matter the format and will display it if possible, streams of images will be played like a video. This makes it perfect for sending images from cameras or headless devices like a Raspberry Pi.

![Screenshot](res/net.gif "Screenshot")

## Correct color channel display:
Images may contain color information that is masked by the alpha channel. Although it is present you will not see it since usually RGB values are multiplied with the A channel when displayed. Oculante allows you to inspect all channels individually and see color data without transparency applied.

![Screenshot](res/premult.png "Screenshot")

## Installation
Get started with Oculante by downloading the executable relevant to your platform from the [releases](https://github.com/woelper/oculante/releases/latest) page. The download size is kept small (currently around 25MB) by linking dependencies statically by default. Packages for ARM Linux are also built, please feel free to open an issue if you want your operating system of choice supported!

For those looking to manage Oculante through a package manager, please see the options below.

### Cargo

```sh
cargo install oculante
```

### Linux

- Arch Linux 

```sh
pacman -S oculante
```

- NixOS

```sh
environment.systemPackages = [
    pkgs.oculante
];
```

- openSUSE

```sh
zypper install oculante
```

- Flatpak

```sh
flatpak install flathub io.github.woelper.Oculante
```

### BSD

- FreeBSD

```sh
pkg install oculante
```

- NetBSD

```sh
pkgin install oculante
```

### Windows

- Scoop

```sh
scoop install extras/oculante
```

## Build Dependencies

Linux (Debian):

`sudo apt-get install libxcb-shape0-dev libxcb-xfixes0-dev libgtk-3-dev libasound2-dev nasm cmake`

Windows:
Install Nasm from https://www.nasm.us/pub/nasm/releasebuilds/2.15.05/win64/

Mac:
`brew install nasm`

## Updates

Oculante only gets updated when it improves something for you. You'll still see new releases about every month or two! To stay up to date you can use the update button in settings, or download the new releases executable! Updates are also managed through your package manager if you installed through one.

## Uninstalling

Uninstalling Oculante is a quick process, just delete the executable and delete the data folder. You can find the data folder in the relevant location for your operating system below.

- Windows: `~/AppData/Local/.oculante`
- Mac: `~/Library/Application Support/oculante`
- Linux & BSD: `~/.local/share/oculante`

## Features

### Image format support

- bmp
- gif (animation support and correct timing)
- hdr, tonemapped
- ico
- icns (via `rust-icns`)
- jpeg
- jpeg2000 (via `jpeg2k`, feature "j2k", on by default)
- png
- pnm
- tga
- jxl (JPEG XL, via `jxl-oxide`)
- avif
- tiff (via `tiff` with additional float/half support)
- webp (via `libwebp-sys` - `image` had _very_ limited format support)
- farbfeld
- DDS (DXT1-5, via `dds-rs`)
- psd (via `psd`)
- svg (via `resvg`)
- exr (via `exr-rs`), tonemapped
- RAW (via `quickraw` - nef, cr2, dng, mos, erf, raf, arw, 3fr, ari, srf, sr2, braw, r3d, nrw, raw). Since raw is a complex field without true standards, not all camera models are supported.
- ppm
- HEIC/HEIF (via `libheif-rs`). Enabled on Windows builds, but optional dependency on MacOS and Linux - available behind `heif` flag.
- qoi
- kra (currently only available on the `krita_support` branch)

### Platform support

- Linux
- Mac
- Windows
- FreeBSD
- NetBSD

### Misc features

- Image info (<kbd>i</kbd>) (pixel position, color info)
- Threaded image loading
- Fit image to view
- Window can be configured to be always on top - helpful to keep image as reference
- Low cpu usage
- Non-destructive painting and operator stack - edit very large images interactively by scaling them down first, then deleting the downscale operator once you want to export.
- Metafile support: Edit stack can be saved into a metafile which will be auto-loaded and applied when loading the original.
- Pretty fast startup / loading time
- Configurable image caching (Select how many images to keep in memory)
- Display unassociated / unpremultiplied alpha (<kbd>u</kbd>)
- Lossless JPEG editing: Crop, rotate, mirror without recompressing data
- Light/Dark theme and follow system theme mode
- Network listen mode: Start with `oculante -l port` and oculante will switch to receive mode. You can then pipe raw image data to that port, for example using `nc localhost 8888 < image.jpg`. Image types will be auto-detected. If you pipe image sequences, these will be played at about 30 fps so you can pipe videos to it. This can be useful to visualize images from a headless system.
- EXIF support: Load metadata if present 
- Load files from stdin: pipe your data with `cat image | oculante -s`

### Misc examples:

Viewing EXIF data

![Screenshot](res/screenshot_exif.png "Screenshot")

Extracting a signature

![signature example](res/ex-signature.gif "Extracting a signature")

## Roadmap

- Tests and benchmarks
- Read EXIF for image rotation
- Investigate PVR / ETC support
- Brighness/gamma adjust for HDR
- Redesigning the User Interface

## Attribution

Test / benchmark pictures:

https://unsplash.com/@mohsen_karimi

https://unsplash.com/@frstvisuals

## Privacy pledge

Oculante does in no way collect or send anonymous or non-anonynmous user data or statistics.
Oculante is and will remain free and open source.
There will never be ads.
There are only two instances where oculante interacts with the network, and both never happen without being triggered by the user:
- Updating the application (must be triggered manually from settings)
- Listening for incoming images on a custom port (must be set on command line)

In addition, Oculante saves some settings locally, for example:
- UI accent color
- Keybindings
- Vsync preferences
- Keep view offset/scale
- Whether the directory index bar is displayed
- Recent files

## License
This project is MIT licensed, but some parts such as the LUTs in res/LUT are under the GPL license. As a result, we're making our entire source code public. If you would like to use Oculante without publishing your source code, please remove any GPL-licensed components and their references.


### Extras
<details>
<summary>Cargo Features</summary>

- `turbo` (on by default), the turbojpeg library will not be used to open jpeg images. You won't need Nasm to be installed.

- `file_open` will enable/disable a OS-native file open dialog. This pulls in additional dependencies and is enabled by default. Disabling it will enable a custom file dialog. This will probably the default in the future.

- `notan/glsl-to-spirv` (default) uses the spirv shader compiler

- `notan/shaderc` uses shaderc as a shader compiler. Longer build time.

- `update` (default) enable app updating.

</details>

<details>
<summary>Default Shortcuts</summary>

`mouse wheel` = zoom

`left mouse`,`middle mouse` = pan

`ctrl + mouse wheel` = prev/next image in folder

`Right mouse` pick color from image (in paint mode)


<kbd>T</kbd> = AlwaysOnTop

<kbd>F</kbd> = Fullscreen

<kbd>I</kbd> = InfoMode

<kbd>E</kbd> = EditMode

<kbd>Right</kbd> = NextImage

<kbd>Home</kbd> = FirstImage

<kbd>End</kbd> = LastImage

<kbd>Left</kbd> = PreviousImage

<kbd>R</kbd> = RedChannel

<kbd>G</kbd> = GreenChannel

<kbd>B</kbd> = BlueChannel

<kbd>A</kbd> = AlphaChannel

<kbd>U</kbd> = RGBChannel

<kbd>C</kbd> = RGBAChannel

<kbd>V</kbd> = ResetView

<kbd>Minus</kbd> = ZoomOut

<kbd>Equals</kbd> = ZoomIn

<kbd>Key1</kbd> = ZoomActualSize

<kbd>Key2</kbd> = ZoomDouble

<kbd>Key3</kbd> = ZoomThree

<kbd>Key4</kbd> = ZoomFour

<kbd>Key5</kbd> = ZoomFive

<kbd>LShift</kbd> + <kbd>C</kbd> = CompareNext

<kbd>LShift</kbd> + <kbd>Left</kbd> = PanLeft

<kbd>LShift</kbd> + <kbd>Right</kbd> = PanRight

<kbd>LShift</kbd> + <kbd>Up</kbd> = PanUp

<kbd>LShift</kbd> + <kbd>Down</kbd> = PanDown

<kbd>Delete</kbd> = DeleteFile

<kbd>LShift</kbd> + <kbd>Delete</kbd> = ClearImage

<kbd>RBracket</kbd> = LosslessRotateRight

<kbd>LBracket</kbd> = LosslessRotateLeft

<kbd>LControl</kbd> + <kbd>C</kbd> = Copy

<kbd>LControl</kbd> + <kbd>V</kbd> = Paste

<kbd>LControl</kbd> + <kbd>O</kbd> = Browse

<kbd>Q</kbd> = Quit

<kbd>Z</kbd> = ZenMode

</details>
