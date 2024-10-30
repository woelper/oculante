use criterion::{criterion_group, criterion_main, Criterion};
use oculante::image_editing::*;
use oculante::image_loader::*;
use std::hint::black_box;
use std::path::PathBuf;

fn process_pixel_ops() {
    let ops = vec![
        ImageOperation::Brightness(10),
        ImageOperation::Contrast(10),
        ImageOperation::Exposure(20),
        ImageOperation::Equalize((10, 100)),
        ImageOperation::Posterize(4),
        ImageOperation::Desaturate(20),
        ImageOperation::HSV((20, 0, 0)),
        ImageOperation::Noise {
            amt: 50,
            mono: false,
        },
        ImageOperation::ChromaticAberration(30),
    ];
    let f = open_image(&PathBuf::from("tests/moss.jpg"), None).unwrap();
    let mut buffer = f.recv().unwrap().buffer;
    process_pixels(&mut buffer, &ops);
}

fn blur() {
    let ops = vec![ImageOperation::Blur(200)];
    let f = open_image(&PathBuf::from("tests/moss.jpg"), None).unwrap();
    let mut buffer = f.recv().unwrap().buffer;
    process_pixels(&mut buffer, &ops);
}

fn resize() {
    let ops = vec![ImageOperation::Resize {
        dimensions: (300, 300),
        aspect: true,
        filter: ScaleFilter::Bilinear,
    }];
    let f = open_image(&PathBuf::from("tests/moss.jpg"), None).unwrap();
    let mut buffer = f.recv().unwrap().buffer;
    process_pixels(&mut buffer, &ops);
}

fn load_webp() {
    let f = open_image(&PathBuf::from("tests/mohsen-karimi.webp"), None).unwrap();
    let _buffer = f.recv().unwrap().buffer;
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("pixel ops", |b| b.iter(|| process_pixel_ops()));
    c.bench_function("blur", |b| b.iter(|| blur()));
    c.bench_function("resize", |b| b.iter(|| resize()));
    c.bench_function("load WebP", |b| b.iter(|| load_webp()));
}

// criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
criterion_group! {
  name = benches;
  config = Criterion::default().measurement_time(std::time::Duration::from_secs(15));
  targets = criterion_benchmark
}
