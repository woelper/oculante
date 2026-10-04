"""Drive Oculante on a virtual X display.

Starts Xvfb, launches the app with its own empty config, sends keys and mouse
input with xdotool and takes screenshots with ImageMagick. Needs `Xvfb`,
`xdotool`, `import` and `compare` on the PATH.
"""
import os
import re
import shutil
import subprocess
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
IMAGES = os.path.join(REPO, "tests")
OUT = os.path.join(REPO, "target", "ui-tests")
SCREEN = (1400, 900)


def image(name):
    return os.path.join(IMAGES, name)


class App:
    """One running instance of the app on its own display."""

    def __init__(self, binary, name, args, display=":97", stdin=subprocess.DEVNULL):
        self.out = os.path.join(OUT, name)
        shutil.rmtree(self.out, ignore_errors=True)
        os.makedirs(self.out)
        self.xvfb = subprocess.Popen(
            ["Xvfb", display, "-screen", "0", f"{SCREEN[0]}x{SCREEN[1]}x24", "-nolisten", "tcp"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(1.0)
        env = dict(os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.update(DISPLAY=display, RUST_LOG="oculante=debug", RUST_BACKTRACE="1", LIBGL_ALWAYS_SOFTWARE="1")
        # never touch the settings of whoever runs the tests
        for var in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            env[var] = os.path.join(self.out, var.lower())
            os.makedirs(env[var])
        self.env = env
        self.logfile = open(os.path.join(self.out, "app.log"), "w")
        self.app = subprocess.Popen(
            [binary] + args, env=env, stdin=stdin, stdout=self.logfile, stderr=subprocess.STDOUT, cwd=REPO
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
                shell = self.x("xdotool", "getwindowgeometry", "--shell", self.win).stdout
                self.geom = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", shell)}
                return
            time.sleep(0.3)
        raise RuntimeError(f"the window never appeared:\n{self.log()[-800:]}")

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
        """Shortcuts the app reported as triggered, e.g. `InfoMode / "I"`."""
        return [l.split("Matched ")[1].strip() for l in self.log()[since:].splitlines() if "Matched " in l]

    def close(self):
        for proc in (self.app, self.xvfb):
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        self.logfile.close()


def changed_pixels(a, b):
    """Number of pixels that differ between two screenshots."""
    result = subprocess.run(["compare", "-metric", "AE", a, b, "null:"], capture_output=True, text=True)
    return int(float(result.stderr.strip().split()[0]))


def region_has_color(path, box, rgb, tolerance=40):
    """Whether any pixel inside box (x, y, w, h) of a screenshot is close to rgb."""
    x, y, w, h = box
    out = subprocess.run(
        ["convert", path, "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-depth", "8", "txt:-"],
        capture_output=True,
        text=True,
    ).stdout
    for r, g, b in re.findall(r"\((\d+),(\d+),(\d+)", out):
        if all(abs(int(v) - t) <= tolerance for v, t in zip((r, g, b), rgb)):
            return True
    return False
