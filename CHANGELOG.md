# 0.6.35  (Unreleased)

### :sparkles: Features

* Ctrl-Scrollwheel can be used to go to the next/previous images (77154a1f)

* Go to Next/Prev now cycles through the images in the folder, instead of stopping at either end  (6d2cd8cc)

* Ctrl-O or F1 bring up a native file browser dialog to select a file to load (8778b92c)

### :beetle: Bug Fixes

* Use natural/lexical sorting for file ordering instead of standard string-based sorting (d7783bd8)

* Passing a folder path as a command-line argument will load the first image in the folder, instead of hanging on startup (61547f46)

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
