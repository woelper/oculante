# Testing steps after Notan removal
- [ ] Shortcuts in the app: Regular, with modifiers, key repeat etc
- [ ] Shortcut settings menu (known issues with modifiers)
- [ ] Borderless mode
- [ ] Always on top: Works on Mac, does not work on PopOS/Cosmic (Wayland)
- [ ] Paint mode
- [ ] OSX file associations

# Obvious defects
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
- [ ] Measure draw above ui panels (#748) but is partially fixed
- [ ] Update position button in compare menu doesn't work (in info panel)
- [ ] Perspective crop is completely broken, only displays above ui panels (#749, not sure if duplication still applies? Definitely test further)
- [x] recent images are not added to list (seems to work on mac, test on linux)
- [x] When loading an animated image (at least png) the view does not reset
- [ ] When the image is finally loaded, the UI is not safely refreshed. This happens especially on very large images. A solution could be to pass a cloned ctx to the loading thread and ask it to repaint when the image was sent. Or use some kind of dirty flag that we already have, which may be easier.
- [ ] Artifact on some apng: https://github.com/etemesi254/zune-image/issues/372


# Performance
- [x] When loading large images (/tests/large_image.jpg), panning and zooming is slow.
- [x] Loading large images (/tests/large_image.jpg) is significantly slower than Apple's "Preview". For most other images it is faster. We need to implement a test or benchmark and see if we can improve this.
- [ ] switching channels (rgba) is slow

# Cleanup
- [x] Update to latest egui
- [ ] Some functionality was added in the past due to the fact that Notan and egui were running in different parts of the loop and could not exchange data easily. For example the drawe() function and other draw code. This should be cleaned up.
- [ ] Functionality which can be better isolated / separated should be compined in modules. Some of it makes sense, for example buttons that can be clicked and have a shortcut, other things are scattered all over the place.
- [ ] egui now supports system theme, remove dark-light (#774)


# Things to improve not related to removing Notan
- [ ] I am unhappy with the HEIC / HEIF situation. It is widely used by now and the build has been hard as we have not been using a native library and linking to libheif was hard on all platforms. Investigate if this has changed and if there is more robust heif/heic support that we can use, native rust if possible
- [ ] Painting should not be a mode but rather a normal operator
- [x] When entering a directory in the file browser and there is a search filter, the filter should be cleared when entering a directory
- [ ] Update dependencies: egui and helper libraries
- [ ] Update image libraries step by step
- [ ] What should happen to the image preview/zoom view in the info panel if it is resized?
- [ ] When the app starts for the first time, iterate through the recent menu and remove all items that do not exist on disk
- [x] Remove update functionality


Things to keep in mind:
Oculante has a multi-stage system to keep textures in memory:
1. Once it is loaded, an image is kept as OculanteState.current_image. It is used to revert edits of the loaded image or enable image editing. It is a DynamicImage, so it can contain more information than we can see (float values etc). It is expensive to keep around as it consumes extra memory in addition to the texture, but I don't know a better way as long as we want to edit images. Perhaps we can get rid of it when we load the image again when entering edit mode and store it in EditState, but is this better?
2. EditState.result_pixel_op: the final edited image. When all edits are done, it should be used to generate the texture.
3. EditState.result_image_op: All image operaters are very expensive as they don't run per pixel and can't be paralellized and SIMD'd. So all image ops are run and cached into this, so the user can scrub and tweak pixel ops freely.
