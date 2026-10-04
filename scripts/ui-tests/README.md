# UI tests

These tests start a real Oculante window on a virtual display, press keys,
click and drag, and check what the app did. They exist because unit tests do
not catch a shortcut that stopped working or a tool that is no longer drawn.

## Requirements

Linux with `Xvfb`, `xdotool` and ImageMagick (`import`, `compare`, `convert`):

    sudo apt install xvfb xdotool imagemagick

Rendering is done in software, no GPU or desktop session is needed.

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
| `test_slider_changes_image` | A filter slider is drawn completely and can be dragged |
| `test_measure_draws_rectangle` | Measuring with the right mouse button draws the rectangle over the image, in any direction, and not over the info panel |
| `test_perspective_crop_handles` | The perspective crop shows its handles, they can be dragged, the crop applied, removed and added again |

## Notes

- The app is started with its own empty config directories, your settings are
  not read or changed.
- Clicks use fixed window coordinates for the default window size. If the
  layout of the edit or info panel changes, the slider and measure tests need
  new ones.
- The app only redraws on input, so the driver nudges the pointer before it
  looks at the result.
