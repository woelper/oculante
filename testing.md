# Testing steps after Notan removal
- [x] Shortcuts in the app: Regular and with modifiers (covered by scripts/ui-tests)
- [ ] Shortcuts in the app: key repeat
- [x] Shortcut settings menu (known issues with modifiers)
- [x] Shortcut settings menu: Ctrl+C, Ctrl+V and Ctrl+X can not be assigned, egui does not report those keys as held (Ctrl+V only while the clipboard holds text)
- [ ] Borderless mode
- [ ] Always on top: Works on Mac, does not work on PopOS/Cosmic (Wayland)
- [ ] Paint mode
- [ ] OSX file associations: reimplemented without fruitbasket (see src/mac.rs), never run on a Mac. Check "Open with" and a double click in Finder, both with Oculante closed and with it already running, a file dropped on the app icon, several files at once, and starting from the terminal with a file as argument (the file must not be opened twice).
- [ ] macOS trackpad: check the zoom speed on the image and the scroll speed in the panels. On master both were off (zoom five times too fast, panels ten times too slow) and got fixed for 0.9.7. That fix is specific to notan, this branch gets its scroll input from egui and has not been tried on a Mac.
- [ ] See if the transparency issue has been fixed (#342) (Blending leaves the window alpha untouched now, needs a check on Wayland.)
- [ ] UI tests: run them in a headless Wayland session (Sway or Jay) with ydotool instead of xdotool, so problems that only show on Wayland are covered (suggested by Stoppedpuma in #811)

# Obvious defects
- [ ] The > icons (carets) are off since the update to egui 0.36
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
- [x] When loading large images (/tests/large_image.jpg), panning and zooming is slow.
- [x] Loading large images (/tests/large_image.jpg) is significantly slower than Apple's "Preview". For most other images it is faster. We need to implement a test or benchmark and see if we can improve this.
- [x] switching channels (rgba) is slow

# Cleanup
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
  - There is a pure Rust decoder now, `heic-rs`. It is in as the feature `heif_native` (not a default feature yet): HEIC on Linux with a plain build, nothing to install. If `heif` is enabled as well, libheif is used.
  - Until tbraun96/heic-rs#7 and #8 are released, Cargo.toml patches in a fork with both. Without them photos from phones come out with wrong contrast and colour. A crate published to crates.io does not get the patch.
  - Open: make it a default feature once upstream has released the fixes, then drop libheif from the Mac bundles (most of `scripts/build_mac.sh` and `scripts/build_mac_intel.sh` exists for it). Not decoded by heic-rs: 4:2:2 and 4:4:4 (Canon HIF, #710), image sequences (#777).
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
