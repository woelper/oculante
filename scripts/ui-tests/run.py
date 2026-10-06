#!/usr/bin/env python3
"""UI tests that drive a real Oculante window on a virtual display.

    python3 scripts/ui-tests/run.py [--binary target/debug/oculante] [test ...]

Each test checks behaviour that broke before. See README.md.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from uitest import (  # noqa: E402
    OUT,
    SCREEN,
    WINDOW_MANAGER,
    App,
    changed_pixels,
    changed_pixels_in,
    find_slider,
    has_window_manager,
    image,
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
        panel = (0, 40, 190, 540)
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
    """In paint mode a drag over the image leaves a stroke."""
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


def start(binary, name, path, settings=None):
    """The app with an image on screen. Closes it if that fails, so nothing is left behind."""
    app = App(binary, name, [path], settings=settings)
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
    app.click(60, 555)  # open "Compare"
    app.settle(0.8)
    app.scroll_down(120, 300, 12)
    app.click(139, 329)  # "Add current image"
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
        for n in range(4):
            frames.append(app.shot(f"frame_{n}"))
            time.sleep(0.25)
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
        assert region_has_color(end, TOAST_AREA, ACCENT, 30), "no message that the animation ended"
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


def test_saved_edits_come_back(binary):
    """Edits saved with "Save edits" are applied again when the image is opened later."""
    folder = folder_of("saved_edits_images", [("moss.jpg", "moss.jpg")])
    path = os.path.join(folder, "moss.jpg")
    # the image, left of the edit panel, whose width can change a little
    area = (130, 50, 560, 450)
    with start(binary, "saved_edits", path) as app:
        plain = app.shot("plain")
        brighten(app)
        edited = app.shot("edited")
        app.click(890, 486)  # "Save edits"
        app.settle(1.0)
    assert os.path.isfile(os.path.join(folder, "moss.oculante")), "no file with the edits was written"
    with start(binary, "saved_edits_again", path) as app:
        app.move(0.4, 0.5)
        app.settle(1.5)
        again = app.shot("again")
    assert changed_pixels_in(plain, again, area) > 50_000, "the saved edits were not applied"
    changed = changed_pixels_in(edited, again, area)
    assert changed < 500, f"the image looks different from when the edits were saved ({changed} pixels)"


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
            # zoom to 100% around the middle of the window, where the image is centred
            app.move(513, 300)
            app.key("1")
            app.x("xdotool", "mousemove", "1020", "595")
            app.settle(1.0)
            shot = app.shot("actual_size")
        off = differing_pixels_near(shot, path, 513 - width // 2, 300 - height // 2, width, height)
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
        assert region_has_color(shot, TOAST_AREA, ACCENT, 30), "no error toast is shown"
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


def needs_window_manager():
    if not has_window_manager():
        raise Skip(f"needs the window manager {WINDOW_MANAGER}")


def test_fullscreen(binary):
    """The window fills the screen in fullscreen and returns to where it was."""
    needs_window_manager()
    with App(binary, "fullscreen", [image("moss.jpg")], window_manager=True) as app:
        app.wait_window()
        app.move(0.6, 0.6)
        assert app.wait_for_log(LOADED), "the image never loaded"
        app.settle(1.0)
        start = dict(app.geom)
        app.key("f")
        app.settle(1.5)
        full = app.update_geometry()
        assert (full["X"], full["Y"], full["WIDTH"], full["HEIGHT"]) == (0, 0, *SCREEN), f"not fullscreen: {full}"
        shot = app.shot("fullscreen")
        assert not region_has_color(shot, (0, 0, 40, 40), (0, 0, 0), 4), "the window does not cover the screen"
        app.key("f")
        app.settle(1.5)
        assert app.update_geometry() == start, f"the window came back as {app.geom}, it was {start}"


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
    test_zen_mode,
    test_paint_mode,
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
    test_actual_size_is_pixel_exact,
    test_overtaken_load,
    test_load_error_shows_toast,
    test_channel_view,
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
