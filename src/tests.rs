// extern crate test;
// use test::{Bencher, black_box};
use super::*;
use std::time::{Duration, Instant};
use std::path::PathBuf;

#[test]
fn load() {
    open_image(&PathBuf::from("tests/isabella-juskova-bECrTveml_w-unsplash.jpg"));
}


// #[bench]
// fn bench_load_large(b: &mut Bencher) {
//     // Optionally include some setup
 
//     let iters = 5;
//     let mut total = 0;

//     // for _i in 0..iters {
//     //     let start = Instant::now();
//     //     open_image(&PathBuf::from("tests/isabella-juskova-bECrTveml_w-unsplash.jpg"));
//     //     let elapsed = Instant::now().checked_duration_since(start);
//     //     total += elapsed.unwrap().as_millis();
//     // }
//     // dbg!(total/iters);
//     // total = 0; 

//     for _i in 0..iters {
//         let start = Instant::now();
//         open_image(&PathBuf::from("/home/woelper/Documents/oculante/tests/frstvisuals-lmV1g1UbdhQ-unsplash.jpg"));
//         let elapsed = Instant::now().checked_duration_since(start);
//         total += elapsed.unwrap().as_millis();
//         dbg!(elapsed); 
//     }
//     dbg!(total/iters);

//     // b.iter(|| {
//     //     // Inner closure, the actual test
//     //     for _i in 1..iters {
//     //         let start = Instant::now();
//     //         open_image(&PathBuf::from("tests/isabella-juskova-bECrTveml_w-unsplash.jpg"));
//     //         let elapsed = Instant::now().checked_duration_since(start);
//     //         dbg!(elapsed); 
//     //     }
//     // });
// }
