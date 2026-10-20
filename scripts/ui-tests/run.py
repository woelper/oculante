#!/usr/bin/env python3
"""UI tests that drive a real Oculante window on a virtual display.

    python3 scripts/ui-tests/run.py [--binary target/debug/oculante] [test ...]

Each test checks behaviour that broke before. See README.md.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from uitest import (  # noqa: E402
    OUT,
    REPO,
    SCREEN,
    WINDOW_MANAGER,
    App,
    changed_pixels,
    changed_pixels_in,
    color_center,
    count_color,
    mean_level,
    find_slider,
    has_window_manager,
    image,
    panel_edge,
    region_has_color,
    rightmost_x,
)

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


class Skip(Exception):
    """The machine lacks something the test needs."""


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
# the icons of info and error messages (src/toasts.rs)
INFO_ICON = (57, 181, 255)
ERROR_ICON = (255, 77, 77)


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
        app.click(60, 341)  # open "Measure"
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
        # A thin white frame that leaves the image as it is, as in 0.9.6. rc.1 drew a
        # black frame over a darkened inside, which got lost on dark images (#813).
        left_edge = (440, 200, 25, 180)
        white = count_color(forward, left_edge, (255, 255, 255), 30) - count_color(before, left_edge, (255, 255, 255), 30)
        assert white > 150, f"the frame is not white, {white} more white pixels at its left edge"
        darker = mean_level(before, inside) - mean_level(forward, inside)
        assert darker < 3, f"the measured part of the image is darker by {darker:.1f}"
        # from the bottom right to the top left, ending on the info panel
        measure(750, 400, 150, 180)
        backward = app.shot("measured_backwards")
        assert changed_pixels_in(before, backward, inside) > 5_000, "measuring backwards draws nothing"
        # the lower part of the info panel shows nothing that depends on the pointer
        panel = (0, 300, panel_edge(forward) - 4, 280)
        assert changed_pixels_in(forward, backward, panel) == 0, "the rectangle runs over the info panel"


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


def test_relative_file_name(binary):
    """Started from a terminal in the folder of the image, with only its file name,
    the next image is the one after it. The same image came again."""
    folder = folder_of("relative_images", [("a.png", "test.png"), ("b.png", "test.png"), ("c.png", "test.png")])
    with App(binary, "relative_file_name", ["b.png"], cwd=folder) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log("Texture was dirty"), "the image never loaded"
        app.settle(1.0)
        app.key("Right")
        app.settle(1.5)
        assert shown_file(app) == "c.png", f"after b.png came {shown_file(app)}"


def test_several_files_as_arguments(binary):
    """Several files given on the command line are opened as a list (#382): the first
    is shown, the next key goes to the second."""
    with App(binary, "several_files", [image("test.png"), image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the first image never loaded"
        app.settle(1.0)
        assert shown_file(app) == "test.png", f"the first file is not shown: {shown_file(app)}"
        app.key("Right")
        app.settle(1.5)
        assert shown_file(app) == "moss.jpg", f"the next key did not go to the second file: {shown_file(app)}"


def test_first_and_last_image(binary):
    """End shows the last image of the folder and Home the first; Home on the first
    loads nothing again."""
    folder = folder_of("first_last_images", [(f"{n:02}.png", "test.png") for n in range(5)])
    with App(binary, "first_last", [os.path.join(folder, "02.png")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        app.key("End")
        app.settle(1.5)
        assert shown_file(app) == "04.png", f"End did not show the last image: {shown_file(app)}"
        app.key("Home")
        app.settle(1.5)
        assert shown_file(app) == "00.png", f"Home did not show the first image: {shown_file(app)}"
        loads = app.log().count(LOADED)
        app.key("Home")
        app.settle(1.5)
        assert app.log().count(LOADED) == loads, "Home on the first image loaded it again"


def test_ctrl_and_the_wheel_go_through_the_images(binary):
    """Ctrl and the wheel go to the next image and back, one a notch, as in 0.9.6.
    egui turns Ctrl and the wheel into zooming, they did nothing for a while. The
    wheel alone still zooms and stays on the image."""
    folder = folder_of("ctrl_wheel_images", [(f"{n:02}.png", "test.png") for n in range(4)])
    with App(binary, "ctrl_wheel", [os.path.join(folder, "01.png")]) as app:
        app.wait_window()
        app.move(0.5, 0.5)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        app.x("xdotool", "windowfocus", app.win)

        def ctrl_wheel(button):
            app.x("xdotool", "keydown", "Control_L")
            app.settle(0.3)
            app.x("xdotool", "click", button)
            app.settle(0.3)
            app.x("xdotool", "keyup", "Control_L")
            app.settle(1.5)

        ctrl_wheel("5")
        assert shown_file(app) == "02.png", f"Ctrl and the wheel down did not show the next image: {shown_file(app)}"
        ctrl_wheel("4")
        assert shown_file(app) == "01.png", f"Ctrl and the wheel up did not go back: {shown_file(app)}"
        before = app.shot("before_zoom")
        app.scroll_down(0.5, 0.5, 3)
        zoomed = app.shot("zoomed")
        assert shown_file(app) == "01.png", f"the wheel alone changed the image: {shown_file(app)}"
        assert changed_pixels(before, zoomed) > 5000, "the wheel alone did not zoom"


def test_save_as_writes_the_file(binary):
    """Save as shows the name of the file and the button to save below the files, and
    writes the file. The files took all the height of the dialog for a while, the row
    to save was cut off."""
    folder = folder_of("save_as_images", [("moss.jpg", "moss.jpg")])
    with App(binary, "save_as", [os.path.join(folder, "moss.jpg")], volatile={"last_open_directory": folder}) as app:
        app.wait_window()
        app.move(0.5, 0.5)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        app.key("e")
        app.settle(1.5)
        app.click(896, 343)  # "Save as..." in the edit panel
        app.settle(2.0)
        app.shot("dialog")
        # the name has the keyboard. In front of it, a name without an extension
        # gets ".jpg" right away, also in 0.9.6.
        app.key("Home")
        app.x("xdotool", "type", "--delay", "60", "copy_")
        app.settle(0.5)
        app.click(713, 531)  # "Save file"
        saved = os.path.join(folder, "copy_unnamed.png")
        for _ in range(20):
            if os.path.exists(saved) and os.path.getsize(saved) > 0:
                break
            time.sleep(0.5)
        assert os.path.exists(saved), "Save file did not write the file"
        with open(saved, "rb") as f:
            assert f.read(8) == b"\x89PNG\r\n\x1a\n", "the saved file is not a PNG"


def test_sequence_of_tiff_pages(binary):
    """A TIFF with three pages: the title counts them, the arrow keys go through them,
    a page seen before comes from the cache, and the list in the info panel shows a
    picture of each that shows it on a click."""
    folder = folder_of("sequence_images", [])
    colors = [(220, 40, 40), (40, 180, 60), (50, 70, 220)]
    pages = []
    # next to the TIFF, a file of one image
    subprocess.run(["convert", "-size", "50x50", "xc:white", os.path.join(folder, "single.png")], check=True)
    for number, ((width, height), color) in enumerate(zip([(400, 300), (300, 400), (500, 250)], colors)):
        pages.append(os.path.join(folder, f"{number}.png"))
        subprocess.run(["convert", "-size", f"{width}x{height}", f"xc:rgb{color}", pages[-1]], check=True)
    path = os.path.join(folder, "pages.tif")
    subprocess.run(["convert", *pages, "-type", "TrueColor", "-depth", "8", path], check=True)

    with App(binary, "sequence", [path], settings={"info_enabled": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the first page never loaded"
        app.settle(1.5)
        title = lambda: app.x("xdotool", "getwindowname", app.win).stdout.strip()
        assert title().endswith("[1/3]"), f"the title does not count the pages: {title()!r}"
        app.key("Down")
        app.settle(1.5)
        assert title().endswith("[2/3]"), f"Down did not go to the second page: {title()!r}"
        canvas = (300, 100, 700, 400)
        assert count_color(app.shot("second_page"), canvas, colors[1]) > 10000, "the second page is not shown"
        app.key("Up")
        app.settle(1.5)
        assert title().endswith("[1/3]"), f"Up did not go back: {title()!r}"
        # the second page again, from the cache this time
        app.key("Down")
        app.settle(1.5)
        assert "pages.tif (1)" in app.log().split("Cache hit for")[-1].splitlines()[0], "the second page was not taken from the cache"
        assert count_color(app.shot("second_page_again"), canvas, colors[1]) > 10000, "the second page is not shown again"
        app.key("Up")
        app.settle(1.5)

        # open the list, then scroll the panel down to it
        app.click(60, 549)
        app.settle(1.0)
        app.scroll_down(120, 150, 6)
        app.settle(2.5)
        shot = app.shot("sequence_list")
        panel = (0, 40, 250, 560)
        for number, color in enumerate(colors):
            assert count_color(shot, panel, color) > 500, f"no picture of page {number + 1} in the list"
        app.click(*color_center(shot, panel, colors[2]))
        app.settle(2.0)
        assert title().endswith("[3/3]"), f"a click on the picture did not show the page: {title()!r}"
        assert count_color(app.shot("third_page"), canvas, colors[2]) > 10000, "the third page is not shown"
        # the next file has one image, the title counts no more
        app.key("Right")
        app.settle(1.5)
        assert "[" not in title().split("|")[-1], f"the title still counts pages: {title()!r}"


def test_chainload_option_is_accepted(binary):
    """0.9.6 took -c, it relaunched itself with it on macOS. The option was gone and
    the app refused to start when it was given."""
    with App(binary, "chainload", ["-c", image("moss.jpg")]) as app:
        app.wait_window()
        assert app.wait_for_log("Received image 1000x750"), "the image given after -c was not opened"


def test_delete(binary):
    """Delete asks first and moves the file to the trash, Cancel keeps it, and
    Shift+Delete only takes the image off the list."""
    folder = folder_of("delete_images", [("a.png", "test.png"), ("b.png", "test.png"), ("c.png", "test.png")])
    with start(binary, "delete", os.path.join(folder, "b.png")) as app:
        trash = os.path.join(app.env["XDG_DATA_HOME"], "Trash", "files")
        app.key("Delete")
        app.settle(1.0)
        app.click(512, 380)  # "Yes"
        app.settle(1.5)
        assert not os.path.exists(os.path.join(folder, "b.png")), "the file is still there"
        assert os.path.isfile(os.path.join(trash, "b.png")), "the file is not in the trash"
        assert shown_file(app) == "c.png", f"after deleting b.png came {shown_file(app)}"

        app.key("Delete")
        app.settle(1.0)
        app.click(512, 420)  # "Cancel"
        app.settle(1.0)
        assert os.path.isfile(os.path.join(folder, "c.png")), "Cancel deleted the file"
        assert shown_file(app) == "c.png"

        since = len(app.log())
        app.key("shift+Delete")
        app.settle(1.5)
        assert "ClearImage" in app.matched_shortcuts(since)
        assert os.path.isfile(os.path.join(folder, "c.png")), "Shift+Delete deleted the file"
        assert shown_file(app) == "a.png", f"after taking c.png off the list came {shown_file(app)}"


def test_lossless_rotation_keys_and_quit(binary):
    """] and [ turn a JPEG on disk without recompressing it, q quits."""
    folder = folder_of("rotation_keys_images", [("moss.jpg", "moss.jpg")])
    path = os.path.join(folder, "moss.jpg")

    def size():
        out = subprocess.run(["identify", "-format", "%w %h", path], capture_output=True, text=True).stdout
        return tuple(int(v) for v in out.split())

    with start(binary, "rotation_keys", path) as app:
        assert size() == (1000, 750)
        loads = app.log().count(LOADED)
        app.key("bracketright")
        app.settle(2.0)
        width, height = size()
        assert width < height, f"] did not turn the file: {width}x{height}"
        assert app.log().count(LOADED) > loads, "the turned file was not shown again"
        app.key("bracketleft")
        app.settle(2.0)
        width, height = size()
        assert width > height, f"[ did not turn it back: {width}x{height}"
        assert {"LosslessRotateRight", "LosslessRotateLeft"} <= set(app.matched_shortcuts())
        app.key("q")
        end = time.time() + 5
        while app.running() and time.time() < end:
            time.sleep(0.2)
        assert not app.running(), "q did not quit"


def test_settings_of_0_9_6(binary):
    """The settings saved by 0.9.6 are used. They could not be read at all, the
    app started with the defaults and overwrote them."""
    with open(os.path.join(REPO, "res", "tests", "settings-0.9.6", "config.json")) as f:
        settings = json.load(f)
    assert settings["info_enabled"], "the info panel was open in the saved settings"
    with start(binary, "settings_default", image("moss.jpg")) as app:
        default = app.shot("default")
    with start(binary, "settings_096", image("moss.jpg"), settings=settings) as app:
        saved = app.shot("saved")
        log = app.log()
    assert changed_pixels_in(default, saved, (0, 40, 190, 540)) > 20_000, "the info panel of the saved settings is not open"
    assert "Could not load" not in log


def test_clipboard(binary):
    """Ctrl+C copies the image, Ctrl+Shift+C its path, Ctrl+V shows an image from
    the clipboard. Checked with xclip, the way other programs see the clipboard."""
    if shutil.which("xclip") is None:
        raise Skip("needs xclip")
    folder = folder_of("clipboard_images", [("test.png", "test.png")])
    path = os.path.join(folder, "test.png")
    pasted = os.path.join(folder, "pasted.png")
    subprocess.run(["convert", image("moss.jpg"), pasted], check=True)
    with start(binary, "clipboard", path) as app:

        def clipboard(target):
            return subprocess.run(
                ["xclip", "-selection", "clipboard", "-t", target, "-o"], env=app.env, capture_output=True, timeout=10
            ).stdout

        app.key("ctrl+c")
        app.settle(1.0)
        copied = os.path.join(folder, "copied.png")
        with open(copied, "wb") as f:
            f.write(clipboard("image/png"))
        size = subprocess.run(["identify", "-format", "%w %h", copied], capture_output=True, text=True).stdout
        assert size == "256 256", f"the clipboard holds no copy of the image: {size!r}"

        app.key("ctrl+shift+c")
        app.settle(1.0)
        assert clipboard("UTF8_STRING").decode() == path, "the path was not copied"

        xclip = subprocess.Popen(
            ["xclip", "-selection", "clipboard", "-t", "image/png", "-i", pasted], env=app.env, stderr=subprocess.DEVNULL
        )
        xclip.wait(timeout=10)
        app.settle(0.5)
        since = len(app.log())
        app.key("ctrl+v")
        app.settle(2.0)
        assert "Received image 1000x750" in app.log()[since:], "the image from the clipboard is not shown"

        # with the menu open, once: it pasted twice
        app.click(1005, 18)  # the menu
        app.settle(1.0)
        since = len(app.log())
        app.key("ctrl+v")
        app.settle(2.0)
        pastes = app.log()[since:].count("Received image")
        assert pastes == 1, f"one Ctrl+V with the menu open pasted {pastes} times"


def test_key_repeat(binary):
    """A key that is held down goes through the images of a folder."""
    folder = os.path.join(OUT, "key_repeat_images")
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    for number in range(40):
        shutil.copy(image("test.png"), os.path.join(folder, f"{number:02}.png"))
    with App(binary, "key_repeat", [os.path.join(folder, "00.png")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        since = len(app.log())
        app.key("Right", hold=2.0)
        app.settle(1.5)
        steps = app.matched_shortcuts(since).count("NextImage")
        assert steps >= 10, f"holding the key for two seconds went on by {steps} images only"
        # the title names the image that is shown
        title = app.x("xdotool", "getwindowname", app.win).stdout
        shown = re.search(r"(\d\d)\.png", title)
        assert shown, f"the title names no image: {title!r}"
        assert int(shown.group(1)) >= 10, f"the key went on {steps} times, but the image shown is {shown.group(0)}"


def test_zen_mode(binary):
    """Zen mode hides the bar and the panels, and leaving it brings them back."""
    with App(binary, "zen", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("i")
        app.settle(1.0)
        bar = (0, 0, 1026, 34)
        # the labels and the zoom preview, not the values: the colour under the pointer
        # is sampled every few frames and can still change
        panel = (0, 40, 160, 540)
        before = app.shot("before")
        app.key("z")
        app.settle(1.0)
        zen = app.shot("zen")
        assert changed_pixels_in(before, zen, bar) > 300, "the bar at the top is still there in zen mode"
        assert changed_pixels_in(before, zen, panel) > 20_000, "the info panel is still there in zen mode"
        app.key("z")
        app.settle(1.0)
        after = app.shot("after")
        assert changed_pixels_in(before, after, bar) == 0, "the bar at the top did not come back"
        assert changed_pixels_in(before, after, panel) < 500, "the info panel did not come back"


def test_paint_mode(binary):
    """In paint mode a drag over the image leaves a stroke. The next stroke leaves the
    first one where it is, a click leaves a dot, and a fading stroke fades. Drawn
    again all at once, everything looks the same as drawn a bit at a time."""
    with App(binary, "paint", [image("moss.jpg")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(896, 162)  # "Paint mode"
        app.settle(0.8)
        app.move(40, 560)
        app.settle(0.8)
        before = app.shot("before_stroke")
        app.drag(300, 250, 600, 400, steps=20)
        app.move(40, 560)
        app.settle(1.5)
        after = app.shot("after_stroke")
        changed = changed_pixels_in(before, after, (280, 230, 340, 190))
        assert changed > 500, f"the stroke changed only {changed} pixels of the image"
        # a drag that paints must not move the image as well
        assert changed_pixels_in(before, after, (130, 60, 150, 120)) == 0, "the drag moved the image"
        app.drag(300, 120, 600, 120, steps=20)
        app.move(40, 560)
        app.settle(1.5)
        second = app.shot("second_stroke")
        changed = changed_pixels_in(after, second, (280, 100, 340, 40))
        assert changed > 500, f"the second stroke changed only {changed} pixels of the image"
        # left of the edit panel, which lists the strokes
        gone = changed_pixels_in(after, second, (280, 230, 310, 190))
        assert gone == 0, f"{gone} pixels of the first stroke changed with the second one"
        # held down without moving
        app.move(200, 480)
        app.x("xdotool", "mousedown", "1")
        app.settle(0.3)
        app.x("xdotool", "mouseup", "1")
        app.move(40, 560)
        app.settle(1.5)
        dot = app.shot("dot")
        changed = changed_pixels_in(second, dot, (180, 460, 40, 40))
        assert changed > 30, f"the click changed only {changed} pixels of the image"
        app.click(733, 253)  # "Fade"
        app.settle(0.5)
        app.drag(160, 180, 560, 180, steps=20)
        app.move(40, 560)
        app.settle(1.5)
        faded = app.shot("faded")
        start = mean_level(faded, (150, 165, 60, 30)) - mean_level(dot, (150, 165, 60, 30))
        end = mean_level(faded, (500, 165, 60, 30)) - mean_level(dot, (500, 165, 60, 30))
        assert start > 20 and end < start / 3, f"the fading stroke is {start:.1f} brighter at its start and {end:.1f} at its end"
        # all edits off and on again: everything is drawn again at once
        app.click(617, 355)  # "Disable all edits"
        app.move(40, 560)
        app.settle(1.0)
        off = app.shot("edits_off")
        assert changed_pixels_in(faded, off, (140, 40, 455, 480)) > 1000, "the edits did not go off"
        app.click(617, 355)
        app.move(40, 560)
        app.settle(1.5)
        again = app.shot("drawn_again")
        changed = changed_pixels_in(faded, again, (140, 40, 455, 480))
        assert changed == 0, f"drawn again at once, {changed} pixels look different"


def test_paint_on_transparency(binary):
    """A stroke over transparent and opaque pixels shows on both, only its part of the
    image is uploaded, with the colors multiplied by alpha as for all of it."""
    folder = os.path.join(OUT, "paint_transparent_images")
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    disc = os.path.join(folder, "disc.png")
    subprocess.run(
        ["convert", "-size", "800x800", "xc:rgba(255,255,255,0)", "-fill", "black",
         "-draw", "circle 400,400 400,60", f"PNG32:{disc}"],
        check=True,
    )
    with App(binary, "paint_transparent", [disc]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(896, 85)  # "Paint mode"
        app.move(40, 560)
        app.settle(1.0)
        # the image starts at x 231, the disc at 273
        app.drag(235, 250, 580, 250, steps=20)
        app.move(40, 560)
        app.settle(1.5)
        painted = app.shot("painted")
        # on the background that shows through and on black, a soft white brush is
        # light gray: its dots add up to about 60 % alpha
        for name, box in (("transparent", (232, 235, 38, 30)), ("opaque", (320, 235, 250, 30))):
            light = count_color(painted, box, (160, 160, 160), 40)
            assert light > 50, f"the stroke shows {light} light pixels on the {name} part"
        # all edits off and on again: all of the image is uploaded
        app.click(627, 307)  # "Disable all edits"
        app.move(40, 560)
        app.settle(1.0)
        off = app.shot("edits_off")
        assert changed_pixels_in(painted, off, (0, 40, 600, 480)) > 1000, "the edits did not go off"
        app.click(627, 307)
        app.move(40, 560)
        app.settle(1.5)
        again = app.shot("uploaded_again")
        changed = changed_pixels_in(painted, again, (0, 40, 600, 480))
        assert changed == 0, f"uploaded all at once, {changed} pixels look different"


# where the filter search shows Annotate, the only filter that matches "annot"
ANNOTATE_BUTTON = (790, 108, 210, 18)
# the line around what is selected or pointed at
OUTLINE = (255, 200, 40)
# the header of the Annotations in the edit panel of moss.jpg, and the button to
# select and the tiles of the tools below it once it is open, scrolled to the top
ANNOTATIONS_HEADER = (890, 167)
SELECT_BUTTON = (780, 192, 222, 32)
ANNOTATE_TOOLS = {
    name: (815 + 80 * (i % 3), 250 + 46 * (i // 3))
    for i, name in enumerate(("arrow", "frame", "circle", "text", "pen", "redact"))
}


def test_annotate(binary):
    """With experimental features on, the Annotations in the edit panel draw an
    arrow, a circle, text and a redaction on the image. Meanwhile the keys are for the annotations: Delete
    removes the one that is selected, not the file, an arrow key moves it and no
    other image comes. What the mouse points at is outlined and moves with any tool,
    a double click changes a text, the middle button still moves the image, Ctrl+C
    copies it, and saved edits bring the annotations back."""
    folder = folder_of("annotate_images", [("a.jpg", "moss.jpg"), ("b.png", "test.png")])
    path = os.path.join(folder, "a.jpg")
    # the image, above the frame counter of debug builds
    image_box = (140, 40, 620, 480)
    with App(binary, "annotate", [path], settings={"experimental_features": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(896, 50)  # "Filters"
        app.click(896, 82)  # its search field
        app.x("xdotool", "type", "--delay", "60", "annot")
        app.settle(0.5)
        found = count_color(app.shot("search"), ANNOTATE_BUTTON, (220, 220, 220), 50)
        assert found == 0, "Annotate is among the filters, it has its own part of the panel"
        app.click(896, 50)  # "Filters" closes
        app.move(40, 560)
        app.settle(0.8)
        closed = app.shot("closed")
        app.click(*ANNOTATIONS_HEADER)
        app.move(40, 560)
        app.settle(0.8)
        tiles = (790, 185, 230, 100)
        changed = changed_pixels_in(closed, app.shot("opened"), tiles)
        assert changed > 3000, f"opening the Annotations shows no tools ({changed} pixels)"
        app.click(*ANNOTATE_TOOLS["arrow"])
        app.move(40, 560)
        app.settle(1.0)
        before = app.shot("before")
        app.move(250, 400)
        app.x("xdotool", "mousedown", "1")
        for i in range(1, 16):
            app.move(250 + i * 200 / 15, 400 - i * 150 / 15)
        app.settle(0.8)
        drawing = app.shot("drawing")
        app.x("xdotool", "mouseup", "1")
        app.move(40, 560)
        app.settle(1.0)
        arrow = app.shot("arrow")
        changed = changed_pixels_in(before, arrow, (240, 240, 220, 170))
        assert changed > 1000, f"the arrow changed only {changed} pixels"
        # the outline of what is selected, only once it is drawn
        arrow_box = (240, 240, 220, 170)
        yellow = [count_color(shot, arrow_box, OUTLINE, 30) for shot in (before, drawing, arrow)]
        assert yellow[1] - yellow[0] < 20, f"an outline while drawing: {yellow}"
        assert yellow[2] - yellow[0] > 100, f"no outline around the arrow that was drawn: {yellow}"
        assert changed_pixels_in(before, arrow, (600, 420, 150, 90)) == 0, "the drag moved the image"
        since = len(app.log())
        app.key("Right")
        assert not app.matched_shortcuts(since), "the arrow key went to the app, not to the arrow"
        # nothing selected, the arrow tool stays
        app.key("Escape")
        app.settle(0.8)
        unselected = app.shot("unselected")
        app.move(350, 325)
        app.settle(0.8)
        pointed = app.shot("pointed_at")
        changed = changed_pixels_in(unselected, pointed, (240, 240, 220, 170))
        assert changed > 200, f"pointing at the arrow outlines only {changed} pixels"
        # what is pointed at moves, with any tool
        app.drag(350, 325, 350, 425, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        moved = app.shot("moved")
        assert changed_pixels_in(before, moved, (240, 240, 220, 60)) == 0, "the arrow left a trace"
        assert changed_pixels_in(arrow, moved, (240, 400, 220, 110)) > 1000, "the arrow did not move there"
        # it is selected now
        app.key("Delete")
        app.move(40, 560)
        app.settle(1.0)
        gone = app.shot("gone")
        assert os.path.exists(path), "Delete removed the file"
        changed = changed_pixels_in(before, gone, image_box)
        assert changed == 0, f"{changed} pixels of the removed arrow are left"
        app.click(*ANNOTATE_TOOLS["circle"])
        app.drag(480, 100, 600, 180, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        circle = app.shot("circle")
        changed = changed_pixels_in(gone, circle, (470, 90, 140, 100))
        assert changed > 500, f"the circle changed only {changed} pixels"
        # the pen moves the circle by its line, it draws nothing
        app.click(*ANNOTATE_TOOLS["pen"])
        app.drag(480, 140, 480, 300, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        circle_moved = app.shot("circle_moved")
        changed = changed_pixels_in(gone, circle_moved, (470, 90, 140, 100))
        assert changed == 0, f"{changed} pixels are left where the circle was"
        changed = changed_pixels_in(gone, circle_moved, (470, 250, 140, 100))
        assert changed > 500, f"the moved circle changed only {changed} pixels"
        app.key("Delete")
        app.move(40, 560)
        app.settle(1.0)
        assert changed_pixels_in(before, app.shot("circle_gone"), image_box) == 0, "the circle is left"
        app.click(*ANNOTATE_TOOLS["text"])
        app.click(200, 100)
        app.x("xdotool", "type", "--delay", "80", "Hello")
        app.key("Return")
        app.move(40, 560)
        app.settle(1.0)
        text = app.shot("text")
        changed = changed_pixels_in(gone, text, (190, 80, 250, 70))
        assert changed > 300, f"the text changed only {changed} pixels"
        # a double click on it with another tool changes it
        app.click(*ANNOTATE_TOOLS["arrow"])
        app.move(215, 115)
        app.settle(0.5)
        app.x("xdotool", "click", "--repeat", "2", "--delay", "90", "1")
        app.settle(0.8)
        app.x("xdotool", "type", "--delay", "80", "!!")
        app.key("Return")
        app.move(40, 560)
        app.settle(1.0)
        changed = changed_pixels_in(text, app.shot("text_changed"), (190, 80, 250, 70))
        assert changed > 30, f"typing after a double click changed only {changed} pixels"
        app.click(*ANNOTATE_TOOLS["redact"])
        app.drag(450, 300, 650, 450, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        redacted = app.shot("redacted")
        changed = changed_pixels_in(text, redacted, (460, 310, 180, 130))
        assert changed > 10000, f"the redaction changed only {changed} pixels"
        if shutil.which("xclip"):
            app.key("ctrl+c")
            targets = app.x("xclip", "-selection", "clipboard", "-t", "TARGETS", "-o").stdout.split()
            assert "image/png" in targets, f"Ctrl+C copied no image, the clipboard has {targets}"
        # nothing selected, then selecting, then no tool: the panel gets shorter, the
        # image stays
        app.key("Escape")
        app.key("Escape")
        app.key("Escape")
        app.move(40, 560)
        app.settle(1.0)
        saved = app.shot("saved")
        app.scroll_down(890, 450, 6)
        app.click(891, 555)  # "Save edits", at the bottom of the panel
        assert os.path.exists(os.path.join(folder, "a.oculante")), "the edits were not saved"
        # back up, a tool on again
        app.move(890, 300)
        app.x("xdotool", "click", "--repeat", "8", "--delay", "60", "4")
        app.settle(0.8)
        app.click(*ANNOTATE_TOOLS["arrow"])
        app.move(300, 300)
        app.x("xdotool", "mousedown", "2")
        for i in range(1, 8):
            app.move(300 + i * 10, 300 + i * 5)
        app.x("xdotool", "mouseup", "2")
        app.move(40, 560)
        app.settle(1.0)
        panned = app.shot("panned")
        assert changed_pixels_in(saved, panned, image_box) > 50000, "the middle button did not move the image"
    with App(binary, "annotate_reopened", [path]) as app:
        app.wait_window()
        app.move(40, 560)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(2.0)
        reopened = app.shot("reopened")
        # the edits open the panel with a card for each annotation, the tools folded
        cards = count_color(reopened, (780, 185, 220, 95), (33, 33, 33), 2)
        assert cards > 10000, f"the cards of the saved annotations have only {cards} pixels"
        # the image is placed differently than before. Its only edits that show are
        # the annotations.
        app.click(785, 327)  # "Disable all edits"
        app.move(40, 560)
        app.settle(1.0)
        off = app.shot("edits_off")
        changed = changed_pixels_in(reopened, off, (10, 40, 740, 460))
        assert changed > 10000, f"the saved annotations change only {changed} pixels"
    # without experimental features there are no Annotations, their header has
    # one of the carets of the panel
    carets = (775, 40, 25, 560)
    counts = []
    for experimental in (True, False):
        settings = {"experimental_features": experimental}
        with App(binary, "annotate_not_offered", [os.path.join(folder, "b.png")], settings=settings) as app:
            app.wait_window()
            app.move(0.6, 0.6)
            assert app.wait_for_log(LOADED), "the image never loaded"
            app.key("e")
            app.move(40, 560)
            app.settle(1.0)
            counts.append(count_color(app.shot(f"panel_{experimental}"), carets, ACCENT, 40))
    assert counts[0] > counts[1] + 15, f"the carets of the panel with and without experimental features: {counts}"


# the image area right of the info panel, free of anything that depends on the pointer
# and above the frame counter a debug build draws at the bottom
IMAGE_AREA = (290, 40, 720, 470)
# where toasts appear
TOAST_AREA = (0, 300, 330, 290)


def folder_of(name, files):
    """A fresh folder in the output directory holding copies of test images, as (new name, source)."""
    folder = os.path.join(OUT, name)
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    for new_name, source in files:
        shutil.copy(image(source), os.path.join(folder, new_name))
    return folder


def shown_file(app):
    """The file named in the window title."""
    title = app.x("xdotool", "getwindowname", app.win).stdout
    found = re.search(r"(\S+\.(?:png|jpg|gif))", title)
    return os.path.basename(found.group(1)) if found else title.strip()


def start(binary, name, path, settings=None, screen=SCREEN):
    """The app with an image on screen. Closes it if that fails, so nothing is left behind."""
    app = App(binary, name, [path], settings=settings, screen=screen)
    try:
        app.wait_window()
        app.move(0.6, 0.6)
        # the first upload to the GPU, for still images and animations alike
        assert app.wait_for_log("Texture was dirty"), "the image never loaded"
        app.settle(1.0)
    except Exception:
        app.close()
        raise
    return app


def add_to_compare_list(app):
    app.key("i")
    app.settle(1.0)
    app.click(60, 551)  # open "Compare"
    app.settle(0.8)
    app.scroll_down(120, 300, 12)
    app.click(139, 275)  # "Add current image"
    app.settle(0.8)
    app.move(0.6, 0.6)
    app.settle(0.5)


def test_compare_list(binary):
    """An image from the compare list comes back with the zoom and position it was added with."""
    folder = folder_of("compare_images", [("00.jpg", "moss.jpg"), ("01.png", "test.png")])
    with start(binary, "compare", os.path.join(folder, "00.jpg")) as app:
        app.key("2")  # zoom to 200%
        add_to_compare_list(app)
        zoomed = app.shot("zoomed")
        app.key("Right")
        app.settle(1.5)
        assert shown_file(app) == "01.png", f"the next image did not load: {shown_file(app)}"
        other = app.shot("other")
        assert changed_pixels_in(zoomed, other, IMAGE_AREA) > 50_000, "the next image looks like the first"
        app.key("shift+c")  # next image of the compare list
        app.settle(2.0)
        back = app.shot("back")
        assert shown_file(app) == "00.jpg", f"the compared image did not come back: {shown_file(app)}"
        changed = changed_pixels_in(zoomed, back, IMAGE_AREA)
        assert changed < 500, f"the compared image is not at its stored zoom and position ({changed} pixels differ)"
    # with the view kept, the compared image takes the current view instead of its stored one
    with start(binary, "compare_keep_view", os.path.join(folder, "00.jpg"), settings={"compare_keep_view": True}) as app:
        app.key("2")
        add_to_compare_list(app)
        zoomed = app.shot("zoomed")
        app.key("Right")
        app.settle(1.5)
        app.key("shift+c")
        app.settle(2.0)
        back = app.shot("back")
        assert shown_file(app) == "00.jpg", f"the compared image did not come back: {shown_file(app)}"
        assert changed_pixels_in(zoomed, back, IMAGE_AREA) > 50_000, "the stored view was restored although the view is to be kept"


def make_animation(path):
    """A GIF of three colored frames, 200 ms each."""
    subprocess.run(
        ["convert", "-delay", "20", "-size", "200x200", "xc:red", "xc:lime", "xc:blue", "-loop", "0", path],
        check=True,
    )


def test_animation_plays_and_stops(binary):
    """An animation plays, and stops when another image is shown."""
    folder = folder_of("animation_images", [("01.png", "test.png")])
    make_animation(os.path.join(folder, "00.gif"))
    with start(binary, "animation", os.path.join(folder, "00.gif")) as app:
        frames = []
        # uneven pauses: a screenshot takes long under load, and shots that come once
        # per loop of the 600 ms animation would all show the same frame
        for n, pause in enumerate((0.1, 0.3, 0.2, 0)):
            frames.append(app.shot(f"frame_{n}"))
            time.sleep(pause)
        changes = [changed_pixels_in(a, b, IMAGE_AREA) for a, b in zip(frames, frames[1:])]
        assert max(changes) > 10_000, f"the animation does not play ({changes} pixels changed between frames)"
        app.key("Right")
        app.settle(1.5)
        assert shown_file(app) == "01.png", f"the next image did not load: {shown_file(app)}"
        still_a = app.shot("still_a")
        time.sleep(0.5)
        still_b = app.shot("still_b")
        assert changed_pixels_in(still_a, still_b, IMAGE_AREA) == 0, "the image keeps changing after the animation"
        assert not region_has_color(still_b, (460, 250, 100, 100), (255, 0, 0), 10), "a frame of the animation is still shown"


def test_animation_plays_as_often_as_the_file_asks(binary):
    """A GIF that is to be played once stops on its last frame and says so."""
    folder = folder_of("play_once_images", [])
    path = os.path.join(folder, "once.gif")
    # with a loop of 1 ImageMagick writes no loop block, which means a single play
    subprocess.run(
        ["convert", "-delay", "20", "-size", "200x200", "xc:red", "xc:lime", "xc:blue", "-loop", "1", path],
        check=True,
    )
    with start(binary, "play_once", path) as app:
        time.sleep(1.5)
        app.settle(0.5)
        end = app.shot("end")
        assert region_has_color(end, (460, 250, 100, 100), (0, 0, 255), 10), "the last frame is not shown"
        assert region_has_color(end, TOAST_AREA, INFO_ICON, 30), "no message that the animation ended"
        time.sleep(1.0)
        later = app.shot("later")
        assert changed_pixels_in(end, later, IMAGE_AREA) == 0, "the animation goes on after its last play"


def test_keep_view(binary):
    """With "keep view" the next image is shown at the same zoom, without it is fitted again."""
    folder = folder_of("keep_view_images", [("00.png", "test.png"), ("01.png", "test.png")])
    for keep in (True, False):
        with start(binary, f"keep_view_{keep}", os.path.join(folder, "00.png"), settings={"keep_view": keep}) as app:
            app.key("2")
            zoomed = app.shot("zoomed")
            app.key("Right")
            app.settle(1.5)
            assert shown_file(app) == "01.png", f"the next image did not load: {shown_file(app)}"
            after = app.shot("after")
            changed = changed_pixels_in(zoomed, after, IMAGE_AREA)
            if keep:
                assert changed == 0, f"the view was not kept ({changed} pixels differ)"
            else:
                assert changed > 10_000, "the view was kept although it should be reset"


def fit_again(app):
    """Fits the image again. Images are fitted into the space the panels leave, so one
    fitted before the edit panel opened sits elsewhere than the next one."""
    app.key("v")
    app.move(0.4, 0.5)
    app.settle(1.0)


def brighten(app):
    """Adds a brightness filter and drags its slider up."""
    open_edit_panel(app)
    app.click(897, 117)  # add "Brightness"
    app.settle(0.8)
    app.move(0.4, 0.5)
    app.settle(0.8)
    # the row of the slider depends on the filters that are already there
    slider = find_slider(app.shot("slider"), (770, 60, 200, 400), ACCENT)
    assert slider is not None, "the slider is missing"
    handle, y = slider
    app.drag(handle - 4, y, handle + 60, y)
    app.move(0.4, 0.5)
    app.settle(1.5)


def test_keep_edits(binary):
    """With "keep edits" the filters stay on the next image, without them it is shown as it is."""
    folder = folder_of("keep_edits_images", [("00.jpg", "moss.jpg"), ("01.jpg", "moss.jpg")])
    for keep in (True, False):
        with start(binary, f"keep_edits_{keep}", os.path.join(folder, "00.jpg"), settings={"keep_edits": keep}) as app:
            # the image, above the frame counter of a debug build
            area = (120, 40, 640, 470)
            plain = app.shot("plain")
            brighten(app)
            fit_again(app)
            edited = app.shot("edited")
            assert changed_pixels_in(plain, edited, area) > 50_000, "the filter changed nothing"
            app.key("Right")
            app.settle(2.0)
            assert shown_file(app) == "01.jpg", f"the next image did not load: {shown_file(app)}"
            after = app.shot("after")
            changed = changed_pixels_in(edited, after, area)
            if keep:
                assert changed < 500, f"the edits were not kept ({changed} pixels differ)"
            else:
                assert changed > 50_000, "the edits stayed although they should be dropped"


def test_image_info_is_computed_when_shown(binary):
    """The histogram and the other numbers of the info panel are computed once for
    every version of the image, and only while the panel is open."""
    computed = "Sending extended info"
    with start(binary, "info_when_shown", image("moss.jpg")) as app:
        app.settle(1.5)
        assert app.log().count(computed) == 0, "computed with the info panel closed"
        app.key("i")
        app.settle(1.5)
        assert app.log().count(computed) == 1, "not computed when the info panel opened"
        brighten(app)
        assert app.log().count(computed) == 1, "computed again for every change of an edit"
        app.click(890, 306)  # "Apply all edits"
        app.settle(1.5)
        assert app.log().count(computed) == 2, "not computed again after the edits were applied"


def test_info_panel_resizes(binary):
    """The info panel can be dragged narrower and wider, and its content follows.
    The zoom preview and the histogram kept their size: a narrower panel showed a
    black strip while dragging and jumped back when let go, and the values ran over
    their labels."""

    def drag_edge(app, x0, x1, label, y=420):
        app.move(x0, y)
        app.x("xdotool", "mousedown", "1")
        for i in range(1, 11):
            app.move(x0 + (x1 - x0) * i / 10, y)
        app.settle(0.6)
        held = app.shot(f"{label}_held")
        app.x("xdotool", "mouseup", "1")
        app.settle(0.8)
        return held, app.shot(f"{label}_released")

    panel_background = (25, 25, 25)
    with start(binary, "info_panel_resizes", image("moss.jpg")) as app:
        app.key("i")
        app.settle(1.5)
        edge = panel_edge(app.shot("open"))
        assert edge and 200 < edge < 400, f"the edge of the info panel is at {edge}"

        # narrower than the panel can be
        held, released = drag_edge(app, edge, 120, "narrow")
        # Inside the panel, left of its edge, the strip had the color of the empty
        # canvas. Right of the edge that color is right, where the panel was before.
        held_edge, narrow = panel_edge(held), panel_edge(released)
        black = count_color(held, (110, 60, held_edge - 112, 500), (8, 8, 8), tolerance=4)
        assert black < 2000, f"a black strip of {black} pixels inside the panel while dragging it narrower"
        # it opens at 260 points and goes down to 240
        assert narrow <= edge - 15, f"the panel did not get narrower: {edge} -> {narrow}"
        assert abs(narrow - held_edge) <= 2, f"the panel jumped from {held_edge} to {narrow} when let go"

        # wider: the zoom preview grows with the panel
        _, released = drag_edge(app, narrow, 560, "wide")
        wide = panel_edge(released)
        assert abs(wide - 560) <= 6, f"the panel is {wide} wide, it was dragged to 560"
        assert not region_has_color(released, (wide - 70, 340, 30, 20), panel_background, tolerance=4), (
            "the zoom preview did not grow with the panel"
        )


def test_info_panel_holds_still(binary):
    """The info panel holds still at every height of the window. The zoom preview
    and the histogram are as tall as they are wide, and the scroll bar took width
    when it came: at heights where that made them fit, the bar went away and came
    back all the time and the panel flickered. With Measure, an experimental
    feature, the panel was that tall on a 14" MacBook."""

    def resize(app, height):
        app.x("xdotool", "windowsize", app.win, str(app.geom["WIDTH"]), str(height))
        app.settle(0.8)

    def has_bar(shot, edge):
        # the handle of the scroll bar, a few pixels left of the edge
        return count_color(shot, (edge - 18, 60, 10, 300), (60, 60, 60), tolerance=3) > 300

    settings = {"experimental_features": True}
    # taller than the panel
    with start(binary, "info_panel_holds_still", image("moss.jpg"), settings, screen=(1400, 1300)) as app:
        app.key("i")
        app.settle(1.5)
        edge = panel_edge(app.shot("open"))
        # the lowest window that shows all of the panel
        low, high = 600, 1250
        resize(app, low)
        assert has_bar(app.shot("low"), edge), "no scroll bar in a low window"
        resize(app, high)
        assert not has_bar(app.shot("high"), edge), "a scroll bar in a window taller than the panel"
        while high - low > 1:
            middle = (low + high) // 2
            resize(app, middle)
            if has_bar(app.shot(f"search_{middle}"), edge):
                low = middle
            else:
                high = middle
        # around it nothing moves in the panel while the pointer rests
        for height in range(high - 9, high + 7, 3):
            resize(app, height)
            first = app.shot(f"h{height}_a")
            time.sleep(0.2)
            second = app.shot(f"h{height}_b")
            changed = changed_pixels_in(first, second, (0, 40, edge, height - 45))
            assert changed == 0, f"the info panel keeps changing in a window {height} high ({changed} pixels)"
        # and the bar does not change the size of what is in it
        resize(app, high - 3)
        with_bar = app.shot("with_bar")
        resize(app, high + 3)
        without_bar = app.shot("without_bar")
        assert has_bar(with_bar, edge) and not has_bar(without_bar, edge), "the bar did not come and go"
        # text is drawn a shade different at another height of the window
        changed = changed_pixels_in(with_bar, without_bar, (0, 40, edge - 20, 560), fuzz=3)
        assert changed == 0, f"the panel changed with the scroll bar ({changed} pixels)"


def test_annotation_cards(binary):
    """Each annotation has a card in the edit panel, the one in front first. What is
    set while nothing is selected is for the next one, an outline here. A card sends
    its annotation backward, hides it and removes it, and Esc closes a list of a card
    without dropping the selection. The tools fold away, the cards stay."""
    white = (255, 255, 255)
    frame_box = (470, 90, 140, 100)
    image_box = (140, 40, 620, 480)
    with App(binary, "annotation_cards", [image("moss.jpg")], settings={"experimental_features": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(*ANNOTATIONS_HEADER)
        app.click(*ANNOTATE_TOOLS["frame"])
        app.move(40, 560)
        app.settle(1.0)
        before = app.shot("before")
        # the outline of the next frame, on its card below the tools
        app.drag(876, 513, 894, 513, steps=6)
        app.drag(480, 100, 600, 180, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        frame = app.shot("frame")
        outline = count_color(frame, frame_box, white, 10) - count_color(before, frame_box, white, 10)
        assert outline > 1000, f"the frame has an outline of only {outline} pixels"
        # a circle over it, then behind it
        app.click(*ANNOTATE_TOOLS["circle"])
        app.drag(500, 120, 640, 220, steps=10)
        app.move(40, 560)
        app.settle(1.0)
        circle = app.shot("circle")
        overlap = (500, 120, 100, 60)
        app.click(803, 417)  # "Send backward" on the card of the circle
        app.move(40, 560)
        app.settle(1.0)
        behind = app.shot("behind")
        changed = changed_pixels_in(circle, behind, overlap)
        assert changed > 100, f"sending the circle backward changed only {changed} pixels"
        # the frame, now first, hides and comes back
        corner = (470, 90, 25, 25)
        app.click(822, 417)
        app.move(40, 560)
        app.settle(1.0)
        hidden = app.shot("frame_hidden")
        assert changed_pixels_in(before, hidden, corner) == 0, "the hidden frame is drawn"
        app.click(822, 417)
        app.move(40, 560)
        app.settle(1.0)
        assert changed_pixels_in(behind, app.shot("frame_shown"), frame_box) == 0, "the frame did not come back"
        # the circle goes by its card, below and right of the frame it was alone
        app.click(842, 465)
        app.move(40, 560)
        app.settle(1.0)
        removed = app.shot("circle_removed")
        for alone in [(622, 150, 30, 60), (520, 202, 80, 30)]:
            assert changed_pixels_in(before, circle, alone) > 300, f"no circle at {alone}"
            changed = changed_pixels_in(before, removed, alone)
            assert changed == 0, f"{changed} pixels of the removed circle are left at {alone}"
        # Esc closes the list of fonts of a text, which stays selected
        app.click(*ANNOTATE_TOOLS["text"])
        app.click(200, 300)
        app.x("xdotool", "type", "--delay", "80", "Hi")
        app.key("Return")
        app.move(40, 560)
        app.settle(1.0)
        text_box = (170, 270, 90, 90)
        text = app.shot("text")
        app.click(980, 501)  # its font
        app.settle(0.8)
        listed = app.shot("list")
        assert changed_pixels_in(text, listed, (790, 280, 230, 200)) > 1000, "no list of fonts"
        app.key("Escape")
        app.move(40, 560)
        app.settle(1.0)
        closed = app.shot("list_closed")
        assert changed_pixels_in(text, closed, text_box) == 0, "Esc on the list dropped the selection"
        # the next one does
        app.key("Escape")
        app.move(40, 560)
        app.settle(1.0)
        unselected = app.shot("unselected")
        changed = changed_pixels_in(closed, unselected, text_box)
        assert changed > 50, f"the outline of the selected text is only {changed} pixels"
        # the tools fold away, the cards stay like those of the filters
        app.click(*ANNOTATIONS_HEADER)
        app.move(40, 560)
        app.settle(1.0)
        folded = app.shot("folded")
        tools = changed_pixels_in(unselected, folded, (780, 190, 225, 90))
        assert tools > 5000, f"folding changed only {tools} pixels of the tools"
        cards = count_color(folded, (780, 180, 220, 300), (33, 33, 33), 2)
        assert cards > 10000, f"the folded Annotations have only {cards} pixels of cards"
        assert changed_pixels_in(unselected, folded, image_box) == 0, "folding changed the image"



def test_annotation_select(binary):
    """With nothing selected Esc goes from a tool that draws to Select, which is
    tinted. In it a box selects what lies wholly inside, also with Shift on top of
    what is selected, a click besides everything drops the selection, and the cards
    of what is selected are tinted. Esc then stops annotating, as folding the
    Annotations does, and opening them again selects as long as there are any."""
    tint = (77, 26, 41)
    frames = [(190, 90, 120, 90), (390, 90, 120, 90)]
    circle_box = (240, 290, 170, 120)
    panel = (775, 40, 240, 560)

    def selected(shot, box):
        # its dashed line, against the shot where nothing is
        return changed_pixels_in(drawn, shot, box) > 150

    with App(binary, "annotation_select", [image("moss.jpg")], settings={"experimental_features": True}) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.key("e")
        app.settle(1.0)
        app.click(*ANNOTATIONS_HEADER)
        app.move(40, 560)
        app.settle(1.0)
        before = app.shot("before")
        assert count_color(before, SELECT_BUTTON, ACCENT, 30) < 20, "Select is on with nothing to select"
        app.click(*ANNOTATE_TOOLS["frame"])
        app.drag(200, 100, 300, 170, steps=10)
        app.drag(400, 100, 500, 170, steps=10)
        app.click(*ANNOTATE_TOOLS["circle"])
        app.drag(250, 300, 400, 400, steps=10)
        app.key("Escape")
        app.key("Escape")
        app.move(40, 560)
        app.settle(1.0)
        drawn = app.shot("select")
        lit = count_color(drawn, SELECT_BUTTON, ACCENT, 30)
        assert lit > 50, f"Select is not on after Esc, {lit} pixels of it are tinted"
        # a box around both frames, shown while it is dragged
        app.move(180, 80)
        app.x("xdotool", "mousedown", "1")
        for i in range(1, 11):
            app.move(180 + i * 34, 80 + i * 11)
        app.settle(0.8)
        boxing = app.shot("boxing")
        app.x("xdotool", "mouseup", "1")
        edge = count_color(boxing, (175, 75, 350, 120), ACCENT, 30)
        assert edge > 400, f"the box has only {edge} pixels"
        app.move(40, 560)
        app.settle(1.0)
        boxed = app.shot("boxed")
        assert all(selected(boxed, box) for box in frames), "the box did not select both frames"
        assert not selected(boxed, circle_box), "the box selected the circle, which is not in it"
        tinted = count_color(boxed, panel, tint, 8)
        assert tinted > 3000, f"the cards of what is selected have {tinted} tinted pixels"
        # a click besides everything drops it
        app.click(650, 500)
        app.move(40, 560)
        app.settle(1.0)
        dropped = app.shot("dropped")
        assert not any(selected(dropped, box) for box in frames), "a click besides them kept the frames"
        # the circle by a click on its line, then the frames with Shift on top
        app.click(250, 350)
        app.x("xdotool", "keydown", "shift")
        app.drag(180, 80, 520, 190, steps=10)
        app.x("xdotool", "keyup", "shift")
        app.move(40, 560)
        app.settle(1.0)
        added = app.shot("added")
        assert all(selected(added, box) for box in frames + [circle_box]), "Shift and a box did not add"
        # nothing selected, then no annotating
        app.key("Escape")
        app.key("Escape")
        app.move(40, 560)
        app.settle(1.0)
        stopped = app.shot("stopped")
        assert count_color(stopped, SELECT_BUTTON, ACCENT, 30) < 20, "Select is still on after Esc"
        # folded there is none, open again it selects
        app.click(*ANNOTATIONS_HEADER)
        app.settle(0.8)
        app.click(*ANNOTATIONS_HEADER)
        app.move(40, 560)
        app.settle(1.0)
        reopened = app.shot("reopened")
        assert count_color(reopened, SELECT_BUTTON, ACCENT, 30) > 50, "Select is off after opening again"
        app.drag(180, 80, 520, 190, steps=10)
        app.key("Delete")
        app.move(40, 560)
        app.settle(1.0)
        deleted = app.shot("deleted")
        for box in frames:
            changed = changed_pixels_in(before, deleted, box)
            assert changed == 0, f"{changed} pixels of the deleted frames are left at {box}"
        changed = changed_pixels_in(before, deleted, circle_box)
        assert changed > 500, f"the circle changed only {changed} pixels, it should stay"

def test_histogram_shows_values_on_hover(binary):
    """Hovering over the histogram shows the channel, the value and the number of
    pixels. egui_plot 0.37 shows them only with a label formatter, they were gone."""
    with start(binary, "histogram_hover", image("moss.jpg")) as app:
        app.key("i")
        app.settle(1.5)
        app.scroll_down(120, 300, 12)  # down to the histogram
        app.move(60, 470)  # the "Pixels" row above it
        app.settle(0.8)
        above = app.shot("above")
        app.move(60, 540)  # on the histogram, near the lines
        app.settle(0.8)
        hover = app.shot("hover")
    # right of the pointer: the tooltip, or only the line of the crosshair
    changed = changed_pixels_in(above, hover, (70, 480, 180, 80))
    assert changed > 1500, f"no values next to the pointer ({changed} pixels changed)"


def test_mipmaps_switched_on_and_off(binary):
    """Mipmaps can be switched on and off with an image on screen. A texture is
    allocated once with all its mipmap levels or with none, so switching needs new
    textures; one without levels that is asked for mipmaps draws black."""

    def toggle_mipmaps(app, on):
        app.click(1005, 18)  # the menu
        app.settle(0.8)
        app.click(940, 218)  # "Preferences"
        app.settle(1.0)
        # the list keeps its position when the preferences open again
        app.move(480, 300)
        for _ in range(12):
            app.x("xdotool", "click", "4")
        app.scroll_down(480, 300, 4)
        app.click(752, 350)  # "Use mipmaps"
        app.settle(0.8)
        checked = region_has_color(app.shot(f"mipmaps_{on}"), (742, 340, 20, 20), ACCENT, 60)
        assert checked == on, f"the mipmap setting is {'off' if on else 'on'}"
        app.click(777, 32)  # close the preferences
        app.move(300, 560)
        app.settle(1.5)

    area = (440, 250, 140, 100)  # inside the image after zooming out
    with start(binary, "mipmaps", image("moss.jpg"), settings={"use_mipmaps": False}) as app:
        app.scroll_down(512, 300, 1)  # zoom out, where mipmaps are used
        app.move(300, 560)
        app.settle(1.5)
        plain = app.shot("plain")
        toggle_mipmaps(app, True)
        mipmapped = app.shot("mipmapped")
        toggle_mipmaps(app, False)
        again = app.shot("plain_again")
    black = count_color(mipmapped, area, (0, 0, 0), tolerance=6)
    assert black < 500, f"the image is black with mipmaps switched on ({black} pixels)"
    assert changed_pixels_in(plain, mipmapped, area) > 0, "switching mipmaps on changed nothing"
    changed = changed_pixels_in(plain, again, area)
    assert changed == 0, f"the image differs after switching mipmaps off again ({changed} pixels)"


def test_image_is_fitted_next_to_the_panels(binary):
    """An image is fitted into the space the bar and the panels leave, as in 0.9.6.
    It was fitted to the whole window and its left part went under the info panel."""
    folder = folder_of("fit_panels_images", [])
    path = os.path.join(folder, "wide.png")
    # wide, so the width decides the scale, with a red strip at the left edge
    subprocess.run(
        ["convert", "-size", "2000x500", "xc:gray50", "-fill", "red", "-draw", "rectangle 0,0 99,499", path],
        check=True,
    )
    with start(binary, "fit_panels", path, settings={"info_enabled": True}) as app:
        app.move(900, 560)
        app.settle(1.0)
        shot = app.shot("fitted")
    edge = panel_edge(shot)
    assert edge and 200 < edge < 400, f"the edge of the info panel is at {edge}"
    assert region_has_color(shot, (edge + 2, 250, 30, 100), (255, 0, 0), 30), (
        "the left edge of the image is not next to the info panel"
    )


def test_saved_edits_come_back(binary):
    """Edits saved with "Save edits" are applied again when the image is opened later."""
    folder = folder_of("saved_edits_images", [("moss.jpg", "moss.jpg")])
    path = os.path.join(folder, "moss.jpg")
    # the image, left of the edit panel, whose width can change a little
    area = (130, 50, 560, 450)
    with start(binary, "saved_edits", path) as app:
        plain = app.shot("plain")
        brighten(app)
        app.click(890, 486)  # "Save edits"
        app.settle(1.0)
        fit_again(app)
        edited = app.shot("edited")
    assert os.path.isfile(os.path.join(folder, "moss.oculante")), "no file with the edits was written"
    with start(binary, "saved_edits_again", path) as app:
        app.move(0.4, 0.5)
        app.settle(1.5)
        # the edit panel opens with the saved edits, after the image was fitted
        fit_again(app)
        again = app.shot("again")
    assert changed_pixels_in(plain, again, area) > 50_000, "the saved edits were not applied"
    # The edit panel can open a few pixels narrower, the image is then scaled a hair
    # differently, so the brightness is compared, not each pixel
    level, wanted = mean_level(again, area), mean_level(edited, area)
    assert abs(level - wanted) < 2, f"the image looks different from when the edits were saved ({level:.1f} instead of {wanted:.1f})"


def test_single_frame_gif_is_editable(binary):
    """A GIF with a single frame is a still image, so a filter changes it."""
    folder = folder_of("single_frame_gif_images", [])
    path = os.path.join(folder, "moss.gif")
    subprocess.run(["convert", image("moss.jpg"), path], check=True)
    with start(binary, "single_frame_gif", path) as app:
        area = (120, 40, 640, 470)
        plain = app.shot("plain")
        brighten(app)
        edited = app.shot("edited")
        changed = changed_pixels_in(plain, edited, area)
        assert changed > 50_000, f"the filter changed only {changed} pixels of a single-frame GIF"


def differing_pixels_near(screenshot, expected, x, y, width, height, margin=2):
    """Pixels of an image file that differ on screen, where it fits best within margin of x, y."""
    crop = screenshot[:-4] + "_at.png"
    best = None
    for dy in range(-margin, margin + 1):
        for dx in range(-margin, margin + 1):
            box = f"{width}x{height}+{x + dx}+{y + dy}"
            subprocess.run(["convert", screenshot, "-crop", box, "+repage", crop], check=True)
            result = subprocess.run(
                ["compare", "-metric", "AE", "-fuzz", "2%", crop, expected, "null:"],
                capture_output=True,
                text=True,
            )
            off = int(float(result.stderr.split()[0]))
            best = off if best is None else min(best, off)
    return best


def test_actual_size_is_pixel_exact(binary):
    """At 100% the screen shows the pixels of the file, also for an odd width and height."""
    folder = folder_of("actual_size_images", [])
    path = os.path.join(folder, "odd.png")
    width, height = 301, 201
    subprocess.run(["convert", image("moss.jpg"), "-resize", f"{width}x{height}!", path], check=True)
    for linear in (True, False):
        settings = {"linear_mag_filter": linear, "linear_min_filter": linear, "use_mipmaps": False}
        with start(binary, f"actual_size_{'linear' if linear else 'nearest'}", path, settings=settings) as app:
            # zoom to 100% around the middle of the space below the bar, where the
            # image is centred
            app.move(513, 318)
            app.key("1")
            app.x("xdotool", "mousemove", "1020", "595")
            app.settle(1.0)
            shot = app.shot("actual_size")
        off = differing_pixels_near(shot, path, 513 - width // 2, 318 - height // 2, width, height)
        assert off < width * height / 1000, f"{off} of {width * height} pixels differ from the file at 100% (linear filter: {linear})"


def test_overtaken_load(binary):
    """A slow image that is skipped over does not show up later and overwrite the image that was chosen."""
    folder = folder_of("overtaken_images", [("00.png", "test.png"), ("02.jpg", "moss.jpg")])
    subprocess.run(["convert", "-size", "5000x5000", "xc:gray50", os.path.join(folder, "01.png")], check=True)
    with start(binary, "overtaken", os.path.join(folder, "00.png")) as app:
        app.key("Right", hold=0.05)
        app.key("Right", hold=0.05)
        app.settle(6.0)
        assert shown_file(app) == "02.jpg", f"the chosen image is not shown: {shown_file(app)}"
        first = app.shot("first")
        time.sleep(3.0)
        app.settle(0.5)
        assert shown_file(app) == "02.jpg", f"a skipped image came back: {shown_file(app)}"
        second = app.shot("second")
        assert changed_pixels_in(first, second, IMAGE_AREA) == 0, "the image changed after the skipped one finished loading"
        assert not region_has_color(second, (600, 250, 100, 100), (128, 128, 128), 3), "the skipped gray image is shown"


def test_load_error_shows_toast(binary):
    """A file that can not be decoded shows an error toast instead of failing silently."""
    with App(binary, "load_error", [image("mp4_ex-signature.gif")]) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        app.settle(1.5)
        shot = app.shot("error")
        assert region_has_color(shot, TOAST_AREA, ERROR_ICON, 30), "no error toast is shown"
        assert app.running(), "the app quit on a broken file"


def test_channel_view(binary):
    """Showing a single channel changes the image, showing all channels again restores it."""
    with start(binary, "channels", image("moss.jpg")) as app:
        rgba = app.shot("rgba")
        app.key("r")
        app.settle(1.0)
        red = app.shot("red")
        assert changed_pixels_in(rgba, red, IMAGE_AREA) > 100_000, "the red channel looks like the full image"
        app.key("c")
        app.settle(1.0)
        back = app.shot("back")
        assert changed_pixels_in(rgba, back, IMAGE_AREA) == 0, "the full image did not come back"


def test_transparent_edges_when_zoomed_out(binary):
    """Zoomed out, the color under transparent pixels does not show at the edges of
    what is opaque, with and without a color. A channel on its own still shows it."""
    folder = os.path.join(OUT, "transparent_edges_images")
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    # black on white that is fully transparent, as gray with alpha and as RGBA
    for name, kind in (("gray", "PNG"), ("rgba", "PNG32")):
        disc = os.path.join(folder, f"{name}.png")
        subprocess.run(
            ["convert", "-size", "800x800", "xc:rgba(255,255,255,0)", "-fill", "black",
             "-draw", "circle 400,400 400,60", f"{kind}:{disc}"],
            check=True,
        )
        with start(binary, f"transparent_edges_{name}", disc) as app:
            app.scroll_down(512, 300, 1)  # zoom out, where mipmaps are used
            app.move(40, 560)
            app.settle(1.0)
            # lighter than the background, which is (30, 30, 30)
            light = count_color(app.shot("all_channels"), IMAGE_AREA, (255, 255, 255), 215)
            assert light == 0, f"{light} pixels at the edges of the {name} image are lighter than the background"
            app.key("u")
            app.settle(1.0)
            white = count_color(app.shot("colors"), IMAGE_AREA, (255, 255, 255), 4)
            assert white > 1000, f"the colors of the {name} image without alpha show no white ({white} pixels)"
            app.key("c")
            app.settle(1.0)
            light = count_color(app.shot("all_channels_again"), IMAGE_AREA, (255, 255, 255), 215)
            assert light == 0, f"{light} pixels at the edges are lighter after showing all channels again ({name})"


def needs_window_manager():
    if not has_window_manager():
        raise Skip(f"needs the window manager {WINDOW_MANAGER}")


def test_fullscreen(binary):
    """The window fills the screen in fullscreen and returns to where it was. The pixel
    under the pointer stays under it, with the key and with a double click."""
    needs_window_manager()
    with App(binary, "fullscreen", [image("moss.jpg")], window_manager=True) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        start = dict(app.geom)
        # what is around the pointer, on the screen
        around = (start["X"] + int(0.6 * start["WIDTH"]) - 30, start["Y"] + int(0.6 * start["HEIGHT"]) - 30, 60, 60)
        windowed = app.shot("windowed")
        app.key("f")
        app.settle(1.5)
        full = app.update_geometry()
        assert (full["X"], full["Y"], full["WIDTH"], full["HEIGHT"]) == (0, 0, *SCREEN), f"not fullscreen: {full}"
        shot = app.shot("fullscreen")
        assert not region_has_color(shot, (0, 0, 40, 40), (0, 0, 0), 4), "the window does not cover the screen"
        moved = changed_pixels_in(windowed, shot, around)
        assert moved < 20, f"{moved} pixels around the pointer changed going fullscreen"
        app.key("f")
        app.settle(1.5)
        assert app.update_geometry() == start, f"the window came back as {app.geom}, it was {start}"
        moved = changed_pixels_in(windowed, app.shot("back"), around)
        assert moved < 20, f"{moved} pixels around the pointer changed leaving fullscreen"
        for label in ("double_click_in", "double_click_out"):
            app.x("xdotool", "click", "--repeat", "2", "--delay", "80", "1")
            app.settle(1.5)
            moved = changed_pixels_in(windowed, app.shot(label), around)
            assert moved < 20, f"{moved} pixels around the pointer changed after a double click ({label})"
        assert app.update_geometry() == start, f"the window came back as {app.geom} after double clicks, it was {start}"


def test_borderless(binary):
    """Without a border the window has no title bar, can be dragged by its own bar and closed with its own button."""
    needs_window_manager()
    with App(binary, "bordered", [image("moss.jpg")], window_manager=True) as app:
        app.wait_window()
        assert app.frame_extents()[2] > 0, "no title bar by default, the window manager does not decorate"
    with App(binary, "borderless", [image("moss.jpg")], settings={"borderless": True}, window_manager=True) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        assert app.frame_extents() == [0, 0, 0, 0], f"the window still has a border: {app.frame_extents()}"
        start = dict(app.geom)
        app.drag(500, 18, 620, 78)
        app.settle(1.0)
        moved = app.update_geometry()
        distance = (moved["X"] - start["X"], moved["Y"] - start["Y"])
        assert abs(distance[0] - 120) < 15 and abs(distance[1] - 60) < 15, f"dragging the bar moved the window by {distance}"
        app.click(16, 18)  # the app's own close button
        end = time.time() + 5
        while app.running() and time.time() < end:
            time.sleep(0.2)
        assert not app.running(), "the close button did not close the app"


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
    test_key_repeat,
    test_relative_file_name,
    test_several_files_as_arguments,
    test_first_and_last_image,
    test_ctrl_and_the_wheel_go_through_the_images,
    test_save_as_writes_the_file,
    test_sequence_of_tiff_pages,
    test_chainload_option_is_accepted,
    test_delete,
    test_lossless_rotation_keys_and_quit,
    test_settings_of_0_9_6,
    test_clipboard,
    test_zen_mode,
    test_paint_mode,
    test_paint_on_transparency,
    test_annotate,
    test_annotation_cards,
    test_annotation_select,
    test_fullscreen,
    test_borderless,
    test_compare_list,
    test_animation_plays_and_stops,
    test_animation_plays_as_often_as_the_file_asks,
    test_keep_view,
    test_keep_edits,
    test_single_frame_gif_is_editable,
    test_saved_edits_come_back,
    test_image_info_is_computed_when_shown,
    test_info_panel_resizes,
    test_info_panel_holds_still,
    test_image_is_fitted_next_to_the_panels,
    test_histogram_shows_values_on_hover,
    test_mipmaps_switched_on_and_off,
    test_actual_size_is_pixel_exact,
    test_overtaken_load,
    test_load_error_shows_toast,
    test_channel_view,
    test_transparent_edges_when_zoomed_out,
]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default="target/debug/oculante")
    parser.add_argument("tests", nargs="*", help="names of tests to run, default all")
    args = parser.parse_args()
    binary = os.path.abspath(args.binary)
    selected = [t for t in TESTS if not args.tests or t.__name__ in args.tests]
    failed = 0
    skipped = 0
    for test in selected:
        try:
            test(binary)
            print(f"ok    {test.__name__}")
        except Skip as reason:
            skipped += 1
            print(f"skip  {test.__name__}: {reason}")
        except (AssertionError, RuntimeError) as error:
            failed += 1
            print(f"FAIL  {test.__name__}: {error}")
    print(f"{len(selected) - failed - skipped} of {len(selected)} passed" + (f", {skipped} skipped" if skipped else ""))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
