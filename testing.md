# Testing steps after Notan removal
- [ ] Shortcuts in the app: Regular, with modifiers, key repeat etc
- [ ] Shortcut settings menu (known issues with modifiers)
- [ ] Borderless mode
- [x] Always on top
- [ ] Paint mode
- [ ] OSX file associations

# Obvious defects
- [x] The loaded image is always drawn in front on top of the ui
- [x] Background color does not work
- [x] Some or all settings don't seem to be saved / restored
- [ ] Animated images do not run when no input (egui isn't refreshing): This is partially fixed, but does not work on some images, for example $HOME/Pictures/ioslaunch.gig
- [x] Changing values in the filter does not update the texture / current image
- [x] No application icon
- [x] Mipmaps don't seem to work
- [x] Vsync possible with egui? If not, remove from settings
- [x] Interpolate while zooming in/out may not work (when zoomed in it works, zooming out has no effect)
- [x] When changing the image / loading an image, the current one should only be transformed once the new one is loaded
- [x] Show alpha bleed in info panel not working
- [x] Show semi-transparent pixels in info panel not working
- [ ] Show transparency grid does not work when enabled in settings
- [ ] Draw frame around image does not work when enabled in settings (#752)
- [ ] Caching does not seem to work any more, going back and forth between images takes a while, it should be instant
- [ ] Info panel ignoring theme
- [ ] Info panel dark theme colour is wrong (should be #191919)
- [ ] Info panel scroll bar in wrong spot
- [ ] Update position button in compare doesn't work
- [ ] Modified and Original buttons in edit menu don't work
- [ ] Measure is completely broken, only displays above ui panels (#748)
- [ ] Perspective crop is completely broken, only displays above ui panels (#749, not sure if duplication still applies? Definitely test further)
- [ ] Custom sliders are broken (#750)

# Cleanup
- [ ] Some functionality was added in the past due to the fact that Notan and egui were running in different parts of the loop and could not exchange data easily. For example the drawe() function and other draw code. This should be cleaned up.
- [ ] egui now supports system theme, remove dark-light (#774)
- [ ] Update to latest egui

Things to improve not related to removing Notan
- [ ] Painting should not be a mode but rather a normal operator
- [ ] When entering a directory in the file browser and there is a search filter, the filter should be cleared when entering a directory


Things to keep in mind:
Oculante has a multi-stage system to keep textures in memory:
1. Once it is loaded, an image is kept as OculanteState.current_image. It is used to revert edits of the loaded image or enable image editing. It is a DynamicImage, so it can contain more information than we can see (float values etc). It is expensive to keep around as it consumes extra memory in addition to the texture, but I don't know a better way as long as we want to edit images. Perhaps we can get rid of it when we load the image again when entering edit mode and store it in EditState, but is this better?
2. EditState.result_pixel_op: the final edited image. When all edits are done, it should be used to generate the texture.
3. EditState.result_image_op: All image operaters are very expensive as they don't run per pixel and can't be paralellized and SIMD'd. So all image ops are run and cached into this, so the user can scrub and tweak pixel ops freely.
