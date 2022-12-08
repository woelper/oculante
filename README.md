# oculante 

![Logo](res/oculante.png "Logo")

_A no-nonsense hardware-accelerated image viewer_


Oculante's vision is to be a fast, unobtrusive, portable image viewer with wide image format support, offering image analysis and basic editing tools.
- Completely bloat-free
- Available for Win, Mac, Linux and NetBSD
- Supports a wide range of images and SVG
- Can display unassociated channels correctly (If your image uses alpha and color channels to encode data in a special way)
- Lets you pick pixels, display location and color values
- Offers basic nondestructive editing: Crop, resize, paint, contrast, HSV, rotate, blur, noise, ...

[![build](https://github.com/woelper/oculante/actions/workflows/rust.yml/badge.svg)](https://github.com/woelper/oculante/actions/workflows/rust.yml)
![GitHub all releases](https://img.shields.io/github/downloads/woelper/oculante/total?label=release%20downloads)
![Crates.io](https://img.shields.io/crates/d/oculante?label=crates.io%20downloads)

![Screenshot](res/screenshot_1.png "Screenshot")


## Correct color channel display:

Images may contain color information that is masked by the alpha channel. Although it's present you will not see it since usually RGB values are multiplied with the A channel when displayed. If you press <kbd>u</kbd> you will be able to inspect such data.

![Screenshot](res/premult.png "Screenshot")


## Installation
Just download the executable for your system from the releases tab (https://github.com/woelper/oculante/releases). No installation is required. In order to open images you can configure your system to open your desired image formats with oculante, drag them onto the executable or into the window. Right now the executables are roughly 10MB.

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
- Display unassociated / unpremultiplied alpha (<kbd>u</kbd>)
- Lossless JPEG editing: Crop, rotate, mirror without recmpressing data
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
> <kbd>Left</kbd>/<kbd>Right</kbd> = prev/next image in folder
>
> `Right mouse` pick color from image (in paint mode)



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


## Attribution
Test / benchmark pictures:

https://unsplash.com/@mohsen_karimi

https://unsplash.com/@frstvisuals

## Building

Linux:

`sudo apt-get install libxcb-shape0-dev libxcb-xfixes0-dev libasound2-dev nasm`

Win:
Install Nasm from https://www.nasm.us/pub/nasm/releasebuilds/2.15.05/win64/

Mac
`brew install nasm`

### Cargo Features
If you disable `turbo` (on by default), the turbojpeg library will not be used to open jpeg images. You won't need Nasm to be installed.