use image::{DynamicImage, ImageReader};
use num_traits::AsPrimitive;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct ImageStatistics<A: Clone + Debug> {
    pub hist_bins: Vec<Vec<A>>,
    pub hist_value: Vec<f32>,
    pub distinct_colors: u64,
    pub transparent_pixels: u64
}

trait ArgumentReducer<A, V> {
    fn reduce(&self, a: A) -> V;
}

#[derive(Clone, Default)]
struct ArgumentReducerUnsigned8 {}

#[derive(Clone, Default)]
struct ArgumentReducerUnsigned16<const BIT_DEPTH: usize> {}

#[derive(Clone, Default)]
struct ArgumentReducerUnsigned16ToU8<const BIT_DEPTH: usize> {}

#[derive(Clone, Default)]
struct ArgumentReducerFloat32 {}

#[derive(Clone, Default)]
struct ArgumentReducerFloat32ToU8 {}

impl ArgumentReducer<u8, u8> for ArgumentReducerUnsigned8 {
    #[inline(always)]
    fn reduce(&self, a: u8) -> u8 {
        a
    }
}

impl<const BIT_DEPTH: usize> ArgumentReducer<u16, u16> for ArgumentReducerUnsigned16<BIT_DEPTH> {
    #[inline(always)]
    fn reduce(&self, a: u16) -> u16 {
        if BIT_DEPTH == 16 {
            a
        } else {
            let max_colors = (1 << BIT_DEPTH) - 1;
            a.min(max_colors)
        }
    }
}

impl ArgumentReducer<f32, u16> for ArgumentReducerFloat32 {
    #[inline(always)]
    fn reduce(&self, a: f32) -> u16 {
        (a * (u16::MAX as f32)) as u16
    }
}

impl<const BIT_DEPTH: usize> ArgumentReducer<u16, u8> for ArgumentReducerUnsigned16ToU8<BIT_DEPTH> {
    #[inline(always)]
    fn reduce(&self, a: u16) -> u8 {
        a as u8
    }
}

impl ArgumentReducer<f32, u8> for ArgumentReducerFloat32ToU8 {
    #[inline(always)]
    fn reduce(&self, a: f32) -> u8 {
        (a * 255.) as u8
    }
}

fn calculate_statistics_impl<
    A: Clone + Debug + Copy + Into<f32>,
    V: Default,
    const CN: usize,
    const USEFUL_CN: usize,
    const BINS_DEPTH: usize,
>(
    image: &[A],
    stride: usize,
    width: usize,
    reducer: /* Static dispatch is important here*/ impl ArgumentReducer<A, V>,
    distinct_colors_reducer: impl ArgumentReducer<A, u8>,
) -> ImageStatistics<u64>
where
    V: AsPrimitive<usize> + AsPrimitive<u32> + AsPrimitive<f32>,
{
    assert!(USEFUL_CN >= 1 && USEFUL_CN <= 3);
    assert!(CN >= 1 && CN <= 4);
    let mut working_row = vec![V::default(); USEFUL_CN * width];

    let bins_count = 1 << BINS_DEPTH;
    let mut v2: Vec<f32> = vec![0f32; 256]; //TODO: calculate "the right way"
    for i in 0..256 {
        v2[i] = i as f32;
    }
    
    let mut bin0 = vec![0u64; bins_count];
    let mut bin1 = if USEFUL_CN > 1 {
        vec![0u64; bins_count]
    } else {
        vec![0u64; 0]
    };
    let mut bin2 = if USEFUL_CN > 2 {
        vec![0u64; bins_count]
    } else {
        vec![0u64; 0]
    };

    let mut transparent_pixels: u64 = 0;
    let has_alpha = (CN-USEFUL_CN) == 1;

    for chunk in image.chunks_exact(CN){
        let mut trans:bool = true;
        let c0: usize = reducer.reduce(chunk[0]).as_();
            bin0[c0] += 1;
            trans &= c0==0;
            if USEFUL_CN > 1 {
                let c1: usize = reducer.reduce(chunk[1]).as_();
                bin1[c1] += 1;
                trans &= c1==0;
            }
            if USEFUL_CN > 2 {
                let c2: usize = reducer.reduce(chunk[2]).as_();
                bin2[c2] += 1;
                trans &= c2==0;
            }
            if has_alpha{
                let alpha: f32 = chunk[USEFUL_CN].into();
                transparent_pixels += (alpha<=f32::EPSILON && trans) as u64;
            }
    }

    /*for row in image.chunks_exact(stride) {
        let fixed_row = &row[0..width * CN];
        /*
           For high bit-depth, 8-bit it is essentially pass through, but for `f32`
           some quantization is needed.
        */
        for (dst, src) in working_row
            .chunks_exact_mut(USEFUL_CN)
            .zip(fixed_row.chunks_exact(CN))
        {
            dst[0] = reducer.reduce(src[0]);
            if USEFUL_CN > 1 {
                dst[1] = reducer.reduce(src[1]);
            }
            if USEFUL_CN > 2 {
                dst[2] = reducer.reduce(src[2]);
            }
            
        }

        // Calculate hist
        for chunk in working_row.chunks_exact_mut(USEFUL_CN) {
            let c0: usize = chunk[0].as_();
            bin0[c0] += 1;
            if USEFUL_CN > 1 {
                let c1: usize = chunk[1].as_();
                bin1[c1] += 1;
            }
            if USEFUL_CN > 2 {
                let c2: usize = chunk[2].as_();
                bin2[c2] += 1;
            }
            /*if CN==4{
                let alpha: f32 = chunk[3].as_();
                transparent_pixels += (alpha<=f32::EPSILON) as u64;
            }*/
        }
    }*/

    working_row.resize(0, V::default());

    let mut distinct_colors = 0u64;

    let mut d_working_row = vec![0u8; USEFUL_CN * width];

    let mut distinct_map = vec![0u8; 1 << 21];

    for row in image.chunks_exact(stride) {
        let fixed_row = &row[0..width * CN];

        /*
        Distinct colors calculation is not straightforward, so quantization is always needed
        */
        for (dst, src) in d_working_row
            .chunks_exact_mut(USEFUL_CN)
            .zip(fixed_row.chunks_exact(CN))
        {
            dst[0] = distinct_colors_reducer.reduce(src[0]);
            if USEFUL_CN > 1 {
                dst[1] = distinct_colors_reducer.reduce(src[1]);
            }
            if USEFUL_CN > 2 {
                dst[2] = distinct_colors_reducer.reduce(src[2]);
            }
        }

        if USEFUL_CN == 1 {
            for &v in d_working_row.iter() {
                let full: u32 = v as u32 >> 3;
                let remainder = v as u32 - (full << 3);

                let bit = 1 << remainder;
                distinct_map[full as usize] |= bit;
            }
        } else if USEFUL_CN == 2 {
            for chunk in d_working_row.chunks_exact_mut(2) {
                let c0: u8 = chunk[0];
                let c1: u8 = chunk[1];
                let v_u32: u32 = u32::from_ne_bytes([c0, c1, 0, 0]);
                let full: u32 = v_u32 >> 3;
                let remainder = v_u32 - (full << 3);

                let bit = 1 << remainder;
                distinct_map[full as usize] |= bit;
            }
        } else if USEFUL_CN == 3 {
            for chunk in d_working_row.chunks_exact_mut(3) {
                let c0: u8 = chunk[0];
                let c1: u8 = chunk[1];
                let c2: u8 = chunk[2];
                let v_u32: u32 = u32::from_ne_bytes([c0, c1, c2, 0]);
                let full: u32 = v_u32 >> 3;
                let remainder = v_u32 - (full << 3);

                let bit = 1 << remainder;
                distinct_map[full as usize] |= bit;
            }
        }
    }

    for &ones in distinct_map.iter() {
        distinct_colors += ones.count_ones() as u64;
    }

    let mut hist_bins = vec![];
    hist_bins.push(bin0);
    if USEFUL_CN > 1 {
        hist_bins.push(bin1);
    }
    if USEFUL_CN > 2 {
        hist_bins.push(bin2);
    }

    ImageStatistics {
        hist_bins,
        hist_value: v2,
        distinct_colors,
        transparent_pixels
    }
}

pub fn calculate_statistics(image: &DynamicImage) -> ImageStatistics<u64> {
    let width = image.width() as usize;
    let stride = image.width() as usize * image.color().channel_count() as usize;
    match image {
        DynamicImage::ImageLuma8(img) => calculate_statistics_impl::<u8, u8, 1, 1, 8>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageLumaA8(img) => calculate_statistics_impl::<u8, u8, 2, 1, 8>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageRgb8(img) => calculate_statistics_impl::<u8, u8, 3, 3, 8>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageRgba8(img) => calculate_statistics_impl::<u8, u8, 4, 3, 8>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageLuma16(img) => calculate_statistics_impl::<u16, u16, 1, 1, 16>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageLumaA16(img) => calculate_statistics_impl::<u16, u16, 2, 1, 16>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgb16(img) => calculate_statistics_impl::<u16, u16, 3, 3, 16>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgba16(img) => calculate_statistics_impl::<u16, u16, 4, 3, 16>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgb32F(img) => calculate_statistics_impl::<f32, u16, 3, 3, 16>(
            &img,
            stride,
            width,
            ArgumentReducerFloat32::default(),
            ArgumentReducerFloat32ToU8::default(),
        ),
        DynamicImage::ImageRgba32F(img) => calculate_statistics_impl::<f32, u16, 4, 3, 16>(
            &img,
            stride,
            width,
            ArgumentReducerFloat32::default(),
            ArgumentReducerFloat32ToU8::default(),
        ),
        _ => unimplemented!(),
    }
}

fn main() {
    let mut img = ImageReader::open("./assets/bench.png")
        .unwrap()
        .decode()
        .unwrap();

    let img_f32 = DynamicImage::ImageRgba8(img.to_rgba8());

    let hist = calculate_statistics(&img_f32);

    println!("Hist {:?}", hist);
}
