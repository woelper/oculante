# oculante

_A no-nonsense hardware-accelerated image viewer_


I started this as a toy project to make a simple image viewer. The vision is to create something with a broad support of industry-standard files and gradually add more image analysis tools. Here are some reasons why this might be helpful to you:
- Completely bloat-free
- Available for Win, Mac, Linux
- Supports a wide range of images
- Can display unassociated channels correctly (For example if your image uses alpha and color channels to encode data in a special way)
- Lets you pick pixels and displays location and color values

[![Build Status](https://travis-ci.org/woelper/oculante.svg?branch=master)](https://travis-ci.org/woelper/oculante)

![Screenshot](res/screenshot_1.png "Screenshot")


## installation
Just download the executable for your system from the releases tab (https://github.com/woelper/oculante/releases). No installation is required. In order to open something, you must configure your system to open your desired image formats with oculante, or drag them onto the executable.

## features

Image format support:
- bmp	
- gif (animation support and correct timing, no looping yet)	
- hdr (tonemapped)
- ico	
- jpeg	
- png	
- pnm	
- tga	
- tiff	
- webp
- farbfeld  
- DDS (DXT1-5, via _dds-rs_)
- psd (via _psd_)
- svg (via _nsvg_)
- exr (via _exr-rs_)

Platform support:
- Linux
- Mac
- Windows

Misc features
- Async image loading
- Color picker / basic image info (sample pixel position under cursor, sample color under cursor)
- Fit image to view
- Low cpu usage
- Pretty fast startup/loading time
- Display unassociated / unpremultiplied alpha (press `u`)
- Network listen mode: Start with `oculante -l port` and oculante will switch to receive mode. You can then pipe raw image data to that port, for example using `nc localhost 8888 < image.jpg`. If you pipe image sequences, these will be played at about 30 fps so you can pipe videos to it. This can be useful to visualize images from a headless system.

Planned:
- ~~Custom display for images with unassociated channels~~
- Image rotation (and read EXIF for that)
- Investigate PVR / ETC support
- ~~SVG support~~
- Brighness/gamma adjust for HDR
- ~~EXR support~~
- ~~Read next image(s) in dir and advance to them~~

Cheatsheet:
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
