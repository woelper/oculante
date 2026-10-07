# Testing steps after Notan removal
- [x] Shortcuts in the app: Regular and with modifiers (covered by scripts/ui-tests)
- [x] Shortcuts in the app: key repeat (UI test `test_key_repeat`, X11)
- [x] Shortcut settings menu (known issues with modifiers)
- [x] Shortcut settings menu: Ctrl+C, Ctrl+V and Ctrl+X can not be assigned, egui does not report those keys as held (Ctrl+V only while the clipboard holds text)
- [x] Borderless mode (UI test `test_borderless`, X11: no title bar, the bar moves the window, the button closes it). Wayland is open, see #733 below.
- [ ] Always on top: Works on Mac, does not work on PopOS/Cosmic (Wayland)
- [x] Paint mode (UI test `test_paint_mode`)
- [x] Zen mode hides the bar and the info panel and brings them back (UI test `test_zen_mode`). The zen mode issues further down are still open.
- [x] Fullscreen fills the screen and the window returns to its place and size (UI test `test_fullscreen`, X11)
- [x] Compared with master (f7ae7ed), same machine, release builds: all 67 test files load the same (same 4 fail on both: no HEIC without a heif feature, two float KTX2 formats, the mp4 named .gif), all 30 edit operations give the same pixels and take the same time. Differences, all on purpose or in favour of this branch: 16 bit PNGs stay 16 bit, the first frame of two animated PNGs is right here and transparent on master, SVG see next item.
- [x] SVG: the default of "SVG scale" is 2 now, as master renders them. Who has run this branch before keeps the 1 that is saved in their settings.
- [x] No update check: master has "Check for updates" in the settings (feature `update`). It was removed here together with self_update and stays out.
- [ ] OSX file associations: reimplemented without fruitbasket (see src/mac.rs), never run on a Mac. Check "Open with" and a double click in Finder, both with Oculante closed and with it already running, a file dropped on the app icon, several files at once, and starting from the terminal with a file as argument (the file must not be opened twice).
- [ ] macOS trackpad: check the zoom speed on the image and the scroll speed in the panels. On master both were off (zoom five times too fast, panels ten times too slow) and got fixed for 0.9.7. That fix is specific to notan, this branch gets its scroll input from egui and has not been tried on a Mac.
- [ ] See if the transparency issue has been fixed (#342) (Blending leaves the window alpha untouched now, needs a check on Wayland.)
- [ ] UI tests: run them in a headless Wayland session (Sway or Jay) with ydotool instead of xdotool, so problems that only show on Wayland are covered (suggested by Stoppedpuma in #811)

# Obvious defects
- [x] Animations play as often as the file asks (GIF loop block, APNG num_plays, WebP and JXL loop counts), stop on the last frame and say so in a message. A GIF without a loop block plays once, as in browsers. Unit tests for GIF and APNG, UI test `test_animation_plays_as_often_as_the_file_asks`. There is no key to play it again yet, opening the file again does.
- [ ] A 16 bit APNG shows its default image instead of playing, the image crate can not composite 16 bit frames. Rare, but a valid file. Fix upstream in the image crate, or convert to 8 bit before compositing.
- [ ] Three broken APNGs of the conformance tests play although they are invalid (040 repeated acTL, 047 num_frames too low, 060 fdAT too large). The png crate does not notice, the spec asks to show the default image.
- [x] At 100% an image with an odd width or height sat on a half pixel and was blurred. Fixed, UI test `test_actual_size_is_pixel_exact`.
- [x] The > icons (carets) are off since the update to egui 0.36
- [x] The loaded image is always drawn in front on top of the ui
- [x] Background color does not work
- [x] Some or all settings don't seem to be saved / restored
- [x] Animated images do not run when no input (egui isn't refreshing): This is partially fixed, but does not work on some images, for example $HOME/Pictures/ioslaunch.gig
- [x] Changing values in the filter does not update the texture / current image
- [x] No application icon
- [x] Mipmaps don't seem to work
- [x] Vsync possible with egui? If not, remove from settings
- [x] Interpolate while zooming in/out may not work (when zoomed in it works, zooming out has no effect)
- [x] When changing the image / loading an image, the current one should only be transformed once the new one is loaded
- [x] Show alpha bleed in info panel not working
- [x] Show semi-transparent pixels in info panel not working
- [x] Show transparency grid does not work when enabled in settings
- [x] Caching does not seem to work any more, going back and forth between images takes a while, it should be instant
- [x] When loading a new image and having edit more present, the new image keeps the edit stack. It should honor the keep_edits option.
- [x] The info panel has a black bar to the right. It also should be resizable now
- [x] Modifying filter can shift image (this one is a little annoying to reproduce, move the image manually, then v to reset, then remove drag button to the right all the way, may take a few tries)
- [x] Filter sliders seem off (if they are clicked, they don't exactly match the mouse pos, maybe this is because of the egui update and custom slider styling)
- [x] Info panel grows indefinitely to the right on Linux
- [x] "Modified" and "Original" buttons in edit menu don't work
- [x] Info panel scroll bar is not in the correct location
- [x] Draw frame around image does not work when enabled in settings (#752)
- [x] recent files menu is way too large and obscures the whole screen and is cut off
- [x] When fullscreen is pressed, the exact same pixel under the cursor should still be under the cursor in full screen. The same should be true when switching back. This was old behavior.
- [x] Some apng files don't animate, for example "tests/Animated_PNG_example_bouncing_beach_ball.png" - this is likely not an animation problem, but due to the fact that the image is not reset/centered on first load.
- [x] Measure draw above ui panels (#748) but is partially fixed
- [x] Update position button in compare menu doesn't work (in info panel)
- [x] Perspective crop is completely broken, only displays above ui panels (#749, not sure if duplication still applies? Definitely test further)
- [x] recent images are not added to list (seems to work on mac, test on linux)
- [x] When loading an animated image (at least png) the view does not reset
- [x] File names piped into stdin are not opened (works on master)
- [ ] When the image is finally loaded, the UI is not safely refreshed. This happens especially on very large images. A solution could be to pass a cloned ctx to the loading thread and ask it to repaint when the image was sent. Or use some kind of dirty flag that we already have, which may be easier. (Background threads request a repaint now. Could not be reproduced on a virtual display, needs a check on a real one.)
- [x] Artifact on some apng: https://github.com/etemesi254/zune-image/issues/372
- [x] Fit image on window resize is broken
- [x] Modifier keys don't work correctly in keybinds part of preferences
- [x] Scrolling on menu items can change window zoom
- [x] Animated images are broken (as of 2026-05-19)
- [x] Colours on histogram no longer blend together correctly (RGB overlap should create white) and they are dim compared to 0.9.2 so this could be the issue since there is still slight blending?
- [x] Having edit menu open freezes animated image, the image has to be reloaded with the edit menu closed to continue animating
- [x] Floating windows cannot be resized vertically. I believe this possibly has to do with egui::ScrollArea?


# Open issues to check on this branch
Found by going through the open issues and looking at the code. None of these has a test yet.
- [ ] Pressing next or previous while the first image is still loading clears everything (#460). The folder list is only filled once the first image has arrived, and `next` on an empty list loads an empty path.
- [ ] Switching quickly between large images can show the wrong one (#400). Not confirmed in the code, but frames carry no path and the cache files them under whatever is current when they arrive.
- [ ] Info and edit buttons disappear when an image fails to decode (#426). They depend on an image being loaded.
- [ ] Next and previous sometimes did not update the view unless "Redraw every frame" was on (#776), same in zen mode (#642). The loader threads ask for a repaint now, check with large images.
- [ ] Animations stop after 500 loops (#504), the player has a `for _ in 0..500`.
- [ ] "Memory limit exceeded" on large files (#782). The generic loader uses the default limits of the image crate.
- [ ] Histogram and info do not follow edits (#647), the size falls back when the edit panel is closed (#761), reset view ignores a resize (#644). The info is computed from the original image, not from the result of the edits.
- [ ] EXIF rotation for files that are not JPEG (#601). The bracket keys only rotate JPEGs (#707).
- [ ] Zen mode: the hamburger menu and the file scroll bar stay visible (#690, #691), the menu does not follow a window resize (#700).
- [ ] Borderless mode on Wayland: dragging the window crashed on master (#733).
- [ ] Drag and drop on Wayland (#781).
- [ ] Keys that stay "held" after the window lost focus on Wayland (#374). The cause was notan's list of held keys, check that it is gone here.


# Performance

## Loading and memory, plan of 2026-10-06
Measured on the OptiPlex with release builds and photos of 24 megapixels from Wikimedia Commons (scratch tools: a stage benchmark, heaptrack, RSS while browsing and editing).

Done:
- [x] Info panel numbers only while it is open, once per image, in parallel: 24 MP 27 -> 6 ms, 144 MP 189 -> 24 ms, 16 bit gray 155 -> 20 ms. Before, every edit slider tick computed them again.
- [x] EXIF read for display without reading the file twice (288 MB for a 144 MB EXR), TIFF 45 -> 12 ms, EXR 90 -> 0 ms. Saving takes the EXIF from the source file.
- [x] Cache keeps to a memory budget (1/16 of RAM, 256 MB to 1 GB): 30 photos 2.1 GB -> levels off at about 1.4 GB in total.
- [x] Editing keeps one RGBA copy instead of two: one filter on 24 MP +245 MB -> +122 MB.
- [x] EXIF orientation in parallel: 75 -> 16 ms for a turned 24 MP photo.
- [x] Bugs found on the way: saved edits were never read again, false extension warning on every .heic, crash of pixel filters on float RGBA, saving as PNG crashed for everything but 8 bit RGBA, delete said "Deleted" when it failed, every image was opened as DICOM.

Open:
- [ ] Prefetch the next and the previous image of the folder into the cache while one is shown, cancelled when moving on. A cache hit shows the image in about 30 ms, decoding a 24 MP photo takes 110-500 ms depending on the format. Needs a load counter first, so a late result can never be shown for another image (the first step of the state handling rework).
- [ ] Bounded memory for long animations: all frames are kept as full RGBA, 380 small frames take 204 MB, a 1080p GIF of 300 frames would need about 2.5 GB. Keep all frames while they fit a budget, above it stream them: the loader decodes ahead into a bounded channel and again for every loop.
- [ ] Measure first: skip mipmaps for images that are never zoomed out below 100% (a third of the GPU memory of an image).
- The slow decoders against alternatives, measured on 2026-10-07 (24 MP photo, decode only, 20 threads):
  - [x] TIFF: decode the strips in parallel, each thread with its own decoder over the file in memory (tiff's `read_chunk`). Same pixels, LZW 348 -> 43 ms, Deflate 203 -> 24 ms, uncompressed 21 -> 5 ms. libtiff takes as long as Oculante now (LZW 320 ms), it decodes strips one after the other too. Keep the serial path for tiles, planar files and bit depths below 8. Done: through the loader LZW 348 -> 37 ms, Deflate 196 -> 20-35 ms, 16 bit Deflate 408 -> 210 ms. Found on the way and fixed: planar TIFFs did not load at all (only the first color was read).
  - [x] AVIF: avif-decode 3.0 decodes with rav1d, the Rust port of dav1d, instead of libaom: 450 -> 76-101 ms, and no C library to build. Needs Rust 1.98 (stable is 1.99). The image crate with the system dav1d takes 135 ms. Separately, Oculante's own copy of the result into RGBA costs 80 ms and can go. Done: 530 -> 112 ms through the loader, 10 and 12 bit images stay 16 bit, gray and 16 bit RGBA load. Rust 1.98 is needed to build now.
  - [ ] WebP: image-webp 361 ms, libwebp 134 ms. No image-webp release since 0.2.4 (2025-08). libwebp is C and had a remotely exploitable bug in 2023 (CVE-2023-4863), image-webp is safe Rust. Undecided.
  - [x] HEIC without tiles: heic-rs 1.4 s is already faster than libheif 1.17 (1.53 s). Both decode such a picture on one thread, there is nothing faster to switch to; the time is in heic-rs's residual decoding. Phones write tiled HEIC, which takes 100 ms.
- [x] #782: a JPEG that libjpeg warns about (here "Premature end of JPEG file") fails in turbojpeg, and the fallback to the image crate refuses anything above 512 MiB, so a 200 MP photo shows "Memory limit exceeded" while one of 50 MP loads. The same limit applies to WebP and the formats loaded through the image crate (a 12000x12000 RGBA WebP fails). Decode without limits as for PNG and TIFF, and show both errors when both decoders fail. Done, a truncated 200 MP JPEG shows in the app.
- [ ] Not worth it, measured: decoding a JPEG at a reduced size first. libjpeg-turbo at 1/8 is only 10-40% faster.

Simplifications found (read and spot checked, nothing changed yet):
- [ ] About 1100 lines can go without a change in behaviour: src/ktx2_loader/dds.rs and image_loader.rs are in no mod and could not compile, about 300 lines of unused KTX2 code, about 200 lines of functions nobody calls (image_ui, label_i_selected, rotate_rgbaimage, solo_channel, unpremult, LegacyEditState is used again now), unused Frame constructors, the fields redraw, first_start and name, src/input.rs (only comments), commented out code. Every module is `pub mod`, so rustc does not warn; `pub(crate)` would show the next batch.
- [ ] One `load()` and `reload()` for about 15 copies of `is_loaded = false; player.load(..); current_path = ..`.
- [ ] `Frame::ImageCollectionMember` behaves like `Still` everywhere.
- [ ] Mark the texture dirty directly instead of sending `Frame::UpdateTexture` through the channel from the main thread.
- [ ] Compare mode: pass the stored geometry instead of a dummy `CompareResult` frame and `transmute`. Removes reset_after_upload, compare_geometry and last_frame_was_compared_image.
- [ ] "Save as..." with the file_open feature saves with the image crate defaults instead of the encoder settings.

Tests to add:
- [x] Exact output of every edit operation on a small image. Found and fixed: results were cut instead of rounded (Invert off by one for 159 of 256 values), Contrast was a multiplication, Desaturate had red and green weights swapped, resize in other layouts than RGBA used the width as the height, the color conversion did nothing on RGBA, 3x3 filters left a black frame.
- [x] Lossless JPEG rotations and crops on a JPEG whose size is not a multiple of the block size. Found and fixed: a strip of the unturned image stayed at an edge, the incomplete blocks are trimmed now.
- [x] One table test that loads every file in res/tests, and one that writes and loads BMP, ICO, TGA, QOI, PNM, Farbfeld, HDR, TIFF in five layouts, XBM, XPM and WBMP. Found and fixed: a single valued 16 bit or float TIFF came out black, Netpbm files reported a wrong extension. Still without a test file: raw formats, DICOM, KRA, ORA, ICNS.
- [x] Scrubber: natural sort, wrap, removing entries at the ends. Found and fixed: started with only a file name (from a terminal in its folder), the first arrow key showed the same image again.
- [x] Delete in a UI test, and the shortcuts the shortcut test missed (Delete, Shift+Delete, [, ], q).
- [x] Settings: defaults from an empty file, a saved file of 0.9.6 still loads. Found and fixed: the settings of 0.9.6 could not be read at all (shortcuts in another format), everybody upgrading would have lost all settings.
- [x] Clipboard copy and paste with xclip. Found and fixed: with the menu open, Ctrl+V pasted twice.

Reported by reading the code, not checked yet:
- [x] Resize of images that are not 8 bit RGBA passed the width as the height: confirmed and fixed.
- [x] A TIFF with a single value came out black: confirmed and fixed.
- [x] Copy to the clipboard may not stay on X11: not so, the UI test reads it back with xclip.
- [x] Paste ran twice while the menu is open: confirmed and fixed.
- [x] When loading large images (/tests/large_image.jpg), panning and zooming is slow.
- [x] Loading large images (/tests/large_image.jpg) is significantly slower than Apple's "Preview". For most other images it is faster. We need to implement a test or benchmark and see if we can improve this.
- [x] switching channels (rgba) is slow

# Cleanup
- [x] Dependencies updated to the newest versions, unused ones removed. `wgpu` was only used for the names of its texture formats and is replaced by `wgpu-types`. Cargo.lock went from 857 to 794 packages. Checked: all 67 test files load the same as before, all edit operations give the same pixels except blur (rounding) and perspective crop (edge pixels, new imageproc).
- [ ] Not updated: `avif-decode` 3 needs Rust 1.98, `glow` 0.18 has to wait for an egui that uses it.
- [ ] LUT on an image with transparency: keeps the alpha now, it made the image opaque before (new lutgen works on RGBA). Check that this is what we want.
- [x] Update to latest egui
- [ ] Some functionality was added in the past due to the fact that Notan and egui were running in different parts of the loop and could not exchange data easily. For example the drawe() function and other draw code. This should be cleaned up.
- [ ] Functionality which can be better isolated / separated should be compined in modules. Some of it makes sense, for example buttons that can be clicked and have a shortcut, other things are scattered all over the place.
- [x] egui now supports system theme, remove dark-light (#774)
- [x] Create and move scripts into a scripts folder
- [ ] Update macOS plist
- [x] See if we can replace or drop fruitbasket
- [x] macOS: do a proper solution for "Open with" and file associations. Done the way Neovide does it on the same winit version: the event loop is created by us (`eframe::create_native`), then the app delegate that winit registered is turned into a subclass with `application:openFiles:` at runtime, and the app picks the paths up when it draws a frame. Once eframe is on winit 0.31 this can become a delegate of our own with `application:openURLs:`, which is the way winit documents (0.30 panics if the delegate is not its own, rust-windowing/winit#4015).
- [x] Move test files to res/tests
- [x] Update to Rust 2024

# Things to improve not related to removing Notan
- [ ] I am unhappy with the HEIC / HEIF situation. It is widely used by now and the build has been hard as we have not been using a native library and linking to libheif was hard on all platforms. Investigate if this has changed and if there is more robust heif/heic support that we can use, native rust if possible
  - There is a pure Rust decoder now, `heic-rs`. It is the default feature `heif_native`: a plain build opens HEIC, on Linux too, with nothing to install. If `heif` is enabled as well, libheif is used (the Mac and Windows release builds).
  - **Before the next release:** Cargo.toml patches in a fork of heic-rs with tbraun96/heic-rs#7 and #8. Without them photos from phones come out with wrong contrast and colour. A crate published to crates.io does not get the patch, and the git source in Cargo.lock is a problem for packagers who take all crates from crates.io (pkgsrc). Either upstream releases both fixes, or the fork is published under its own name.
  - Open: drop libheif from the Mac bundles (most of `scripts/build_mac.sh` and `scripts/build_mac_intel.sh` exists for it) and the step that builds libheif in the Linux release job, it is not used there. Not decoded by heic-rs: 4:2:2 and 4:4:4 (Canon HIF, #710), image sequences (#777).
- [ ] Painting should not be a mode but rather a normal operator
- [x] When entering a directory in the file browser and there is a search filter, the filter should be cleared when entering a directory
- [ ] Update dependencies: egui and helper libraries
- [ ] Update image libraries step by step
- [ ] What should happen to the image preview/zoom view in the info panel if it is resized?
- [x] When the app starts for the first time, iterate through the recent menu and remove all items that do not exist on disk
- [x] Remove update functionality
- [ ] Sign release binaries

Things to keep in mind:
Oculante has a multi-stage system to keep textures in memory:
1. Once it is loaded, an image is kept as OculanteState.current_image. It is used to revert edits of the loaded image or enable image editing. It is a DynamicImage, so it can contain more information than we can see (float values etc). It is expensive to keep around as it consumes extra memory in addition to the texture, but I don't know a better way as long as we want to edit images. Perhaps we can get rid of it when we load the image again when entering edit mode and store it in EditState, but is this better?
2. EditState.result_pixel_op: the final edited image. When all edits are done, it should be used to generate the texture.
3. EditState.result_image_op: All image operaters are very expensive as they don't run per pixel and can't be paralellized and SIMD'd. So all image ops are run and cached into this, so the user can scrub and tweak pixel ops freely.
