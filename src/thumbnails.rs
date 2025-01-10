pub const THUMB_SIZE: [u32; 2] = [120, 90];
pub const THUMB_CAPTION_HEIGHT: u32 = 24;
pub const MAX_THREADS: usize = 4;

use std::{
    fs::{create_dir_all, File}, hash::{DefaultHasher, Hash, Hasher}, path::{Path, PathBuf}, sync::{Arc, Mutex}, time::Duration
};

use anyhow::{anyhow, bail, Context, Result};
use image::{imageops, DynamicImage, GenericImageView};
use log::{debug, error, trace, warn};

use crate::image_loader::open_image;

#[derive(Debug, Default, Clone)]
pub struct Thumbnails {
    /// The known thumbnail ids. This is used to avoid re-generating known thumbnails
    ids: Vec<PathBuf>,
    /// Number of thumbnails being created at a given time
    pool: Arc<Mutex<usize>>,
}

impl Thumbnails {
    pub fn get<P: AsRef<Path>>(&mut self, path: P) -> Result<PathBuf> {
        trace!("Thumbnail requested for {}", path.as_ref().display());

        if !get_disk_cache_path()?.exists() {
            warn!("Thumbnail cache dir missing, creating it");
            create_dir_all(&get_disk_cache_path()?)?;
        }
        let cached_path = get_cached_path(&path);

        // The thumbnail is missing and needs to be generated
        if !cached_path.exists() {
            if self.ids.contains(&cached_path) {
                bail!("Thumbnail is still processing or failed in the past.");
            }
            debug!("\tThumbnail missing");
            let fp = path.as_ref().to_path_buf();
            let pool = self.pool.clone();
            std::thread::spawn(move || {
                loop {
                    let num = *pool.lock().unwrap();
                    if num > MAX_THREADS {
                        std::thread::sleep(Duration::from_millis(100));
                    } else {
                        break;
                    }
                }
                *pool.lock().unwrap() += 1;
                if let Err(e) = generate(&fp) {
                    error!("Error generating thumbnail: {e}");
                }
                let num = *pool.lock().unwrap();
                *pool.lock().unwrap() = num.saturating_sub(1);
            });
            self.ids.push(cached_path);
            bail!("Thumbnail not yet present.");
        }
        Ok(cached_path)
    }
}

pub fn path_to_id<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    path.as_ref().hash(&mut hasher);
    let size = File::open(path).and_then(|f|f.metadata().map(|m|m.len())).unwrap_or_default();
    PathBuf::from(format!("{}_{size}",hasher.finish().to_string())).with_extension("png")
}

fn get_disk_cache_path() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .ok_or(anyhow!("Can't get local dir"))?
        .join("oculante")
        .join("thumbnails"))
}

pub fn get_cached_path<P: AsRef<Path>>(path: P) -> PathBuf {
    get_disk_cache_path()
        .unwrap_or_default()
        .join(path_to_id(path))
}

pub fn generate<P: AsRef<Path>>(source_path: P) -> Result<()> {
    let dest_path = get_cached_path(&source_path);
    debug!(
        "\tGen thumbnail for {} to {}",
        source_path.as_ref().display(),
        dest_path.display()
    );
    let f = open_image(source_path.as_ref(), None)?;
    let i = f.recv()?.get_image().context("Can't get buffer")?;

    debug!("\tOpened {}", source_path.as_ref().display());

    from_existing(dest_path, &i)?;
    Ok(())
}

pub fn from_existing<P: AsRef<Path>>(dest_path: P, image: &DynamicImage) -> Result<()> {
    let (mut width, mut height) = image.dimensions();
    let x = 0;
    let mut y = 0;

    if width < height {
        height = (width as f32 * 1. / 1.3333) as u32;
        y = (image.dimensions().0 as f32 - height as f32 / 2.) as u32;
    }

    if width > height {
        width = (height as f32 * 1.3333) as u32;
    }

    if width == height {
        height = (width as f32 * 1. / 1.3333) as u32;
    }

    let mut d = DynamicImage::ImageRgba8(image.crop_imm(x, y, width, height).to_rgba8());
    debug!("\tDim: {:?}", d.dimensions());

    // let op = ImageOperation::Resize {
    //     dimensions: (THUMB_SIZE[0], THUMB_SIZE[1]),
    //     aspect: false,
    //     filter: crate::image_editing::ScaleFilter::Bilinear,
    // };
    // op.process_image(&mut d)?;

    d = d.resize(
        THUMB_SIZE[0],
        THUMB_SIZE[1],
        imageops::FilterType::CatmullRom,
    );
    debug!("\tDim: {:?}", d.dimensions());
    d.save(&dest_path)?;
    debug!("\tSaved to {}.", dest_path.as_ref().display());
    Ok(())
}

#[test]
fn test_thumbs() {
    std::env::set_var("RUST_LOG", "debug");
    let _ = env_logger::try_init();
    let mut thumbs = Thumbnails::default();
    thumbs.get("tests/rust.png").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3000));
}
