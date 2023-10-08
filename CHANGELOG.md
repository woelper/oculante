# 0.7.7 (2023-10-08)

# 0.7.6 (2023-09-20)
- Update smithay-client-toolkit
- Update JXL
- Remove LTO from pkgbuild
- Add file deletion option (They are moved to system trash)
# 0.7.5 (2023-09-15)
- Better Windows performance (Lazy_loop enabled)
- Notan and Egui update
# 0.7.4 (2023-09-03)

# 0.7.3 (2023-08-22)

# 0.7.2 (2023-08-21)

### :green_apple: Chore

* update dependencies (601968b4)

# 0.6.69 (2023-07-20)

### :green_apple: Chore

* update deps (872704cb)

# 0.6.67 (2023-07-05)

### :green_apple: Chore

* update dependencies (7e9f87c7)

# 0.6.66 (2023-06-25)

### :green_apple: Chore

* update deps (2236c283)

# 0.6.65 (2023-06-18)

# 0.6.64 (2023-05-27)

### :beetle: Bug Fixes

* EXIF data is horizontally scrollable (fixes #184) (85cd7618)

### :sparkles: Features

* svg update with faster performance and text rendering support (0278fe97)
* Native rust JXL support (16f73800)

# 0.6.63 (2023-04-20)

# 0.6.62 (2023-04-19)

# 0.6.61 (2023-04-19)

### :beetle: Bug Fixes

* Prevent info panel flickering while auto-hiding scrollbar (696d8818)

### :sparkles: Features

* Simplified file saving - now with file picker (aa410cc6)
* Add optional checker background (f09d012e)

# 0.6.60 (2023-04-02)

# 0.6.59 (2023-04-02)

# 0.6.58 (2023-04-01)

### :green_apple: Chore

* update dependencies (498c862b)

# 0.6.57 (2023-03-27)

### :green_apple: Chore

* Update libwebp-sys and dirs (8fd3f97b)

# 0.6.56 (2023-03-23)

# 0.6.55 (2023-03-23)

### :green_apple: Chore

* Zip only executable for arm (57dfc4d4)

# 0.6.54 (2023-03-18)

### :sparkles: Features

* Support directory-specific edits (.oculante file). If such an edit file is present, the edit operations will be applied to any image.
* Minor UI tweaks
* The release process now builds `armv7-unknown-linux-gnueabihf` (Raspberry pi and others) and includes it in the release. This is Oculante with minimal features for now, with some external libraries disabled (have a look at Cargo.toml for details what is left out)

# 0.6.53 (2023-03-04)

### :beetle: Bug Fixes

* Prevent freezing when window resizes (91424bac)
* update jpegxl (8779d04b)

### :sparkles: Features

* Allow window title to be configured (000a34db)
* Keep track of recently opened images (77857a8f)

### :green_apple: Chore

* update dependencies (2dad8909)
* Update dependencies (8c78fa9f)

# 0.6.52 (2023-02-19)

# 0.6.51 (2023-02-12)

### :sparkles: Features

* Add multiply / divide by aplha operator (8b3eda86)
* Add window and taskbar icon (32900ab5)

### :green_apple: Chore

* update dependencies (92f3eb87)
* Update resvg and usvg (c2f96b47)
* update notan and rfd (6e0c00c2)

# 0.6.50 (2023-01-16)
* AVIF support

There are two features to choose from: `avif_native` (default, less images supported) and `dav1d` (optional, harder to build, better support)
`david` requires meson, ninja and nasm at least.

### :sparkles: Features

* RAW file support (02fa90e2)

# 0.6.39 (2023-01-07)

### :beetle: Bug Fixes

* slider is 1-based (fixes #116) (63226d5e)

### :green_apple: Chore

* update deps (2bc54c8f)

# 0.6.38 (2023-01-05)

### :beetle: Bug Fixes

* Reverse PanUp/Down (fixes #110) (89e43ef8)
* Shortcuts are sorted and grouped (8e6d2430)

### :sparkles: Features

* add home/end to move to first/last image (39412c7f)
* Add slider to step through images (5934b052)

# 0.6.37 (2023-01-02)

# 0.6.36 (2023-01-01)

### :beetle: Bug Fixes

* Make it possible to pass a folder-path as a command-line arg, instead of requiring a file within that (61547f46)
* Use Natural Sorting for filenames (d7783bd8)
* Prevent old settings file from becoming invalid (fixes #103) (10573c1b)

### :sparkles: Features

* Ctrl-O and/or F1 bring up a file browser dialog to select an image to load (8778b92c)
* Go to Next/Prev now cycles through the images in the folder, instead of stopping at either end (6d2cd8cc)
* Ctrl-Scrollwheel can be used to go to the next/prev images too (77154a1f)

### :green_apple: Chore

* update clap (c08f5f1a)
* update rfd and self_update (8ba00d8e)
* Update Changelog with the missing revision ID's (01f7bad3)
* Split out the list of supported image formats to a constant (SUPPORTED_EXTENSIONS) (60762f49)
* Update Changelog with recent changes (c4ab7fe7)

# 0.6.35 (2022-12-30)

### :sparkles: Features

* Enable persistent offset/zoom in settings (20e33e14)

### :green_apple: Chore

* remove edit/info checkboxes (11613c21)

# 0.6.34 (2022-12-19)

### :beetle: Bug Fixes

* Correct offset when entering/exiting full-screen mode (2ffe2d03)

### :green_apple: Chore

* Enhance crop precision (3b02a304)

# 0.6.33 (2022-12-18)

# 0.6.32 (2022-12-13)

### :sparkles: Features

* Mipmap generation (smoother images when zoomed out) and correct gamme when zooming (SRgba8 format) (b83b1c65)

# 0.6.31 (2022-12-13)

# 0.6.30 (2022-12-12)

### :sparkles: Features

* Correct gamma scaling and SIMD speedup (21d7159b)

### :green_apple: Chore

* update dependencies (1c73246b)

# 0.6.29 (2022-12-12)

### :beetle: Bug Fixes

* Support lossless ops on jpeg and jpg (757b29fc)

# 0.6.28 (2022-12-11)

### :beetle: Bug Fixes

* Allow building without default features (10a0f6a4)

# 0.6.27 (2022-12-10)

# 0.6.26 (2022-12-09)

# 0.6.25 (2022-12-08)

# 0.6.24 (2022-12-08)

### :sparkles: Features

* Lossless JPEG editing (2b4e4d40)

# 0.6.23 (2022-12-03)

### :beetle: Bug Fixes

* Histogram was not computed on image change (2096104a)

# 0.6.22 (2022-11-13)

### :sparkles: Features

* Save/load edit information in metafile. This allows non-destructive eding while leaving your original pictures intact. (c47bddb6)

### :green_apple: Chore

* Update SVG rendering (9fdc2e56)
* Slightly relax & update dependencies (bb9c03a8)

# 0.6.20 (2022-10-30)

### :beetle: Bug Fixes

* Support bad Gif data gracefully (fixes #60) (c0acfa69)
* Build script generates app icon on windows (548b9749)

# 0.6.19 (2022-10-25)

### :beetle: Bug Fixes

* Prevent thread crashing when opening corrupt images (3360dc7f)

# 0.6.18 (2022-10-22)

### :beetle: Bug Fixes

* Remove UI flicker if alpha tools are expanded/closed (1254dffc)
* Network listen mode now refreshes UI and has a dedicated unit test (00c7a91b)

### :sparkles: Features

* Enable EXIF support (37aeda9d)

# 0.6.17 (2022-10-17)

### :sparkles: Features

* Keep image centered on window resize (a8ca6f1e)

# 0.6.14 (2022-10-14)

### :beetle: Bug Fixes

* Fix unreliable gif loading (928610b6)


# 0.6.13 (2022-10-10)

### :green_apple: Chore

* update arboard and notan (4cb66206)


# 0.6.12 (2022-10-03)

### :beetle: Bug Fixes

* Change windows release to use windows server 2019 (bb740e12)

# 0.6.11 (2022-10-01)

### :beetle: Bug Fixes

* Re-enable blur (fixes #52) (e33d27db)

# 0.6.10 (2022-09-30)

### :beetle: Bug Fixes

* Tooltip colors automatically contrast theme color (51eee15e)

### :sparkles: Features

* Add always on top mode (a8fdc891)
* Filter with custom expressrion per pixel (afa438fe)

### :green_apple: Chore

* update dependencies (72ac0dce)

# 0.6.9 (2022-09-11)

### :beetle: Bug Fixes

* Enable correct accent color selection by changing layout (fixes #48) (a63cc859)

### :sparkles: Features

* Better operator layout, fixes quirky color picking in operator menu (627ace1c)

# 0.6.8 (2022-09-07)

### :beetle: Bug Fixes

* Remove offset when initially clicking into OSX window (81544cc4)

### :sparkles: Features

* Persistent settings support. Vsync and color theme are now customizable. (21ed3954)

### :green_apple: Chore

* Update psd, ext, dds-rs (ad2f531b)

# 0.6.7 (2022-09-06)

### :beetle: Bug Fixes

* Disable image center on window resize, as this caused jumping (ee557d47)

### :sparkles: Features

* Add Posterize image effect (3e019728)
* Equalize image operator added (748bf15e)
* Allow editing the export image extension to save as a different image format (23519eee)

# 0.6.6 (2022-09-01)

### :sparkles: Features

* Channel  Copy filter replaces Swapping - this brings more flexibility. Fill operator is now supporting the alpha channel to blend the color. (670ecaac)
* Improved UI slider widgets (9a3d2b20)
