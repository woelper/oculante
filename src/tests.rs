use super::*;
use std::{path::PathBuf, time::Instant};

#[test]
fn load() {
    open_image(&PathBuf::from("tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg")).unwrap();
}

#[test]
fn net() {
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();
    std::process::Command::new("cargo")
        .args(["run", "--", "-l", "11111"])
        .spawn()
        .unwrap();
    // this is not yet supported
    // std::process::Command::new("nc")
    //     .args([
    //         "localhost",
    //         "11111",
    //         "<",
    //         "tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg",
    //     ])
    //     .status()
    //     .unwrap();
    info!("For now, this test needs to run manually:");
    info!("nc localhost 11111 < tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg");
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
