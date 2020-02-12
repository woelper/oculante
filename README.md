# oculante
A no-nonsense hardware-accelerated image viewer


[![Build Status](https://travis-ci.org/woelper/oculante.svg?branch=master)](https://travis-ci.org/woelper/oculante)

I started this as a toy project to make a simple image viewer. Here are the features:

Image format support:
- bmp	
- gif (No animation support)	
- hdr	
- ico	
- jpeg	
- png	
- pnm	
- tga	
- tiff	
- webp	
- DDS (DXT1-5, via _dds-rs_)
- psd (via _psd_)
- svg (via _nsvg_)

Platform support:
- Linux
- Mac
- Windows

Misc
- Async image loading
- Color picker / basic image info

Planned:
- Custom display for images with unassociated channels
- Image rotation (and read EXIF for that)
- Investigate PVR / ETC support
- ~~SVG support~~
- Brighness/gamma adjust for HDR
- EXR support
- ~~Read next image(s) in dir and advance to them~~
