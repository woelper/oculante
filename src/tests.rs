use cmd_lib::run_cmd;

use super::*;
use std::{path::PathBuf, time::Instant};

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
    ).unwrap();

}

#[test]
fn bench_load_large() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    let iters = 5;
    info!("Benching this with {iters} iterations...");
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
        info!("Loaded image in {}", d);
    }
    info!("{} ms mean", total / iters);
}
