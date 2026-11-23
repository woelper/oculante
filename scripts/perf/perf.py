#!/usr/bin/env python3
"""Performance measurements on a real Oculante window on a virtual display.

    python3 scripts/perf/perf.py run --binary target/release/oculante --label next
    python3 scripts/perf/perf.py run --binary target/release/oculante --label next-gpu --vgl egl0
    python3 scripts/perf/perf.py compare master.json next.json

`run` starts the app many times, measures how long things take until they are
visible on screen, how much memory and CPU the process uses, and writes the
numbers to a JSON file. `compare` puts two of those files next to each other.
See README.md.
"""
import argparse
import ctypes
import datetime
import glob
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "ui-tests"))
from uitest import App, REPO, changed_pixels  # noqa: E402

OUT = os.path.join(REPO, "target", "perf")
IMAGES = os.path.join(OUT, "images")
DISPLAY = ":98"
VGLRUN = shutil.which("vglrun") or "/opt/VirtualGL/bin/vglrun"
# changing the images makes old results incomparable
IMAGE_SET = 1

# Every image is noise with a patch of one color in the middle. Noise keeps the
# decoders busy, the patch is what the measurements look for on screen.
PATCH = (200, 40, 160)
PATCH_COLORS = [(200, 40, 160), (40, 180, 60), (220, 150, 30), (40, 90, 220), (210, 210, 210), (150, 30, 30)]

# name, width, height, kind
OPEN_IMAGES = [
    ("rgb8_2mp.jpg", 1920, 1280, "rgb"),
    ("rgb8_24mp.jpg", 6000, 4000, "rgb"),
    ("rgba8_24mp.png", 6000, 4000, "rgba"),
    ("gray8_64mp.png", 8000, 8000, "gray8"),
    ("gray16_16mp.png", 4000, 4000, "gray16"),
    ("float_12mp.hdr", 4000, 3000, "rgb"),
    # wider than the largest texture of most GPUs
    ("rgb8_144mp.jpg", 18000, 8000, "rgb"),
]
SWITCH_COUNT = 12
ANIMATION = "anim_20fps.gif"
ANIMATION_FPS = 20

# metrics where a larger number is the better one
HIGHER_IS_BETTER = ("fps",)


# ---------------------------------------------------------------- images


def _convert(*args):
    subprocess.run(["convert", *map(str, args)], check=True)


def _noise():
    path = os.path.join(IMAGES, "noise.png")
    if not os.path.exists(path):
        _convert("-seed", 7, "-size", "1024x1024", "plasma:fractal", path)
    return path


def make_image(path, width, height, kind="rgb", color=PATCH):
    """Noise of the given size with a patch covering the middle 40 percent."""
    if os.path.exists(path):
        return path
    box = f"rectangle {int(width * 0.3)},{int(height * 0.3)} {int(width * 0.7)},{int(height * 0.7)}"
    cmd = ["-size", f"{width}x{height}", "tile:" + _noise()]
    if kind in ("gray8", "gray16"):
        cmd += ["-colorspace", "Gray", "-fill", "gray(78%)", "-draw", box]
        cmd += ["-define", "png:color-type=0", "-depth", "8" if kind == "gray8" else "16"]
    else:
        cmd += ["-fill", "rgb(%d,%d,%d)" % color, "-draw", box]
    target = path
    if kind == "rgba":
        target = "png32:" + path
    if path.endswith(".png"):
        cmd += ["-define", "png:compression-level=3"]
    if path.endswith(".jpg"):
        cmd += ["-quality", "90"]
    _convert(*cmd, target)
    return path


def make_images():
    os.makedirs(IMAGES, exist_ok=True)
    for name, width, height, kind in OPEN_IMAGES:
        make_image(os.path.join(IMAGES, name), width, height, kind)
    folder = os.path.join(IMAGES, "switch")
    os.makedirs(folder, exist_ok=True)
    for i in range(SWITCH_COUNT):
        color = PATCH_COLORS[i % len(PATCH_COLORS)]
        make_image(os.path.join(folder, f"photo_{i:02}.jpg"), 4000, 3000, "rgb", color)
    anim = os.path.join(IMAGES, ANIMATION)
    if not os.path.exists(anim):
        frames = []
        for i in range(2):
            frame = os.path.join(IMAGES, f"anim_frame_{i}.png")
            make_image(frame, 800, 600, "rgb", PATCH_COLORS[i])
            frames.append(frame)
        _convert("-delay", 100 // ANIMATION_FPS, "-loop", 0, *(frames * 10), anim)


# ---------------------------------------------------------------- screen and input


class Screen:
    """Reads pixels from the virtual display and sends input, without starting a process each time."""

    def __init__(self, display):
        x = ctypes.CDLL("libX11.so.6")
        xtst = ctypes.CDLL("libXtst.so.6")
        x.XOpenDisplay.restype = ctypes.c_void_p
        x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x.XDefaultRootWindow.restype = ctypes.c_ulong
        x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        x.XGetImage.restype = ctypes.c_void_p
        x.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong] + [ctypes.c_int] * 2 + [ctypes.c_uint] * 2
        x.XGetImage.argtypes += [ctypes.c_ulong, ctypes.c_int]
        x.XGetPixel.restype = ctypes.c_ulong
        x.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        x.XDestroyImage.argtypes = [ctypes.c_void_p]
        x.XStringToKeysym.restype = ctypes.c_ulong
        x.XStringToKeysym.argtypes = [ctypes.c_char_p]
        x.XKeysymToKeycode.restype = ctypes.c_ubyte
        x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p] + [ctypes.c_int] * 3 + [ctypes.c_ulong]
        xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.x, self.xtst = x, xtst
        self.dpy = x.XOpenDisplay(display.encode())
        assert self.dpy, f"cannot open display {display}"
        self.root = x.XDefaultRootWindow(self.dpy)

    def sample(self, cx, cy, spread=25):
        """Colors at (cx, cy) and at four points around it, the center first."""
        size = 2 * spread + 1
        img = self.x.XGetImage(self.dpy, self.root, cx - spread, cy - spread, size, size, 0xFFFFFFFF, 2)
        points = [(spread, spread), (0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)]
        colors = []
        for px, py in points:
            v = self.x.XGetPixel(img, px, py)
            colors.append(((v >> 16) & 255, (v >> 8) & 255, v & 255))
        self.x.XDestroyImage(img)
        return colors

    def _key_event(self, name, down):
        code = self.x.XKeysymToKeycode(self.dpy, self.x.XStringToKeysym(name.encode()))
        assert code, f"no key named {name}"
        self.xtst.XTestFakeKeyEvent(self.dpy, code, 1 if down else 0, 0)
        self.x.XSync(self.dpy, 0)

    def key(self, name):
        """Press and release a key."""
        self._key_event(name, True)
        time.sleep(0.02)
        self._key_event(name, False)

    def timed_key(self, name, condition, timeout=30):
        """Press a key and return the seconds until condition() is true, or None after the timeout.

        The key is released after 20 ms, as a person would, while the screen is watched already.
        """
        start = time.time()
        self._key_event(name, True)
        held = True
        try:
            while time.time() - start < timeout:
                if condition():
                    return time.time() - start
                if held and time.time() - start > 0.02:
                    self._key_event(name, False)
                    held = False
                time.sleep(0.001)
            return None
        finally:
            if held:
                self._key_event(name, False)

    def move(self, x, y):
        self.xtst.XTestFakeMotionEvent(self.dpy, -1, int(x), int(y), 0)
        self.x.XSync(self.dpy, 0)

    def button(self, button, down):
        self.xtst.XTestFakeButtonEvent(self.dpy, button, 1 if down else 0, 0)
        self.x.XSync(self.dpy, 0)


def differs(a, b, tolerance=24):
    return any(abs(p - q) > tolerance for p, q in zip(a, b))


def uniform(colors, tolerance=12):
    return all(not differs(colors[0], c, tolerance) for c in colors[1:])


def wait_until(condition, timeout, interval=0.002):
    """Seconds since the call until condition() is true, or None after the timeout."""
    start = time.time()
    while time.time() - start < timeout:
        if condition():
            return time.time() - start
        time.sleep(interval)
    return None


# ---------------------------------------------------------------- process numbers


def cpu_seconds(pid):
    """CPU time used by all threads of the process so far."""
    with open(f"/proc/{pid}/stat") as f:
        fields = f.read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def wakeups(pid):
    """How often the threads of the process went to sleep and were woken again."""
    total = 0
    for path in glob.glob(f"/proc/{pid}/task/*/status"):
        try:
            with open(path) as f:
                for line in f:
                    if line.startswith("voluntary_ctxt_switches"):
                        total += int(line.split()[1])
        except FileNotFoundError:
            pass
    return total


def gpu(pid):
    """What the kernel knows about the process on the GPU: memory in MB and the seconds
    the GPU worked for it. None if it does not render on one."""
    clients = {}
    for path in glob.glob(f"/proc/{pid}/fdinfo/*"):
        try:
            with open(path) as f:
                text = f.read()
        except OSError:
            continue
        if "drm-client-id" not in text:
            continue
        fields = dict(line.split(":", 1) for line in text.splitlines() if line.startswith("drm-"))
        clients[fields["drm-client-id"].strip()] = fields

    def total(prefix, unit):
        return sum(
            int(value.split()[0])
            for fields in clients.values()
            for key, value in fields.items()
            if key.startswith(prefix) and value.strip().endswith(unit)
        )

    if not clients:
        return None
    return {
        "mb": total("drm-resident-", "KiB") / 1024,
        # buffers the driver keeps around and gives up when memory gets tight
        "purgeable_mb": total("drm-purgeable-", "KiB") / 1024,
        "seconds": total("drm-engine-", "ns") / 1e9,
    }


def gpu_seconds(pid):
    usage = gpu(pid)
    return usage["seconds"] if usage else None


def memory(pid):
    """Resident, proportional and peak memory in MB, and the memory on the GPU if there is one."""
    numbers = {}
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith(("VmRSS", "VmHWM")):
                numbers[line.split(":")[0]] = int(line.split()[1]) / 1024
    with open(f"/proc/{pid}/smaps_rollup") as f:
        for line in f:
            if line.startswith("Pss:"):
                numbers["Pss"] = int(line.split()[1]) / 1024
    result = {"rss_mb": numbers["VmRSS"], "pss_mb": numbers["Pss"], "peak_rss_mb": numbers["VmHWM"]}
    usage = gpu(pid)
    if usage:
        result["gpu_mb"] = usage["mb"]
        result["gpu_purgeable_mb"] = usage["purgeable_mb"]
    return result


def start_app(ctx, name, args, pointer=None):
    """Start the app on the virtual display, rendering in software or on a GPU."""
    binary, env = ctx["binary"], {"RUST_LOG": "error"}
    if ctx.get("vgl"):
        # VirtualGL renders on the GPU and copies the frames to the virtual display
        args = ["-d", ctx["vgl"], binary] + args
        binary = VGLRUN
        env["LIBGL_ALWAYS_SOFTWARE"] = "0"
    return App(binary, name, args, display=DISPLAY, env=env, pointer=pointer)


# ---------------------------------------------------------------- scenarios


class Session:
    """One start of the app, with what the scenarios need to look at it."""

    def __init__(self, ctx, name, args):
        self.ctx = ctx
        self.cx, self.cy = ctx["center"]
        # the pointer is over the image, but away from where pixels are read
        self.app = start_app(ctx, "perf_" + name, args, pointer=(self.cx + 60, self.cy + 60))
        # the driver writes into target/ui-tests, which is fine for logs
        self.screen = Screen(DISPLAY)
        self.pid = self.app.app.pid

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.app.close()

    def patch(self):
        """The color in the middle if a patch is shown there, else None."""
        colors = self.screen.sample(self.cx, self.cy)
        # black is the empty screen before the window is there
        if colors[0] != (0, 0, 0) and uniform(colors) and differs(colors[0], self.ctx["background"]):
            return colors[0]
        return None

    def wait_window(self, timeout=60):
        """Seconds from the start of the process until the window is painted."""
        if wait_until(lambda: self.screen.sample(self.cx, self.cy)[0] != (0, 0, 0), timeout) is None:
            raise RuntimeError("the window never appeared:\n" + self.app.log()[-600:])
        return time.time() - self.app.started

    def wait_image(self, timeout=120):
        """Seconds from the start of the process until the image is on screen."""
        if wait_until(lambda: self.patch() is not None, timeout) is None:
            raise RuntimeError("the image never appeared:\n" + self.app.log()[-600:])
        return time.time() - self.app.started

    def changed(self, before):
        """Whether the middle of the window shows a patch other than `before`."""
        now = self.patch()
        return now is not None and (before is None or differs(now, before))

    def idle(self, seconds):
        """CPU use in percent of one core and wakeups per second while nothing happens."""
        cpu, wake, start = cpu_seconds(self.pid), wakeups(self.pid), time.time()
        busy = gpu_seconds(self.pid)
        time.sleep(seconds)
        elapsed = time.time() - start
        result = {
            "idle_cpu_percent": 100 * (cpu_seconds(self.pid) - cpu) / elapsed,
            "idle_wakeups_per_s": (wakeups(self.pid) - wake) / elapsed,
        }
        if busy is not None:
            result["idle_gpu_percent"] = 100 * (gpu_seconds(self.pid) - busy) / elapsed
        return result

    def focus(self):
        self.app.wait_window()
        self.app.x("xdotool", "windowfocus", self.app.win)
        time.sleep(0.3)


def scenario_empty(ctx):
    """Start without an image."""
    with Session(ctx, "empty", []) as s:
        result = {"time_to_window_s": s.wait_window()}
        time.sleep(ctx["settle"])
        result.update(memory(s.pid))
        result.update(s.idle(ctx["idle"]))
        return result


def scenario_open(ctx, name):
    """Start with an image: time until it shows, memory once loaded, CPU while idle."""
    with Session(ctx, "open", [os.path.join(IMAGES, name)]) as s:
        result = {"time_to_window_s": s.wait_window(), "time_to_image_s": s.wait_image()}
        time.sleep(ctx["settle"])
        result.update(memory(s.pid))
        result.update(s.idle(ctx["idle"]))
        return result


def scenario_panels(ctx):
    """CPU while idle and memory with the info and the edit panel open."""
    result = {}
    with Session(ctx, "panels", [os.path.join(IMAGES, "rgb8_24mp.jpg")]) as s:
        s.wait_image()
        s.focus()
        for key, panel in (("i", "info"), ("e", "edit")):
            closed = s.app.shot("closed")
            s.screen.key(key)
            time.sleep(ctx["settle"])
            if changed_pixels(closed, s.app.shot(panel)) < 20000:
                raise RuntimeError(f"the {panel} panel did not open")
            result[f"{panel}_rss_mb"] = memory(s.pid)["rss_mb"]
            for metric, value in s.idle(ctx["idle"]).items():
                result[f"{panel}_{metric}"] = value
            s.screen.key(key)
            time.sleep(1)
    return result


def scenario_animation(ctx):
    """CPU while an animation plays, and how many of its frames reach the screen."""
    with Session(ctx, "animation", [os.path.join(IMAGES, ANIMATION)]) as s:
        s.wait_image()
        time.sleep(2)
        cpu, busy, start = cpu_seconds(s.pid), gpu_seconds(s.pid), time.time()
        last, changes = s.patch(), 0
        while time.time() - start < ctx["idle"]:
            now = s.patch()
            if now is not None and (last is None or differs(now, last)):
                changes += 1
                last = now
            time.sleep(0.001)
        elapsed = time.time() - start
        result = {"cpu_percent": 100 * (cpu_seconds(s.pid) - cpu) / elapsed, "shown_fps": changes / elapsed}
        if busy is not None:
            result["gpu_percent"] = 100 * (gpu_seconds(s.pid) - busy) / elapsed
        result.update(memory(s.pid))
        return result


def scenario_switch(ctx):
    """Time from the key press until the next image of a folder is on screen."""
    folder = os.path.join(IMAGES, "switch")
    with Session(ctx, "switch", [os.path.join(folder, "photo_00.jpg")]) as s:
        s.wait_image()
        s.focus()
        time.sleep(ctx["settle"])
        passes = {}
        for label, key in (("next", "Right"), ("back", "Left")):
            times = []
            for _ in range(SWITCH_COUNT - 1):
                before = s.patch()
                elapsed = s.screen.timed_key(key, lambda: s.changed(before))
                if elapsed is None:
                    raise RuntimeError(f"no new image after {key}")
                times.append(elapsed)
                time.sleep(0.5)
            passes[label] = times
        result = {
            "next_median_s": statistics.median(passes["next"]),
            "next_max_s": max(passes["next"]),
            # the way back shows images that were loaded before
            "back_median_s": statistics.median(passes["back"]),
        }
        result.update(memory(s.pid))
        return result


def scenario_channels(ctx):
    """Time from the key press until another color channel is shown."""
    with Session(ctx, "channels", [os.path.join(IMAGES, "rgba8_24mp.png")]) as s:
        s.wait_image()
        s.focus()
        time.sleep(ctx["settle"])
        to_channel, to_color = [], []
        for key in ("r", "g", "b"):
            for press, times in ((key, to_channel), ("c", to_color)):
                # a single channel can be as dark as the background, so only look for a change
                before = s.screen.sample(s.cx, s.cy)[0]
                elapsed = s.screen.timed_key(press, lambda: differs(before, s.screen.sample(s.cx, s.cy)[0]))
                if elapsed is None:
                    raise RuntimeError(f"the image did not change after {press}")
                times.append(elapsed)
                time.sleep(0.8)
        return {"to_channel_median_s": statistics.median(to_channel), "to_color_median_s": statistics.median(to_color)}


def scenario_view(ctx, name):
    """Panning and zooming: time until a pan is on screen, and the CPU a fixed set of moves costs."""
    with Session(ctx, "view", [os.path.join(IMAGES, name)]) as s:
        s.wait_image()
        s.focus()
        time.sleep(ctx["settle"])
        cx, cy, screen = s.cx, s.cy, s.screen

        # Drag the patch out of the middle and back. The first small move starts
        # the drag, the large one is the one that is timed.
        latencies = []
        x = cx + 60
        for i in range(10):
            direction = 1 if i % 2 == 0 else -1
            screen.move(x, cy + 60)
            time.sleep(0.2)
            screen.button(1, True)
            time.sleep(0.1)
            x += 10 * direction
            screen.move(x, cy + 60)
            time.sleep(0.3)
            before = screen.sample(cx, cy)
            x += 320 * direction
            moved = time.time()
            screen.move(x, cy + 60)
            seen = wait_until(lambda: any(differs(a, b) for a, b in zip(before, screen.sample(cx, cy))), 10)
            # counted from before the move was sent
            elapsed = None if seen is None else time.time() - moved
            screen.button(1, False)
            if elapsed is None:
                raise RuntimeError("dragging did not move the image")
            latencies.append(elapsed)
            time.sleep(0.3)

        # 120 moves in a bit more than a second
        screen.move(cx + 60, cy + 60)
        time.sleep(0.5)
        cpu, busy = cpu_seconds(s.pid), gpu_seconds(s.pid)
        screen.button(1, True)
        for i in range(120):
            offset = 200 * (1 - abs((i % 40) - 20) / 10)
            screen.move(cx + 60 + offset, cy + 60 + offset / 2)
            time.sleep(0.01)
        screen.button(1, False)
        time.sleep(1.0)
        pan_cpu = cpu_seconds(s.pid) - cpu
        pan_gpu = gpu_seconds(s.pid) - busy if busy is not None else None

        # 15 notches in and out again
        cpu, busy = cpu_seconds(s.pid), gpu_seconds(s.pid)
        for button in (4, 5):
            for _ in range(15):
                screen.button(button, True)
                screen.button(button, False)
                time.sleep(0.04)
        time.sleep(1.0)
        zoom_cpu = cpu_seconds(s.pid) - cpu

        result = {
            "pan_latency_median_s": statistics.median(latencies),
            "pan_latency_max_s": max(latencies),
            "pan_cpu_s": pan_cpu,
            "zoom_cpu_s": zoom_cpu,
        }
        if busy is not None:
            result["pan_gpu_s"] = pan_gpu
            result["zoom_gpu_s"] = gpu_seconds(s.pid) - busy
        return result


def probe(ctx):
    """Where the middle of the window is and what its background looks like."""
    with start_app(ctx, "perf_probe", []) as app:
        app.wait_window()
        time.sleep(2)
        geom = app.geom
        center = (geom["X"] + geom["WIDTH"] // 2, geom["Y"] + geom["HEIGHT"] // 2)
        colors = Screen(DISPLAY).sample(*center)
        background = max(set(colors), key=colors.count)
        assert background != (0, 0, 0), "cannot tell the window from the empty screen"
        return center, background


def scenarios(only):
    """Name and function of every measurement, in the order they run."""
    todo = [("empty", scenario_empty)]
    todo += [("open." + name, lambda ctx, n=name: scenario_open(ctx, n)) for name, *_ in OPEN_IMAGES]
    todo += [
        ("panels", scenario_panels),
        ("animation", scenario_animation),
        ("switch", scenario_switch),
        ("channels", scenario_channels),
        ("view.rgb8_24mp.jpg", lambda ctx: scenario_view(ctx, "rgb8_24mp.jpg")),
        ("view.rgb8_144mp.jpg", lambda ctx: scenario_view(ctx, "rgb8_144mp.jpg")),
    ]
    if only:
        todo = [(name, fn) for name, fn in todo if any(name.startswith(o) for o in only)]
    return todo


# ---------------------------------------------------------------- commands


def cpu_model():
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor()


def rendering(vgl):
    """What draws the frames, for the result file."""
    if not vgl:
        return "software (llvmpipe), textures are part of the process memory"
    eglinfo = os.path.join(os.path.dirname(os.path.realpath(VGLRUN)), "eglinfo")
    info = subprocess.run([eglinfo, vgl], capture_output=True, text=True)
    for line in info.stdout.splitlines():
        if line.startswith("OpenGL renderer string:"):
            return line.split(":", 1)[1].strip() + ", through VirtualGL"
    raise SystemExit(f"VirtualGL cannot use the device {vgl}:\n{info.stdout}{info.stderr}")


def run(args):
    binary = os.path.abspath(args.binary)
    make_images()
    rendered_by = rendering(args.vgl)
    ctx = {"binary": binary, "vgl": args.vgl, "idle": args.idle, "settle": args.settle}
    ctx["center"], ctx["background"] = probe(ctx)
    metrics, failures = {}, []
    for name, fn in scenarios(args.only):
        values = {}
        for attempt in range(args.runs):
            try:
                result = fn(ctx)
            except Exception as e:  # keep going, one broken scenario should not cost the others
                failures.append(f"{name}: {e}".splitlines()[0])
                print(f"FAIL  {name}: {e}", flush=True)
                break
            for metric, value in result.items():
                values.setdefault(metric, []).append(value)
        for metric, numbers in values.items():
            metrics[f"{name}.{metric}"] = {"median": statistics.median(numbers), "values": numbers}
            print(f"{name + '.' + metric:<52} {fmt(statistics.median(numbers)):>10}", flush=True)
    report = {
        "meta": {
            "label": args.label,
            "date": datetime.datetime.now().isoformat(timespec="seconds"),
            "binary_size_mb": os.path.getsize(binary) / 1e6,
            "runs": args.runs,
            "idle_seconds": args.idle,
            "image_set": IMAGE_SET,
            "cpu": cpu_model(),
            "cores": os.cpu_count(),
            "rendering": rendered_by,
            "failures": failures,
        },
        "metrics": metrics,
    }
    out = args.out or os.path.join(OUT, f"{args.label}.json")
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    with open(out, "w") as f:
        json.dump(report, f, indent=1)
    print(f"written to {out}")
    return 1 if failures else 0


def fmt(value):
    if value is None:
        return "-"
    return f"{value:.3f}" if abs(value) < 10 else f"{value:.1f}"


def compare(args):
    with open(args.base) as f:
        base = json.load(f)
    with open(args.new) as f:
        new = json.load(f)
    a, b = base["meta"]["label"], new["meta"]["label"]
    if base["meta"].get("image_set") != new["meta"].get("image_set"):
        print("warning: the two files were measured with different images")
    if base["meta"].get("cpu") != new["meta"].get("cpu"):
        print("warning: the two files were measured on different machines")
    if base["meta"].get("rendering") != new["meta"].get("rendering"):
        print("warning: one of the two files was rendered in software, the other on a GPU")
    rows = [("binary_size_mb", base["meta"]["binary_size_mb"], new["meta"]["binary_size_mb"])]
    names = list(base["metrics"]) + [n for n in new["metrics"] if n not in base["metrics"]]
    for name in names:
        rows.append((name, base["metrics"].get(name, {}).get("median"), new["metrics"].get(name, {}).get("median")))
    width = max(len(r[0]) for r in rows)
    print(f"{'':<{width}} {a:>10} {b:>10} {'change':>8}")
    for name, old, now in rows:
        change, mark = "", ""
        if old is not None and now is not None:
            delta = now - old
            # small numbers jump around, percentages of them mean nothing
            if abs(old) > 1e-9 and abs(delta) > args.noise * max(abs(old), abs(now)):
                change = f"{100 * delta / old:+.0f}%"
                better = (delta > 0) == name.endswith(HIGHER_IS_BETTER)
                mark = " better" if better else " worse"
        print(f"{name:<{width}} {fmt(old):>10} {fmt(now):>10} {change:>8}{mark}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("run", help="measure a binary")
    r.add_argument("--binary", default=os.path.join(REPO, "target", "release", "oculante"))
    r.add_argument("--label", required=True, help="name of this measurement, for example a branch or commit")
    r.add_argument("--out", help="result file, default target/perf/<label>.json")
    r.add_argument("--runs", type=int, default=3, help="how often every scenario is repeated")
    r.add_argument("--idle", type=float, default=10, help="seconds to watch the idle process")
    r.add_argument("--settle", type=float, default=3, help="seconds to wait before memory and idle are measured")
    r.add_argument("--vgl", metavar="DEVICE", help="render on a GPU through VirtualGL, for example egl0")
    r.add_argument("only", nargs="*", help="only scenarios starting with these names")
    r.set_defaults(fn=run)
    c = sub.add_parser("compare", help="compare two result files")
    c.add_argument("base")
    c.add_argument("new")
    c.add_argument("--noise", type=float, default=0.1, help="ignore changes smaller than this fraction")
    c.set_defaults(fn=compare)
    args = parser.parse_args()
    sys.exit(args.fn(args))


if __name__ == "__main__":
    main()
