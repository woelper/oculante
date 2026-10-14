use criterion::{Criterion, criterion_group, criterion_main};
use oculante::image_editing::*;
use oculante::image_loader::*;
use std::path::PathBuf;

/// In RGBA, as the editor has it
fn moss() -> image::DynamicImage {
    let f = open_image(&PathBuf::from("res/tests/moss.jpg"), None, None).unwrap();
    image::DynamicImage::ImageRgba8(f.recv().unwrap().get_image().unwrap().to_rgba8())
}

fn pixel_ops() -> Vec<ImageOperation> {
    vec![
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
    ]
}

fn load_webp() {
    let f = open_image(&PathBuf::from("res/tests/mohsen-karimi.webp"), None, None).unwrap();
    let _buffer = f.recv().unwrap().get_image().unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    // the image is loaded once, a run starts from a copy of it
    let image = moss();
    let ops = pixel_ops();
    c.bench_function("pixel ops", |b| {
        b.iter(|| process_pixels(&mut image.clone(), &ops))
    });
    // whole-image operations, process_pixels skips them
    for (name, op) in [
        ("blur", ImageOperation::Blur(200)),
        (
            "resize",
            ImageOperation::Resize {
                dimensions: (300, 300),
                aspect: true,
                filter: ScaleFilter::Bilinear,
            },
        ),
        ("color fringe", ImageOperation::ChromaticAberration(30)),
    ] {
        c.bench_function(name, |b| b.iter(|| op.process_image(&mut image.clone())));
    }
    c.bench_function("load WebP", |b| b.iter(load_webp));
}

// criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
criterion_group! {
  name = benches;
  config = Criterion::default().measurement_time(std::time::Duration::from_secs(15));
  targets = criterion_benchmark
}
