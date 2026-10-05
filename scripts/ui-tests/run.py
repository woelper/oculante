#!/usr/bin/env python3
"""UI tests that drive a real Oculante window on a virtual display.

    python3 scripts/ui-tests/run.py [--binary target/debug/oculante] [test ...]

Each test checks behaviour that broke in a release before. See README.md.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from uitest import App, changed_pixels, image, region_has_color, rightmost_x  # noqa: E402

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
        # the slider sits in the row at y = 215: filled up to the handle, the
        # empty rail to the right of it
        row = (820, 211, 190, 9)
        handle = rightmost_x(before, row, ACCENT)
        assert handle is not None, "the filled part of the slider is missing"
        rail_end = rightmost_x(before, row, RAIL, 8)
        assert rail_end is not None and rail_end > handle + 20, "the rail right of the handle is missing"
        app.drag(handle - 4, 215, handle + 40, 215)
        app.move(0.4, 0.5)
        app.settle(1.5)
        after = app.shot("after_drag")
        moved = rightmost_x(after, row, ACCENT)
        assert moved is not None and moved > handle + 25, "the handle did not follow the drag"
        changed = changed_pixels(before, after)
        assert changed > 50_000, f"dragging the slider changed only {changed} pixels of the image"


def test_measure_draws_rectangle(binary):
    """Dragging with the right mouse button draws the measured rectangle over the image (invisible in 0.9.3)."""
    with App(binary, "measure", [image("moss.jpg")], settings={"experimental_features": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log("Got frame"), "the image never loaded"
        app.key("i")
        app.settle(1.0)
        app.scroll_down(120, 300, 12)  # down to the tools in the info panel
        app.click(70, 389)  # open "Measure"
        app.settle(0.8)
        left_edge = (446, 250, 9, 60)
        before = app.shot("before_measure")
        assert not region_has_color(before, left_edge, (255, 255, 255), 12), "there is a rectangle before measuring"
        app.move(450, 180)
        app.x("xdotool", "mousedown", "3")
        for step in range(1, 11):
            app.move(450 + 30 * step, 180 + 22 * step)
        app.x("xdotool", "mouseup", "3")
        app.move(900, 520)
        app.settle(1.0)
        after = app.shot("after_measure")
        assert region_has_color(after, left_edge, (255, 255, 255), 12), "the measured rectangle is not drawn"


# the gold handles as they appear under the dark overlay of the crop tool
GOLD = (185, 156, 0)


def add_perspective_crop(app):
    app.click(900, 58)  # open the filter list
    app.settle(0.8)
    app.scroll_down(912, 200, 16)  # to the end of the list
    app.click(912, 361)  # add "Perspective crop"
    app.settle(1.0)
    app.move(0.3, 0.8)
    app.settle(1.0)


def test_perspective_crop_handles(binary):
    """The perspective crop shows its corner handles over the image and can be applied (handles were hidden in 0.9.3)."""
    with App(binary, "perspective_crop", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log("Got frame"), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        add_perspective_crop(app)
        added = app.shot("added")
        top_left = (125, 25, 30, 30)  # around the image's top left corner
        assert region_has_color(added, top_left, GOLD, 25), "the corner handle is not drawn"
        app.drag(140, 50, 300, 160)
        app.move(0.3, 0.8)
        app.settle(1.0)
        dragged = app.shot("dragged")
        assert not region_has_color(dragged, top_left, GOLD, 25), "the handle did not leave its corner"
        assert region_has_color(dragged, (285, 145, 30, 30), GOLD, 25), "the handle did not follow the drag"
        app.click(906, 224)  # "Apply"
        app.move(0.3, 0.8)
        app.settle(2.0)
        applied = app.shot("applied")
        assert "panicked" not in app.log(), "the app crashed when applying the crop"
        assert changed_pixels(dragged, applied) > 50_000, "applying the crop did not change the image"

        # a crop that is added after an applied one was removed starts with its handles again
        app.click(868, 185)  # the x in the header of the crop
        app.settle(1.5)
        add_perspective_crop(app)
        again = app.shot("added_again")
        assert region_has_color(again, top_left, GOLD, 25), "a crop added a second time has no handles"


TESTS = [
    test_perspective_crop_handles,
    test_shortcuts,
    test_start_with_idle_stdin,
    test_piped_file_names,
    test_slider_changes_image,
    test_measure_draws_rectangle,
]


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
