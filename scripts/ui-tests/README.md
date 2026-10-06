# UI tests

These tests start a real Oculante window on a virtual display, press keys,
click and drag, and check what the app did. They exist because unit tests do
not catch a shortcut that stopped working or a tool that is no longer drawn.

## Requirements

Linux with `Xvfb`, `xdotool` and ImageMagick (`import`, `compare`, `convert`):

    sudo apt install xvfb xdotool imagemagick

Rendering is done in software, no GPU or desktop session is needed.

The tests for fullscreen and for the window without a border need a window
manager, since a bare X server has nothing that draws a title bar or makes a
window fullscreen. They use `openbox` and `xprop` and are skipped without them:

    sudo apt install openbox x11-utils

## Running

    cargo build
    python3 scripts/ui-tests/run.py

Run a single test by name, or test another binary:

    python3 scripts/ui-tests/run.py test_shortcuts
    python3 scripts/ui-tests/run.py --binary path/to/oculante

Screenshots and the app log of each test end up in `target/ui-tests/<test>/`.

## What is covered

| Test | Checks |
|---|---|
| `test_shortcuts` | Every default shortcut is triggered by its key, including the ones with ctrl and shift |
| `test_start_with_idle_stdin` | The app starts when stdin is an open, silent pipe |
| `test_piped_file_names` | File names piped into stdin are opened as a list |
| `test_reload_when_file_changes` | An image that is overwritten on disk is loaded again |
| `test_image_cannot_get_lost` | Panning stops at the edge of the window |
| `test_image_layouts` | Gray, gray with alpha, RGB and RGBA images of 8 and 16 bit are shown with the right pixels and alpha |
| `test_system_fonts_on_demand` | The fonts of the system are loaded for a Japanese file name, and not for a plain one |
| `test_slider_changes_image` | A filter slider is drawn completely and can be dragged |
| `test_measure_draws_rectangle` | Measuring with the right mouse button draws the rectangle over the image, in any direction, and not over the info panel |
| `test_perspective_crop_handles` | The perspective crop shows its handles, they can be dragged, the crop applied, removed and added again |
| `test_key_repeat` | A key that is held down goes through the images of a folder |
| `test_zen_mode` | Zen mode hides the bar and the info panel, leaving it brings both back |
| `test_paint_mode` | A drag in paint mode leaves a stroke and does not move the image |
| `test_fullscreen` | The window fills the screen and returns to its place and size (needs a window manager) |
| `test_borderless` | Without a border there is no title bar, the app's own bar moves the window and its button closes it (needs a window manager) |
| `test_compare_list` | An image from the compare list comes back at its stored zoom and position, or keeps the current view with "persistent zoom" |
| `test_animation_plays_and_stops` | An animated GIF plays, and stops when the next image is shown |
| `test_keep_view` | "Keep view" shows the next image at the same zoom, without it the image is fitted again |
| `test_keep_edits` | "Keep edits" applies the filters to the next image, without it the image is shown as it is |
| `test_single_frame_gif_is_editable` | A GIF with one frame is treated as a still image, so filters change it |
| `test_actual_size_is_pixel_exact` | At 100% an image with an odd width and height shows exactly the pixels of the file, with linear and with nearest filtering |
| `test_overtaken_load` | A slow image that was skipped over does not show up later |
| `test_load_error_shows_toast` | A broken file shows an error toast and the app keeps running |
| `test_channel_view` | A single channel changes the image and all channels restore it |

## Notes

- The app is started with its own empty config directories, your settings are
  not read or changed.
- Clicks use fixed window coordinates for the default window size. If the
  layout of the edit or info panel changes, the slider and measure tests need
  new ones.
- The app only redraws on input, so the driver nudges the pointer before it
  looks at the result.
