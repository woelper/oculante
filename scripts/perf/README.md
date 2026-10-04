# Performance measurements

`perf.py` starts a real Oculante window on a virtual display, many times, and
measures what a user would notice: how long it takes until an image is on
screen, how long switching images, panning and changing channels take, how
much memory the process holds and how much CPU it uses while nothing happens.

The numbers go into a JSON file, so two builds can be compared and results can
be kept to see how things develop.

## Requirements

Linux with `Xvfb`, `xdotool` and ImageMagick, as for the UI tests:

    sudo apt install xvfb xdotool imagemagick

## Running

Measure a release build. Debug builds are several times slower and say little.

    cargo build --release
    python3 scripts/perf/perf.py run --label my-change

The result is written to `target/perf/my-change.json`. A full run takes about
20 minutes. Do not use the machine for anything else meanwhile, a build
running next to it changes the timings.

Measure another binary, for example a build of the master branch, and compare:

    python3 scripts/perf/perf.py run --label master --binary ../master/target/release/oculante
    python3 scripts/perf/perf.py compare target/perf/master.json target/perf/my-change.json

Only some scenarios, fewer repetitions:

    python3 scripts/perf/perf.py run --label quick --runs 1 open.rgb8_24mp switch

The test images are generated into `target/perf/images` on the first run. They
are noise with a patch of one color in the middle. The noise gives the decoders
real work, the patch is what the script looks for on screen.

## What is measured

Every scenario starts the app anew. Each one is repeated (`--runs`, default 3)
and the median is reported.

| Scenario | Metrics |
|---|---|
| `empty` | Start without an image: time until the window is painted, memory, idle CPU |
| `open.<image>` | Start with an image: time until the window is painted and until the image is on screen, memory once loaded, peak memory, idle CPU |
| `panels` | Memory and idle CPU with the info panel and with the edit panel open |
| `animation` | CPU while an animation plays, and how many of its 20 frames per second reach the screen |
| `switch` | A folder of 12 photos: time from the key press until the next image is on screen, and the same on the way back, when the images were loaded before |
| `channels` | Time from the key press until a single color channel is shown, and back |
| `view.<image>` | Time from a mouse move until the panned image is on screen, CPU used for 120 pan moves and for 30 zoom steps |

The images cover what takes different paths in the app: a small and a 24 MP
JPEG, a PNG with alpha, 8 and 16 bit grayscale, a float image and a JPEG that
is wider than the largest texture of most GPUs.

Units are in the metric names: `_s` seconds, `_mb` megabytes, `_percent`
percent of one CPU core.

- `rss_mb` is the memory the process holds, `pss_mb` the same with shared
  libraries counted by their share, `peak_rss_mb` the most it ever held, which
  shows what loading costs on top.
- `idle_cpu_percent` and `idle_wakeups_per_s` are taken over 10 seconds without
  any input. Both should be zero. If they are not, something redraws or polls
  all the time.

## How to read the numbers

- Everything is rendered in software (llvmpipe). Textures are part of the
  process memory, so memory includes what would be GPU memory on a desktop.
  That makes texture formats and duplicated image data visible.
- There is no display refresh to wait for. Whatever redraws continuously does
  so as fast as it can, on all cores. `animation.cpu_percent`, `pan_cpu_s` and
  `zoom_cpu_s` are therefore much higher than on a real display and mostly
  useful to compare two builds, not as absolute values.
- Times are measured from outside, by reading pixels from the screen every two
  milliseconds. Times until something is visible include decoding, upload and
  drawing.
- Compare only results from the same machine and the same set of images.
  `compare` warns if they differ.
- Differences below 10 percent are not marked, they are within what two runs
  of the same build differ by. Change that with `--noise`.

## Kept results

`results/` holds measurements worth keeping, named after branch and commit.
Add a file when a change is supposed to improve something, so the next change
can be compared against it.
