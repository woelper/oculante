use image::{DynamicImage, ImageReader};
use num_traits::{AsPrimitive, Signed};
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct ImageStatistics<A: Clone + Debug> {
    pub hist_bins: Vec<Vec<A>>,
    pub hist_value: Vec<f32>,
    pub distinct_colors: u64,
    pub transparent_pixels: u64,
    pub max_value:f32,
    pub min_value:f32
}

trait ArgumentReducer<A, V> {    
    fn reduce(&self, a: A, min:A, max:A, range:A) -> V;
    fn bins(&self) -> Vec<f32>;
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
    fn reduce(&self, a: u8, _min:u8, _max:u8, _range:u8) -> u8 {
        a
    }

    fn bins(&self) -> Vec<f32> {        
        (0..u8::MAX as i32+1).map(|f|{f as f32}).collect()
    }
}

impl<const BIT_DEPTH: usize> ArgumentReducer<u16, u16> for ArgumentReducerUnsigned16<BIT_DEPTH> {
    #[inline(always)]
    fn reduce(&self, a: u16, _min:u16, _max:u16, _range:u16) -> u16 {
        if BIT_DEPTH == 16 {
            a
        } else {
            let max_colors = (1 << BIT_DEPTH) - 1;
            a.min(max_colors)
        }
    }

    fn bins(&self) -> Vec<f32> {        
        (0..1<<BIT_DEPTH).map(|f|{f as f32}).collect()
    }
}

impl ArgumentReducer<f32, u16> for ArgumentReducerFloat32 {
    #[inline(always)]
    fn reduce(&self, a: f32, min:f32, _max:f32, range:f32) -> u16 {
        ((a-min)/range * (u16::MAX as f32)) as u16
        //(a * (u16::MAX as f32)) as u16
    }
    fn bins(&self) -> Vec<f32> {  
        let norm_factor = 1f32/(u16::MAX as f32);      
        (0..u16::MAX as i32+1).map(|f|{f as f32 * norm_factor}).collect()
    }
}

impl<const BIT_DEPTH: usize> ArgumentReducer<u16, u8> for ArgumentReducerUnsigned16ToU8<BIT_DEPTH> {
    #[inline(always)]
    fn reduce(&self, a: u16, _min:u16, _max:u16, _range:u16) -> u8 {
        a as u8
    }
    fn bins(&self) -> Vec<f32> {        
        (0..u8::MAX as i32+1).map(|f|{f as f32}).collect()
    }
}

impl ArgumentReducer<f32, u8> for ArgumentReducerFloat32ToU8 {
    #[inline(always)]
    fn reduce(&self, a: f32, min:f32, _max:f32, range:f32) -> u8 {
        ((a-min)/range * (u8::MAX as f32)) as u8
        //((a) * 255.) as u8
    }

    fn bins(&self) -> Vec<f32> {  
        let norm_factor = 1f32/(u8::MAX as f32);      
        (0..u8::MAX as i32+1).map(|f|{f as f32 * norm_factor}).collect()
    }
}

trait AlphaTest{ 
    fn is_transparent_alpha(self)-> bool;
    }

impl AlphaTest for u8 {
    #[inline(always)]
    fn is_transparent_alpha(self)-> bool{
        self == 0
    }
}

impl AlphaTest for u16 {
    #[inline(always)]
    fn is_transparent_alpha(self)-> bool{
        self == 0
    }
}

impl AlphaTest for f32 {
    #[inline(always)]
    fn is_transparent_alpha(self)-> bool{
        let mut value = self;

        if self.is_nan() {
            value= f32::MAX // Or any local positive extremum, as 1, 255, 65535 etc
        } else if self.is_infinite() && self.is_positive() {
            value= f32::MAX // Or any local positive extremum, as 1, 255, 65535 etc
        } else if self.is_infinite() && self.is_negative() {
            value= f32::MIN  // Or any local negative extremum, 0 if u8, u16 type required
        } else if self.is_subnormal() {
            value= 0. // Non-transparent value, since 1/65535 is subnormal, but for 16 bit-depth it is not transparent :)
        } 

        value <= f32::EPSILON
    }
}

pub trait MinMax {
    const MIN: Self;
    const MAX: Self;
    const RANGE_MIN: Self;
    const RANGE_MAX: Self;

    fn mini(self, b:Self)-> Self;
    fn maxi(self, b:Self)-> Self;
}

impl MinMax for u8 {
    const MIN: u8 = u8::MIN;
    const MAX: u8 = u8::MAX;
    const RANGE_MIN:u8 = 0u8;
    const RANGE_MAX:u8 = Self::MAX;

    #[inline(always)]
    fn mini(self, b:u8)-> u8{
        self.min(b)
    }
    #[inline(always)]
    fn maxi(self, b:u8)-> u8{
        self.max(b)
    }
}

impl MinMax for u16 {
    const MIN: u16 = u16::MIN;
    const MAX: u16 = u16::MAX;
    const RANGE_MIN:u16 = 0u16;
    const RANGE_MAX:u16 = Self::MAX;
    #[inline(always)]
    fn mini(self, b:u16)-> u16{
        self.min(b)
    }
    #[inline(always)]
    fn maxi(self, b:u16)-> u16{
        self.max(b)
    }
}

impl MinMax for f32 {
    const MIN: f32 = f32::MIN;
    const MAX: f32 = f32::MAX;
    const RANGE_MIN:f32 = 0.0f32;
    const RANGE_MAX:f32 = 1.0f32;
    #[inline(always)]
    fn mini(self, b:f32)-> f32{
        self.min(b)
    }
    #[inline(always)]
    fn maxi(self, b:f32)-> f32{
        self.max(b)
    }
}


fn calculate_statistics_impl<
    A: Clone + Debug + Copy + AlphaTest + MinMax + Into<f32> + std::ops::Sub<Output = A>,
    V: Default,
    const CN: usize,
    const USEFUL_CN: usize,
    const BINS_DEPTH: usize,
    const USE_MIN_MAX: bool
>(
    image: &[A],
    stride: usize,
    width: usize,
    reducer: /* Static dispatch is important here*/ impl ArgumentReducer<A, V>,
    distinct_colors_reducer: impl ArgumentReducer<A, u8>,
) -> ImageStatistics<u64>
where
    V: AsPrimitive<usize> + AsPrimitive<u32>,
{
    use std::time::Instant;
    assert!(USEFUL_CN >= 1 && USEFUL_CN <= 3);
    assert!(CN >= 1 && CN <= 4);
    let mut working_row = vec![V::default(); USEFUL_CN * width];

    let now = Instant::now();
    //Calculate min and max
    let mut max_value: A = A::MIN;
    let mut min_value: A = A::MAX;
    for chunk in image.chunks_exact(CN){
        for i in 0..USEFUL_CN{
            max_value = max_value.maxi(chunk[i]);            
            min_value = min_value.mini(chunk[i]);
        }
    }

    let min_scale: A;
    let max_scale: A;
    let range_scale: A;
    if(USE_MIN_MAX){
        min_scale = min_value;
        max_scale = max_value;
    }
    else {
        min_scale = A::RANGE_MIN;
        max_scale = A::RANGE_MAX;        
    }
    range_scale = max_scale-min_scale;

    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
    
    let now2 = Instant::now();
    let bins_count = 1 << BINS_DEPTH;
    let mut hist_bins = vec![];
    for _i in 0..USEFUL_CN{
        hist_bins.push(vec![0u64; bins_count]);
    }    
    
    /*let mut bin0 = vec![0u64; bins_count];
    let mut bin1 = if USEFUL_CN > 1 {
        vec![0u64; bins_count]
    } else {
        vec![0u64; 0]
    };
    let mut bin2 = if USEFUL_CN > 2 {
        vec![0u64; bins_count]
    } else {
        vec![0u64; 0]
    };*/

    let mut transparent_pixels: u64 = 0;
    let has_alpha = (CN-USEFUL_CN) == 1;

    let mut asdf = [0u8; 4];
    //let mut distinct_map = vec![0u8; 1 << 21];
    let mut distinct_colors = 0u64;

    //Colors counting
    const FIXED_RGB_SIZE: usize = 24;
    const SUB_INDEX_SIZE: usize = 5;
    const MAIN_INDEX_SIZE: usize = 1 << (FIXED_RGB_SIZE - SUB_INDEX_SIZE);
    let mut color_map = vec![0u32; MAIN_INDEX_SIZE];
    for chunk in image.chunks_exact(CN){
        let mut trans = true;
        
        for i in 0..USEFUL_CN{
            let c: usize = reducer.reduce(chunk[i], min_scale, max_scale, range_scale).as_();
            hist_bins[i][c] += 1;
            trans &= chunk[i].is_transparent_alpha();

            let col_reduced: u8 = distinct_colors_reducer.reduce(chunk[i], min_scale, max_scale, range_scale);
            asdf[i] = col_reduced;
        }
        if has_alpha{                
            trans &= chunk[USEFUL_CN].is_transparent_alpha();
            if trans{
                transparent_pixels += 1;
            }
            
        }            

        let pos: u32 = u32::from_ne_bytes(asdf);
                /*let full: u32 = v_u32 >> 3;
                let remainder = v_u32 - (full << 3);

                let bit = 1 << remainder;
                distinct_map[full as usize] |= bit;*/
            let pos_main = pos >> SUB_INDEX_SIZE;
            let pos_sub = pos - (pos_main << SUB_INDEX_SIZE);
            color_map[pos_main as usize] |= 1 << pos_sub;
    }
    let elapsed = now2.elapsed();
    println!("Elapsed: {:.2?}", elapsed);

    let now3 = Instant::now();

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

    /*working_row.resize(0, V::default());

    

    let mut d_working_row = vec![0u8; USEFUL_CN * width];

    

    for row in image.chunks_exact(stride) {
        let fixed_row = &row[0..width * CN];

        /*
        Distinct colors calculation is not straightforward, so quantization is always needed
        */
        for (dst, src) in d_working_row
            .chunks_exact_mut(USEFUL_CN)
            .zip(fixed_row.chunks_exact(CN))
        {
            dst[0] = distinct_colors_reducer.reduce(src[0], min_scale, max_scale, range_scale);
            if USEFUL_CN > 1 {
                dst[1] = distinct_colors_reducer.reduce(src[1], min_scale, max_scale, range_scale);
            }
            if USEFUL_CN > 2 {
                dst[2] = distinct_colors_reducer.reduce(src[2], min_scale, max_scale, range_scale);
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
    }*/

    /*for &ones in distinct_map.iter() {
        distinct_colors += ones.count_ones() as u64;
    }*/

    for &intensity in color_map.iter() {
        distinct_colors += intensity.count_ones() as u64;
    }

    let elapsed = now3.elapsed();
    println!("Elapsed: {:.2?}", elapsed);

    ImageStatistics {
        hist_bins,
        hist_value: reducer.bins(),
        distinct_colors,
        transparent_pixels,
        min_value: min_value.into(),
        max_value: max_value.into(),
    }
}

pub fn calculate_statistics(image: &DynamicImage) -> ImageStatistics<u64> {
    let width = image.width() as usize;
    let stride = image.width() as usize * image.color().channel_count() as usize;
    match image {
        DynamicImage::ImageLuma8(img) => calculate_statistics_impl::<u8, u8, 1, 1, 8, false>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageLumaA8(img) => calculate_statistics_impl::<u8, u8, 2, 1, 8, false>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageRgb8(img) => calculate_statistics_impl::<u8, u8, 3, 3, 8, false>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageRgba8(img) => calculate_statistics_impl::<u8, u8, 4, 3, 8, false>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned8::default(),
            ArgumentReducerUnsigned8::default(),
        ),
        DynamicImage::ImageLuma16(img) => calculate_statistics_impl::<u16, u16, 1, 1, 16, true>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageLumaA16(img) => calculate_statistics_impl::<u16, u16, 2, 1, 16, true>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgb16(img) => calculate_statistics_impl::<u16, u16, 3, 3, 16, true>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgba16(img) => calculate_statistics_impl::<u16, u16, 4, 3, 16, true>(
            &img,
            stride,
            width,
            ArgumentReducerUnsigned16::<16>::default(),
            ArgumentReducerUnsigned16ToU8::<16>::default(),
        ),
        DynamicImage::ImageRgb32F(img) => calculate_statistics_impl::<f32, u16, 3, 3, 16, true>(
            &img,
            stride,
            width,
            ArgumentReducerFloat32::default(),
            ArgumentReducerFloat32ToU8::default(),
        ),
        DynamicImage::ImageRgba32F(img) => calculate_statistics_impl::<f32, u16, 4, 3, 16, true>(
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
