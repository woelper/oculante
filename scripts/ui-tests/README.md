# UI tests

These tests start a real Oculante window on a virtual display, press keys,
click and drag, and check what the app did. They exist because unit tests did
not catch the regressions that went out with 0.9.3 and 0.9.4.

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
| `test_shortcuts` | Every default shortcut is triggered by its key |
| `test_start_with_idle_stdin` | The app starts when stdin is an open, silent pipe |
| `test_piped_file_names` | File names piped into stdin are opened |
| `test_slider_changes_image` | A filter slider can be dragged and changes the image |

## Notes

- The app is started with its own empty config directories, your settings are
  not read or changed.
- Clicks use fixed window coordinates for the default window size. If the
  layout of the edit panel changes, `test_slider_changes_image` needs new ones.
- The app only redraws on input, so the driver nudges the pointer before it
  looks at the result.
