# oculante 

![Logo](res/oculante.png "Logo")

_A no-nonsense hardware-accelerated image viewer_


Oculante's vision is to be a fast, unobtrusive, portable image viewer with wide image format support, offering image analysis and basic editing tools.
- Free of charge, bloat-free, ad-free, privacy-respecting open source application
- Fast opening of images, fast startup
- Available for Win, Mac, Linux and NetBSD
- Supports a wide range of images and SVG
- Caches images for faster reloading
- Can display unassociated channels correctly (If your image uses alpha and color channels to encode data in a special way)
- Lets you pick pixels, display location and color values
- Offers basic nondestructive editing: Crop, resize, paint, contrast, HSV, rotate, blur, noise, ...
- SIMD-accelerated image editing

[![Cross-platform check](https://github.com/woelper/oculante/actions/workflows/build_checks.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/build_checks.yml)
![GitHub all releases](https://img.shields.io/github/downloads/woelper/oculante/total?label=release%20downloads)
![Crates.io](https://img.shields.io/crates/d/oculante?label=crates.io%20downloads)

![Screenshot](res/screenshot_1.png "Screenshot")

## Flipbook
With configurable caching, Oculante can quickly step through image sequences:
![Screenshot](res/flipbook.gif "Screenshot")

## Inspection
Get info about pixel values and position, with precise picking:
![Screenshot](res/picker.gif "Screenshot")

## Network
Raw image data can be sent to Oculante and will be loaded if possible, regardless of format. Streams of images will be played as a video. You can send images from cameras or headless systems such as a Raspberry Pi for example.
![Screenshot](res/net.gif "Screenshot")

## Correct color channel display:
Images may contain color information that is masked by the alpha channel. Although it is present you will not see it since usually RGB values are multiplied with the A channel when displayed. Oculante allows you to inspect all channels individually and see color data without transparency applied.
![Screenshot](res/premult.png "Screenshot")

## Installation
Oculante needs no installation, as it is just one executable. Just download it for your system from the releases tab (https://github.com/woelper/oculante/releases). In order to open images you can configure your system to open your desired image formats with oculante, drag them onto the executable or into the window. Right now the executables are roughly 12MB.

On NetBSD, a pre-compiled binary is available through the native package manager.
To install it, simply run
```sh
pkgin install oculante
```

## Features

### Image format support:
- bmp
- gif (animation support and correct timing)
- hdr, tonemapped
- ico
- jpeg
- png
- pnm
- tga
- avif
- tiff
- webp (via `libwebp-sys` - `image` had _very_ limited format support)
- farbfeld  
- DDS (DXT1-5, via `dds-rs`)
- psd (via `psd`)
- svg (via `resvg`)
- exr (via `exr-rs`), tonemapped

### Platform support:
- Linux
- Mac
- Windows
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
- Network listen mode: Start with `oculante -l port` and oculante will switch to receive mode. You can then pipe raw image data to that port, for example using `nc localhost 8888 < image.jpg`. Image types will be auto-detected. If you pipe image sequences, these will be played at about 30 fps so you can pipe videos to it. This can be useful to visualize images from a headless system.



### Shortcuts:
> <kbd>Esc</kbd>/<kbd>q</kbd> = quit
>
> <kbd>i</kbd> = display extended info
>
> <kbd>e</kbd> = display edit toolbox
>
> <kbd>v</kbd> = reset view
>
> <kbd>r</kbd>,<kbd>g</kbd>,<kbd>b</kbd>,<kbd>a</kbd> = display `r`ed/`g`reen/`b`lue/`a`lpha channel
>
> <kbd>c</kbd> = display color channel
>
> <kbd>u</kbd> = display colors unpremultiplied
>
> <kbd>f</kbd> = toggle fullscreen

> <kbd>t</kbd> = toggle always on top
>
> `mouse wheel`,  <kbd>+</kbd> <kbd>-</kbd> = zoom
>
> `left mouse`,`middle mouse`,  <kbd>Left</kbd> <kbd>Right</kbd> <kbd>Up</kbd> <kbd>Down</kbd> = pan
>
> <kbd>Left</kbd>/<kbd>Right</kbd>, `ctrl + mouse wheel` = prev/next image in folder
>
> `Right mouse` pick color from image (in paint mode)
>
> <kbd>Ctrl+O</kbd>, <kbd>F1</kbd> = show file picker to open an image



### Misc examples:

EXIF display

![Screenshot](res/screenshot_exif.png "Screenshot")



Extract a signature

![signature example](res/ex-signature.gif "Extracting a signature")

## Roadmap:
- ~~Image loading time is still worse than feh or xv~~ This is now very close, in particular after switching to `turbojpeg`
- Tests and benchmarks
- Image rotation (and read EXIF for that)
- Investigate PVR / ETC support
- Brighness/gamma adjust for HDR
- ~~SVG support~~
- ~~Custom display for images with unassociated channels~~
- ~~EXR support~~
- ~~Read next image(s) in dir and advance to them~~

### Privacy pledge
Oculante does in no way collect or send anonymous or non-anonynmous user data or statistics.
There are only two instances where oculante interacts with the network, and both never happen without being triggered by the user:
- Updating the application (must be triggered manually from settings)
- Listening for incoming images on a custom port (must be set on command line)

In addition, the only data saved locally by the application is:
- UI accent color
- Keybindings
- Vsync preferences
- Keep view offset/scale
- Whether the directory index bar is displayed

## Attribution
Test / benchmark pictures:

https://unsplash.com/@mohsen_karimi

https://unsplash.com/@frstvisuals

## Building

Linux:

`sudo apt-get install libxcb-shape0-dev libxcb-xfixes0-dev libgtk-3-dev libasound2-dev nasm`

Win:
Install Nasm from https://www.nasm.us/pub/nasm/releasebuilds/2.15.05/win64/

Mac
`brew install nasm`

### Cargo Features
If you disable `turbo` (on by default), the turbojpeg library will not be used to open jpeg images. You won't need Nasm to be installed.
The feature `file_open` will enable/disable a file open dialog. This pulls in additional dependencies and is enabled by default.
