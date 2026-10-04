#!/usr/bin/env python3
"""UI tests that drive a real Oculante window on a virtual display.

    python3 scripts/ui-tests/run.py [--binary target/debug/oculante] [test ...]

Each test checks behaviour that broke before. See README.md.
"""
import argparse
import os
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from uitest import OUT, App, changed_pixels, changed_pixels_in, image, region_has_color, rightmost_x  # noqa: E402

# what the app logs once an image is on screen
LOADED = "Received image"

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
    ("shift+c", "CompareNext"), ("ctrl+c", "Copy"), ("ctrl+shift+c", "CopyPath"), ("ctrl+v", "Paste"),
    ("s", "ScrubBar"), ("s", "ScrubBar"), ("f", "Fullscreen"), ("f", "Fullscreen"),
    # last, this opens the file browser
    ("ctrl+o", "Browse"),
]


def test_shortcuts(binary):
    """Every default shortcut is triggered by its key."""
    failures = []
    with App(binary, "shortcuts", [image("test.png")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        for combo, command in SHORTCUTS:
            since = len(app.log())
            app.key(combo)
            if command not in app.matched_shortcuts(since):
                failures.append(f"{combo} did not trigger {command}")
    assert not failures, "; ".join(failures)


def test_start_with_idle_stdin(binary):
    """The image shows even if stdin is a pipe that never delivers anything."""
    read_end, write_end = os.pipe()
    try:
        with App(binary, "idle_stdin", [image("test.png")], stdin=read_end) as app:
            app.wait_window()
            assert app.wait_for_log(LOADED), "nothing was loaded while stdin stayed open"
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
            assert app.wait_for_log(LOADED), "the piped image never loaded"
            opened = [l for l in app.log().splitlines() if "Image is: [" in l and "rust.png" in l]
            assert opened, "the second piped file is missing from the list"
    finally:
        os.close(read_end)


def test_reload_when_file_changes(binary):
    """An image that is overwritten on disk is loaded again."""
    folder = os.path.join(OUT, "reload_source")
    os.makedirs(folder, exist_ok=True)
    path = os.path.join(folder, "watched.png")
    shutil.copy(image("test.png"), path)
    with App(binary, "reload", [path]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        before = app.shot("before")
        time.sleep(1.1)  # file times can be as coarse as a second
        shutil.copy(image("rust.png"), path)
        time.sleep(1.0)
        # the file is looked at when a frame is drawn
        app.settle(2.0)
        app.settle(1.0)
        after = app.shot("after")
        assert changed_pixels(before, after) > 5000, "the changed file was not loaded again"


def test_image_cannot_get_lost(binary):
    """Panning stops at the edge of the window, so a short drag brings the image back."""
    with App(binary, "pan_limit", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.5, 0.5)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        for _ in range(4):
            app.drag(0.1, 0.5, 0.95, 0.5)
        gone = app.shot("gone")
        app.drag(0.9, 0.5, 0.3, 0.5)
        back = app.shot("back")
        assert changed_pixels(gone, back) > 20000, "the image was dragged further away than the window is wide"


def test_system_fonts_on_demand(binary):
    """The fonts of the system are only loaded for file names that need them."""
    folder = os.path.join(OUT, "font_source")
    os.makedirs(folder, exist_ok=True)
    loading = "Attempting to load sys fonts"
    for name, needed in (("plain name.png", False), ("写真 こんにちは.png", True)):
        path = os.path.join(folder, name)
        shutil.copy(image("test.png"), path)
        with App(binary, "fonts", [path]) as app:
            app.wait_window()
            app.move(0.6, 0.6)
            assert app.wait_for_log(LOADED), "the image never loaded"
            app.key("i")
            app.settle(1.5)
            app.shot("needed" if needed else "plain")
            if needed:
                assert loading in app.log(), "no system fonts were loaded for a Japanese file name"
            else:
                assert loading not in app.log(), "system fonts were loaded although nothing needs them"


# name, ImageMagick options that give the file its layout
LAYOUTS = [
    ("gray8.png", ["-colorspace", "Gray", "-define", "png:color-type=0", "-depth", "8"]),
    ("gray_alpha8.png", ["-colorspace", "Gray", "-alpha", "set", "-define", "png:color-type=4", "-depth", "8"]),
    ("rgb8.png", ["-define", "png:color-type=2", "-depth", "8"]),
    ("rgba8.png", ["-alpha", "set", "-define", "png:color-type=6", "-depth", "8"]),
    ("gray16.png", ["-colorspace", "Gray", "-define", "png:color-type=0", "-depth", "16"]),
    ("gray_alpha16.png", ["-colorspace", "Gray", "-alpha", "set", "-define", "png:color-type=4", "-depth", "16"]),
    ("rgb16.png", ["-define", "png:color-type=2", "-depth", "16"]),
    ("rgba16.png", ["-alpha", "set", "-define", "png:color-type=6", "-depth", "16"]),
]
LEFT, RIGHT = (200, 200, 200), (60, 60, 60)


def test_image_layouts(binary):
    """Gray, gray with alpha, RGB and RGBA images of 8 and 16 bit are shown with the right pixels."""
    folder = os.path.join(OUT, "layout_source")
    os.makedirs(folder, exist_ok=True)
    failures = []
    for name, options in LAYOUTS:
        path = os.path.join(folder, name)
        # Light on the left, dark on the right. The odd width makes rows that do not
        # end on a four byte boundary, which shears the image if the upload ignores it.
        draw = ["-size", "401x301", "xc:rgb(200,200,200)", "-fill", "rgb(60,60,60)", "-draw", "rectangle 201,0 400,300"]
        subprocess.run(["convert", *draw, *options, path], check=True)
        with App(binary, "layout", [path]) as app:
            app.wait_window()
            assert app.wait_for_log(LOADED), f"{name} never loaded"
            app.move(0.5, 0.97)
            app.settle(1.0)
            cx, cy = app.geom["WIDTH"] // 2, app.geom["HEIGHT"] // 2
            shot = app.shot(name[:-4])
            for dy in (-100, 100):
                for dx, color in ((-100, LEFT), (100, RIGHT)):
                    if not region_has_color(shot, (cx + dx, cy + dy, 3, 3), color, 12):
                        failures.append(f"{name}: wrong color at {dx},{dy}")
            # the alpha channel of an opaque image is white
            app.key("a")
            shot = app.shot(name[:-4] + "_alpha")
            if not region_has_color(shot, (cx + 100, cy + 100, 3, 3), (255, 255, 255), 12):
                failures.append(f"{name}: the alpha channel is not white")
    assert not failures, "; ".join(failures)


RAIL = (60, 60, 60)
ACCENT = (255, 0, 75)


def open_edit_panel(app):
    app.key("e")
    app.settle(1.0)
    app.click(895, 48)  # open the filter list
    app.settle(0.8)


def test_slider_changes_image(binary):
    """A filter slider is drawn completely and can be dragged."""
    with App(binary, "slider", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        open_edit_panel(app)
        app.click(897, 117)  # add "Brightness"
        app.settle(0.8)
        app.move(0.4, 0.5)
        app.settle(0.8)
        before = app.shot("before_drag")
        # the slider sits in the row at y = 214: filled up to the handle, the
        # empty rail to the right of it
        row = (770, 210, 200, 9)
        handle = rightmost_x(before, row, ACCENT)
        assert handle is not None, "the filled part of the slider is missing"
        rail_end = rightmost_x(before, row, RAIL, 8)
        assert rail_end is not None and rail_end > handle + 20, "the rail right of the handle is missing"
        app.drag(handle - 4, 214, handle + 40, 214)
        app.move(0.4, 0.5)
        app.settle(1.5)
        after = app.shot("after_drag")
        moved = rightmost_x(after, row, ACCENT)
        assert moved is not None and moved > handle + 25, "the handle did not follow the drag"
        changed = changed_pixels_in(before, after, (120, 40, 640, 550))
        assert changed > 50_000, f"dragging the slider changed only {changed} pixels of the image"


def test_measure_draws_rectangle(binary):
    """Dragging with the right mouse button draws the measured rectangle over the image, in any direction."""
    with App(binary, "measure", [image("moss.jpg")], settings={"experimental_features": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("i")
        app.settle(1.0)
        app.scroll_down(120, 300, 12)  # down to the tools in the info panel
        app.click(60, 395)  # open "Measure"
        app.settle(0.8)
        app.move(900, 520)
        app.settle(0.8)
        before = app.shot("before_measure")

        def measure(x0, y0, x1, y1):
            app.move(x0, y0)
            app.x("xdotool", "mousedown", "3")
            for step in range(1, 11):
                app.move(x0 + (x1 - x0) * step / 10, y0 + (y1 - y0) * step / 10)
            app.x("xdotool", "mouseup", "3")
            app.move(900, 520)
            app.settle(1.0)

        inside = (500, 220, 200, 140)  # well inside both rectangles measured below
        measure(450, 180, 750, 400)
        forward = app.shot("measured")
        assert changed_pixels_in(before, forward, inside) > 5_000, "the measured rectangle is not drawn"
        # from the bottom right to the top left, ending on the info panel
        measure(750, 400, 150, 180)
        backward = app.shot("measured_backwards")
        assert changed_pixels_in(before, backward, inside) > 5_000, "measuring backwards draws nothing"
        # the lower part of the info panel shows nothing that depends on the pointer
        assert changed_pixels_in(forward, backward, (0, 300, 270, 280)) == 0, "the rectangle runs over the info panel"


GOLD = (255, 215, 0)


def add_perspective_crop(app):
    app.click(895, 48)  # open the filter list
    app.settle(0.8)
    app.scroll_down(895, 200, 16)  # to the end of the list
    app.click(897, 393)  # add "Perspective crop"
    app.settle(1.0)
    app.move(500, 520)
    app.settle(1.0)


def test_perspective_crop_handles(binary):
    """The perspective crop shows its corner handles, they can be dragged, and the crop applied and added again."""
    with App(binary, "perspective_crop", [image("test.png")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        add_perspective_crop(app)
        added = app.shot("added")
        top_left = (370, 158, 30, 30)  # around the image's top left corner
        assert region_has_color(added, top_left, GOLD, 25), "the corner handle is not drawn"
        app.drag(385, 173, 440, 230)
        app.move(500, 520)
        app.settle(1.0)
        dragged = app.shot("dragged")
        assert not region_has_color(dragged, top_left, GOLD, 25), "the handle did not leave its corner"
        assert region_has_color(dragged, (425, 215, 30, 30), GOLD, 25), "the handle did not follow the drag"
        app.click(900, 127)  # "Apply"
        app.move(500, 520)
        app.settle(2.0)
        applied = app.shot("applied")
        assert "panicked" not in app.log(), "the app crashed when applying the crop"
        assert changed_pixels(dragged, applied) > 5_000, "applying the crop did not change the image"

        # a crop that is added after an applied one was removed starts with its handles again
        app.click(841, 91)  # the x in the header of the crop
        app.settle(1.5)
        add_perspective_crop(app)
        again = app.shot("added_again")
        assert region_has_color(again, top_left, GOLD, 25), "a crop added a second time has no handles"


TESTS = [
    test_perspective_crop_handles,
    test_shortcuts,
    test_start_with_idle_stdin,
    test_piped_file_names,
    test_reload_when_file_changes,
    test_image_cannot_get_lost,
    test_image_layouts,
    test_system_fonts_on_demand,
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
