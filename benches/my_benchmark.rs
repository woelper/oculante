use std::hint::black_box;
use std::path::PathBuf;
use criterion::{criterion_group, criterion_main, Criterion};
use oculante::image_loader::*;
use oculante::image_editing::*;

fn ci_bench_process_pxl() {

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

        let f = open_image(
            &PathBuf::from("tests/test.jpg"),
            None,
        )
        .unwrap();
        let mut buffer = f.recv().unwrap().buffer;
        process_pixels(&mut buffer, &ops);
}


#[inline]
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n-1) + fibonacci(n-2),
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("fib 5", |b| b.iter(|| fibonacci(black_box(5))));
    c.bench_function("pixel ops", |b| b.iter(|| ci_bench_process_pxl()));
}

// criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
criterion_group!{
    name = benches;
    config = Criterion::default().measurement_time(std::time::Duration::from_secs(10));
    targets = criterion_benchmark
  }