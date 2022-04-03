# oculante

![Logo](res/logo.png "Logo")

_A no-nonsense hardware-accelerated image viewer_


I started this as a toy project to make a simple image viewer. The vision is to create something with a broad support of industry-standard files and gradually add more image analysis tools. Here are some reasons why this might be helpful to you:
- Completely bloat-free
- Available for Win, Mac, Linux
- Supports a wide range of images and SVG
- Can display unassociated channels correctly (For example if your image uses alpha and color channels to encode data in a special way)
- Lets you pick pixels, displays location and color values

[![Build Status](https://travis-ci.org/woelper/oculante.svg?branch=master)](https://travis-ci.org/woelper/oculante)

## Window with active info box
![Screenshot](res/screenshot_1.png "Screenshot")


## Correct color channel display:

Images may contain color information that is masked by the alpha channel. Although it's present you will not see it since usually RGB values are multiplied with the A channel when displayed. If you press <kbd>u</kbd> you will be able to inspect such data.

![Screenshot](res/premult.png "Screenshot")


## Installation
Just download the executable for your system from the releases tab (https://github.com/woelper/oculante/releases). No installation is required. In order to open images you can configure your system to open your desired image formats with oculante, drag them onto the executable or into the window. Right now the executables are roughly 10MB.

## Features

Image format support:
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

Platform support:
- Linux
- Mac
- Windows

Misc features
- Image info (<kbd>i</kbd>) (pixel position, color info)
- Threaded image loading
- Fit image to view
- Low cpu usage
- Pretty fast startup/loading time
- Display unassociated / unpremultiplied alpha (<kbd>u</kbd>)
- Network listen mode: Start with `oculante -l port` and oculante will switch to receive mode. You can then pipe raw image data to that port, for example using `nc localhost 8888 < image.jpg`. If you pipe image sequences, these will be played at about 30 fps so you can pipe videos to it. This can be useful to visualize images from a headless system.



Cheatsheet:
> <kbd>Esc</kbd>/<kbd>q</kbd> = quit
>
> <kbd>i</kbd> = display extended info
>
> <kbd>v</kbd> = reset view
>
> <kbd>r</kbd>,<kbd>g</kbd>,<kbd>b</kbd>,<kbd>a</kbd> = display red/green/blue/alpha channel
>
> <kbd>c</kbd> = display color channel
>
> <kbd>u</kbd> = display colors unpremultiplied
>
> <kbd>f</kbd> = toggle fullscreen
>
> `mouse wheel` = zoom
>
> <kbd>left</kbd>/<kbd>right</kbd> = prev/next image in folder

Please submit bugs and feature requests on this github repo!

Roadmap:
- Image loading time is still worse than feh or xv
- Tests and benchmarks
- Image rotation (and read EXIF for that)
- Investigate PVR / ETC support
- Brighness/gamma adjust for HDR
- ~~SVG support~~
- ~~Custom display for images with unassociated channels~~
- ~~EXR support~~
- ~~Read next image(s) in dir and advance to them~~
