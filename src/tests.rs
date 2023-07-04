use crate::image_loader::*;
use cmd_lib::run_cmd;

use crate::{
    image_editing::{process_pixels, ImageOperation, ScaleFilter},
    shortcuts::{keypresses_as_markdown, ShortcutExt, Shortcuts},
};

use super::*;
use std::{fs::File, io::Write, path::PathBuf, time::Instant};

#[test]
fn load() {
    open_image(&PathBuf::from("tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg")).unwrap();
}

#[test]
/// This test needs a window system and user verification (it should show an image)
fn net() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();

    info!("Spawn thread...");
    std::thread::spawn(|| {
        std::process::Command::new("cargo")
            .args(["run", "--", "-l", "11111"])
            .output()
            .unwrap();
    });

    info!("Ran the app, sleeping");

    std::thread::sleep(std::time::Duration::from_secs(5));
    info!("Running nc");

    run_cmd! (
        nc localhost 11111 < tests/test.jpg;
    )
    .unwrap();
}

#[test]
fn bench_load_large() {
    #[cfg(debug_assertions)]
    panic!("This test needs release mode to pass.");
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    let iters = 5;

    info!("Benching JPEG with {iters} iterations...");
    let mut total = 0;

    for _i in 0..iters {
        let start = Instant::now();
        open_image(&PathBuf::from(
            "tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg",
        ))
        .unwrap();
        let elapsed = start.elapsed();
        let d = elapsed.as_millis();
        total += d;
        info!("Loaded jpg image in {}", d);
    }
    info!("{} ms mean", total / iters);
    let mut f = File::create("benches/load_large_jpg.bench").unwrap();
    f.write_fmt(format_args!("{}ms", total / iters)).unwrap();

    info!("Benching PNG with {iters} iterations...");
    let mut total = 0;

    for _i in 0..iters {
        let start = Instant::now();
        open_image(&PathBuf::from("tests/large.png")).unwrap();
        let elapsed = start.elapsed();
        let d = elapsed.as_millis();
        total += d;
        info!("Loaded png image in {}", d);
    }
    info!("{} ms mean", total / iters);
    let mut f = File::create("benches/load_large_png.bench").unwrap();
    f.write_fmt(format_args!("{}ms", total / iters)).unwrap();
}

#[test]
fn bench_process_pxl() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    let iters = 5;
    info!("Benching this with {iters} iterations...");
    let mut total = 0;

    let ops = vec![
        ImageOperation::Brightness(10),
        ImageOperation::Contrast(10),
        ImageOperation::Exposure(20),
        ImageOperation::Equalize((10, 100)),
        ImageOperation::Posterize(4),
        ImageOperation::Desaturate(20),
        ImageOperation::HSV((20, 0, 0)),
        // ImageOperation::Noise {amt: 50, mono: false},
    ];

    for _i in 0..iters {
        let f = open_image(&PathBuf::from(
            "tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg",
        ))
        .unwrap();
        let mut buffer = f.recv().unwrap().buffer;
        let start = Instant::now();
        process_pixels(&mut buffer, &ops);
        let elapsed = start.elapsed();
        let d = elapsed.as_millis();
        total += d;
        info!("Processed image in {} s", elapsed.as_secs_f32());
    }
    info!("{} ms mean", total / iters);
    info!("295");
}

#[test]
fn bench_process_bright() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    let iters = 5;
    info!("Benching this with {iters} iterations...");
    let mut total = 0;

    let ops = vec![ImageOperation::Brightness(10)];

    for _i in 0..iters {
        let f = open_image(&PathBuf::from(
            "tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg",
        ))
        .unwrap();
        let mut buffer = f.recv().unwrap().buffer;
        let start = Instant::now();
        process_pixels(&mut buffer, &ops);
        let elapsed = start.elapsed();
        let d = elapsed.as_millis();
        total += d;
        info!("Processed image in {} s", elapsed.as_secs_f32());
    }
    info!("{} ms mean", total / iters);
    info!("24 simd");
}

#[test]
fn dump_shortcuts() {
    use std::io::prelude::*;
    let mut shortcuts_file = File::create("shortcuts.txt").unwrap();

    let shortcuts = Shortcuts::default_keys();
    let mut ordered_shortcuts = shortcuts.iter().collect::<Vec<_>>();
    ordered_shortcuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (k, v) in ordered_shortcuts {
        writeln!(shortcuts_file, "{} = {:?}\n", keypresses_as_markdown(&v), k).unwrap();
    }
}

#[test]
fn bench_process_all() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    let iters = 5;
    info!("Multi-ops: benching this with {iters} iterations...");
    let mut total = 0;

    // let blur = ImageOperation::Blur(10);

    for _i in 0..iters {
        let ops = vec![
            ImageOperation::Brightness(10),
            ImageOperation::ChromaticAberration(5),
            // ImageOperation::Blur(5),
            ImageOperation::Desaturate(100),
            ImageOperation::Resize {
                dimensions: (300, 200),
                aspect: true,
                filter: ScaleFilter::Hamming,
            },
            // ImageOperation::
        ];
        let f = open_image(&PathBuf::from(
            "tests/mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg",
        ))
        .unwrap();
        let mut buffer = f.recv().unwrap().buffer;
        let start = Instant::now();
        process_pixels(&mut buffer, &ops);

        for op in ops {
            info!("IMG {:?}", op);
            op.process_image(&mut buffer).unwrap();
        }

        let elapsed = start.elapsed();
        let d = elapsed.as_millis();
        total += d;
        info!("Processed image in {} s", elapsed.as_secs_f32());
        info!("simd 1.467s");
    }
    info!("{} ms mean", total / iters);
}
