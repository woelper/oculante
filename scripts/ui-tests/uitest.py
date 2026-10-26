"""Drive Oculante on a virtual X display.

Starts Xvfb, launches the app with its own empty config, sends keys and mouse
input with xdotool and takes screenshots with ImageMagick. Needs `Xvfb`,
`xdotool`, `import` and `compare` on the PATH.
"""
import json
import os
import re
import select
import shutil
import subprocess
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
IMAGES = os.path.join(REPO, "res", "tests")
OUT = os.path.join(REPO, "target", "ui-tests")
SCREEN = (1400, 900)
WINDOW_MANAGER = "openbox"
# far above the displays of a desktop, so a test never takes one of them
FIRST_DISPLAY = 100


def has_window_manager():
    return shutil.which(WINDOW_MANAGER) is not None


def image(name):
    return os.path.join(IMAGES, name)


def start_xvfb(screen, display=None, log=subprocess.DEVNULL):
    """Start Xvfb and wait until it takes connections. Without a display it takes the
    first free one from FIRST_DISPLAY on, so test runs of other checkouts or sessions
    can go on at the same time. Returns the process and the display."""
    numbers = [display.lstrip(":")] if display else range(FIRST_DISPLAY, FIRST_DISPLAY + 200)
    for number in numbers:
        read_end, write_end = os.pipe()
        # Xvfb writes the number here once it is ready, or exits if the display is taken
        proc = subprocess.Popen(
            ["Xvfb", f":{number}", "-displayfd", str(write_end), "-screen", "0", f"{screen[0]}x{screen[1]}x24", "-nolisten", "tcp"],
            pass_fds=(write_end,),
            stdout=log,
            stderr=log,
        )
        os.close(write_end)
        ready, _, _ = select.select([read_end], [], [], 10)
        answer = os.read(read_end, 16) if ready else b""
        os.close(read_end)
        if answer.strip():
            return proc, f":{number}"
        if proc.poll() is None:
            proc.kill()
        proc.wait()
    raise RuntimeError(f"Xvfb did not start on {display or 'any free display'}")


class App:
    """One running instance of the app on its own display."""

    def __init__(
        self,
        binary,
        name,
        args,
        display=None,
        stdin=subprocess.DEVNULL,
        settings=None,
        volatile=None,
        env=None,
        pointer=None,
        window_manager=False,
        cwd=None,
        screen=SCREEN,
    ):
        self.out = os.path.join(OUT, name)
        shutil.rmtree(self.out, ignore_errors=True)
        os.makedirs(self.out)
        if display is not None:
            # a display left behind by a test that died would swallow the keys of this one
            subprocess.run(["pkill", "-f", f"^Xvfb {display} "], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(0.3)
        self.xvfb_log = open(os.path.join(self.out, "xvfb.log"), "w")
        self.xvfb, display = start_xvfb(screen, display, self.xvfb_log)
        self.display = display
        extra_env = env or {}
        env = dict(os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.update(DISPLAY=display, RUST_LOG="oculante=debug", RUST_BACKTRACE="1", LIBGL_ALWAYS_SOFTWARE="1")
        env.update(extra_env)
        # never touch the settings of whoever runs the tests
        for var in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            env[var] = os.path.join(self.out, var.lower())
            os.makedirs(env[var])
        # settings that are not given keep their defaults
        settings_dir = os.path.join(env["XDG_DATA_HOME"], "oculante")
        for file, values in (("config.json", settings), ("config_volatile.json", volatile)):
            if values is not None:
                os.makedirs(settings_dir, exist_ok=True)
                with open(os.path.join(settings_dir, file), "w") as f:
                    json.dump(values, f)
        self.env = env
        self.wm = None
        if window_manager:
            # Without one nothing draws a title bar, moves a window or makes it fullscreen
            self.wm = subprocess.Popen([WINDOW_MANAGER], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(0.8)
        if pointer is not None:
            # place the pointer before the app starts, so no input reaches the app later
            subprocess.run(["xdotool", "mousemove", str(pointer[0]), str(pointer[1])], env=env)
        self.logfile = open(os.path.join(self.out, "app.log"), "w")
        self.started = time.time()
        self.app = subprocess.Popen(
            [binary] + args, env=env, stdin=stdin, stdout=self.logfile, stderr=subprocess.STDOUT, cwd=cwd or REPO
        )
        self.win = None
        self.geom = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def x(self, *cmd):
        return subprocess.run(list(cmd), env=self.env, capture_output=True, text=True)

    def wait_window(self, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            if self.app.poll() is not None:
                raise RuntimeError(f"app exited with {self.app.returncode}:\n{self.log()[-800:]}")
            ids = self.x("xdotool", "search", "--onlyvisible", "--name", "culante").stdout.split()
            if ids:
                self.win = ids[-1]
                self.update_geometry()
                return
            time.sleep(0.3)
        raise RuntimeError(f"the window never appeared:\n{self.log()[-800:]}")

    def update_geometry(self):
        """Read position and size of the window again, after it was moved or resized."""
        shell = self.x("xdotool", "getwindowgeometry", "--shell", self.win).stdout
        self.geom = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", shell)}
        return self.geom

    def frame_extents(self):
        """Width of what the window manager draws around the window: left, right, top, bottom."""
        out = self.x("xprop", "-id", self.win, "_NET_FRAME_EXTENTS").stdout
        return [int(v) for v in re.findall(r"\d+", out.split("=")[1])] if "=" in out else None

    def running(self):
        return self.app.poll() is None

    def settle(self, seconds=0.6):
        """The app only redraws on events, so nudge the pointer and wait."""
        self.x("xdotool", "mousemove_relative", "--", "1", "0")
        self.x("xdotool", "mousemove_relative", "--", "-1", "0")
        time.sleep(seconds)

    def wait_for_log(self, text, timeout=10):
        end = time.time() + timeout
        while time.time() < end:
            if text in self.log():
                return True
            self.settle(0.3)
        return False

    def move(self, x, y):
        """Move the pointer. Values up to 1 are fractions of the window, larger ones pixels."""
        px = self.geom["X"] + (int(x * self.geom["WIDTH"]) if x <= 1 else int(x))
        py = self.geom["Y"] + (int(y * self.geom["HEIGHT"]) if y <= 1 else int(y))
        self.x("xdotool", "mousemove", str(px), str(py))
        time.sleep(0.15)

    def click(self, x, y, button=1):
        self.move(x, y)
        self.x("xdotool", "click", str(button))
        self.settle(0.4)

    def scroll_down(self, x, y, notches):
        self.move(x, y)
        for _ in range(notches):
            self.x("xdotool", "click", "5")
            time.sleep(0.12)
        self.settle(0.8)

    def drag(self, x0, y0, x1, y1, steps=10):
        self.move(x0, y0)
        self.x("xdotool", "mousedown", "1")
        for i in range(1, steps + 1):
            t = i / steps
            self.move(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
        self.x("xdotool", "mouseup", "1")
        self.settle(0.4)

    def key(self, combo, hold=0.2):
        """Press a key or a combination like `shift+Right`."""
        keys = combo.split("+")
        self.x("xdotool", "windowfocus", self.win)
        self.x("xdotool", "keydown", "--delay", "40", *keys)
        time.sleep(hold)
        self.x("xdotool", "keyup", "--delay", "40", *reversed(keys))
        self.settle(0.5)

    def shot(self, label):
        path = os.path.join(self.out, f"{label}.png")
        self.x("import", "-window", "root", path)
        return path

    def log(self):
        self.logfile.flush()
        with open(os.path.join(self.out, "app.log"), errors="replace") as f:
            return f.read()

    def matched_shortcuts(self, since=0):
        """Names of the shortcuts the app reported as triggered, e.g. `InfoMode`."""
        marker = "Matched shortcut "
        return [l.split(marker)[1].strip() for l in self.log()[since:].splitlines() if marker in l]

    def close(self):
        for proc in (self.app, self.wm, self.xvfb):
            if proc is not None and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        self.logfile.close()
        self.xvfb_log.close()


def changed_pixels(a, b, fuzz=0):
    """Number of pixels that differ between two screenshots, by more than fuzz percent."""
    result = subprocess.run(
        ["compare", "-fuzz", f"{fuzz}%", "-metric", "AE", a, b, "null:"], capture_output=True, text=True
    )
    return int(float(result.stderr.strip().split()[0]))


def changed_pixels_in(a, b, box, fuzz=0):
    """Number of pixels that differ inside box (x, y, w, h) of two screenshots."""
    x, y, w, h = box
    crops = []
    for path in (a, b):
        crop = path[:-4] + "_crop.png"
        subprocess.run(["convert", path, "-crop", f"{w}x{h}+{x}+{y}", "+repage", crop], check=True)
        crops.append(crop)
    return changed_pixels(*crops, fuzz=fuzz)


def _pixels(path, box):
    x, y, w, h = box
    out = subprocess.run(
        ["convert", path, "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-depth", "8", "txt:-"],
        capture_output=True,
        text=True,
    ).stdout
    for px, py, r, g, b in re.findall(r"^(\d+),(\d+): \((\d+),(\d+),(\d+)", out, re.M):
        yield x + int(px), y + int(py), (int(r), int(g), int(b))


def _close(color, rgb, tolerance):
    return all(abs(v - t) <= tolerance for v, t in zip(color, rgb))


def region_has_color(path, box, rgb, tolerance=40):
    """Whether any pixel inside box (x, y, w, h) of a screenshot is close to rgb."""
    return any(_close(color, rgb, tolerance) for _, _, color in _pixels(path, box))


def find_slider(path, box, rgb, tolerance=40, min_fill=40):
    """The filled part of a slider inside box: (right end, y), or None. A row counts
    if at least min_fill pixels in it are close to rgb, which leaves out icons."""
    rows = {}
    for x, y, color in _pixels(path, box):
        if _close(color, rgb, tolerance):
            rows.setdefault(y, []).append(x)
    filled = sorted(y for y, xs in rows.items() if len(xs) >= min_fill)
    if not filled:
        return None
    middle = filled[len(filled) // 2]
    return max(rows[middle]), middle


def mean_level(path, box):
    """The mean of all channels inside box (x, y, w, h) of a screenshot, 0 to 255."""
    x, y, w, h = box
    out = subprocess.run(
        ["convert", path, "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-format", "%[fx:mean*255]", "info:"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return float(out)


def count_color(path, box, rgb, tolerance=40):
    """Number of pixels inside box (x, y, w, h) of a screenshot close to rgb."""
    return sum(1 for _, _, color in _pixels(path, box) if _close(color, rgb, tolerance))


def color_center(path, box, rgb, tolerance=40):
    """The middle (x, y) of the pixels inside box (x, y, w, h) close to rgb, or None."""
    found = [(x, y) for x, y, color in _pixels(path, box) if _close(color, rgb, tolerance)]
    if not found:
        return None
    return (sum(x for x, _ in found) // len(found), sum(y for _, y in found) // len(found))


def panel_edge(path, x_range=(100, 900), rows=(80, 160, 240, 320, 400, 480, 560)):
    """The x of the right edge of a panel on the left: its separator line, the leftmost
    column with the same grey in all the rows, (60, 60, 60), or near white while the
    pointer is on it. Right of an image the empty canvas has one colour in all rows too."""
    colors = {}
    for y in rows:
        for x, _, color in _pixels(path, (x_range[0], y, x_range[1] - x_range[0], 1)):
            colors.setdefault(x, set()).add(color)
    for x in sorted(colors):
        if len(colors[x]) == 1:
            r, g, b = next(iter(colors[x]))
            if r == g == b and (abs(r - 60) <= 4 or r >= 180):
                return x
    return None


def rightmost_x(path, box, rgb, tolerance=40):
    """The largest x inside box (x, y, w, h) with a pixel close to rgb, or None."""
    xs = [x for x, _, color in _pixels(path, box) if _close(color, rgb, tolerance)]
    return max(xs) if xs else None
