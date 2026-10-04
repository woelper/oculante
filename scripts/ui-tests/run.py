#!/usr/bin/env python3
"""UI tests that drive a real Oculante window on a virtual display.

    python3 scripts/ui-tests/run.py [--binary target/debug/oculante] [test ...]

Each test checks behaviour that broke in a release before. See README.md.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from uitest import App, changed_pixels, image, region_has_color  # noqa: E402

# key combination (xdotool names) and the shortcut it must trigger
SHORTCUTS = [
    ("i", "InfoMode"), ("i", "InfoMode"), ("e", "EditMode"), ("e", "EditMode"),
    ("r", "RedChannel"), ("g", "GreenChannel"), ("b", "BlueChannel"), ("a", "AlphaChannel"),
    ("u", "RGBChannel"), ("c", "RGBAChannel"), ("v", "ResetView"),
    ("1", "ZoomActualSize"), ("2", "ZoomDouble"), ("3", "ZoomThree"), ("4", "ZoomFour"), ("5", "ZoomFive"),
    ("equal", "ZoomIn"), ("minus", "ZoomOut"), ("t", "AlwaysOnTop"), ("t", "AlwaysOnTop"),
    ("z", "ZenMode"), ("z", "ZenMode"), ("Right", "NextImage"), ("Left", "PreviousImage"),
    ("End", "LastImage"), ("Home", "FirstImage"),
    ("shift+Right", "PanRight"), ("shift+Left", "PanLeft"), ("shift+Up", "PanUp"), ("shift+Down", "PanDown"),
    ("shift+c", "CompareNext"), ("ctrl+c", "Copy"), ("ctrl+v", "Paste"), ("f", "Fullscreen"), ("f", "Fullscreen"),
]


def test_shortcuts(binary):
    """Every default shortcut is triggered by its key (broke in 0.9.3 with the notan 0.14 key names)."""
    failures = []
    with App(binary, "shortcuts", [image("test.png")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log("Got frame"), "the image never loaded"
        for combo, command in SHORTCUTS:
            since = len(app.log())
            app.key(combo)
            if not any(m.startswith(command + " ") for m in app.matched_shortcuts(since)):
                failures.append(f"{combo} did not trigger {command}")
    assert not failures, "; ".join(failures)


def test_start_with_idle_stdin(binary):
    """The image shows even if stdin is a pipe that never delivers anything (hung since 0.9.3)."""
    read_end, write_end = os.pipe()
    try:
        with App(binary, "idle_stdin", [image("test.png")], stdin=read_end) as app:
            app.wait_window()
            assert app.wait_for_log("Got frame"), "nothing was loaded while stdin stayed open"
    finally:
        os.close(read_end)
        os.close(write_end)


def test_piped_file_names(binary):
    """File names piped into stdin are opened as a list."""
    read_end, write_end = os.pipe()
    os.write(write_end, f"{image('test.png')}\n{image('rust.png')}\n".encode())
    os.close(write_end)
    try:
        with App(binary, "piped_names", [], stdin=read_end) as app:
            app.wait_window()
            assert app.wait_for_log("Got frame"), "the piped image never loaded"
            opened = [l for l in app.log().splitlines() if "Image is: [" in l and "rust.png" in l]
            assert opened, "the second piped file is missing from the list"
    finally:
        os.close(read_end)


RAIL = (60, 60, 60)
ACCENT = (255, 0, 75)


def test_slider_changes_image(binary):
    """A filter slider is drawn completely and can be dragged (handle and rail vanished in 0.9.3)."""
    with App(binary, "slider", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log("Got frame"), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(900, 58)  # open the filter list
        app.settle(0.8)
        app.click(912, 88)  # add "Brightness"
        app.settle(0.8)
        app.move(0.4, 0.5)
        app.settle(0.8)
        before = app.shot("before_drag")
        # the slider sits in the row at y = 215: filled up to the handle in the
        # middle, the empty rail to the right of it
        assert region_has_color(before, (835, 211, 40, 9), ACCENT), "the filled part of the slider is missing"
        assert region_has_color(before, (895, 211, 60, 9), RAIL), "the rail right of the handle is missing"
        assert not region_has_color(before, (905, 211, 20, 9), ACCENT), "the slider does not start in the middle"
        app.drag(867, 215, 930, 215)
        app.move(0.4, 0.5)
        app.settle(1.5)
        after = app.shot("after_drag")
        assert region_has_color(after, (905, 211, 20, 9), ACCENT), "the handle did not follow the drag"
        changed = changed_pixels(before, after)
        assert changed > 50_000, f"dragging the slider changed only {changed} pixels of the image"


TESTS = [test_shortcuts, test_start_with_idle_stdin, test_piped_file_names, test_slider_changes_image]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default="target/debug/oculante")
    parser.add_argument("tests", nargs="*", help="names of tests to run, default all")
    args = parser.parse_args()
    binary = os.path.abspath(args.binary)
    selected = [t for t in TESTS if not args.tests or t.__name__ in args.tests]
    failed = 0
    for test in selected:
        try:
            test(binary)
            print(f"ok    {test.__name__}")
        except (AssertionError, RuntimeError) as error:
            failed += 1
            print(f"FAIL  {test.__name__}: {error}")
    print(f"{len(selected) - failed} of {len(selected)} passed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
