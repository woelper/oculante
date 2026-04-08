Testing steps after Notan removal

- [ ] Shortcuts in the app: Regular, with modifiers, key repeat etc
- [ ] Shortcut settings menu (known issues with modifiers)
- [ ] Borderless mode
- [x] Always on top
- [ ] Paint mode

Obvious defects
- [ ] The loaded image is always drawn in front on top of the ui
- [ ] Changing values in the filter does not update the texture / current image
- [ ] No application icon
- [ ] Mipmaps possibly unnecessary (is this possible with egui?) if not, remove from settings
- [ ] Vsync possible with egui? If not, remove from settings
- [ ] Background color does not work
- [ ] Interpolate while zooming in/out may not work
- [ ] Animated images do not run when no input (egui isn't refreshing)

Cleanup
- [ ] Some functionality was added in the past due to the fact that Notan and egui were running in different parts of the loop and could not exchange data easily. For example the drawe() function and other draw code. This should be cleaned up.


Things to improve not related to removing Notan
- [ ] Painting should not be a mode but rather a normal operator


Things to keep in mind:
Oculante has a multi-stage system to keep textures in memory:
1. Once it is loaded, an image is kept as OculanteState.current_image. It is used to revert edits of the loaded image or enable image editing. It is a DynamicImage, so it can contain more information than we can see (float values etc). It is expensive to keep around as it consumes extra memory in addition to the texture, but I don't know a better way as long as we want to edit images. Perhaps we can get rid of it when we load the image again when entering edit mode and store it in EditState, but is this better?
2. EditState.result_pixel_op: the final edited image. When all edits are done, it should be used to generate the texture.
3. EditState.result_image_op: All image operaters are very expensive as they don't run per pixel and can't be paralellized and SIMD'd. So all image ops are run and cached into this, so the user can scrub and tweak pixel ops freely.