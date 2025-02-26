pub const THUMB_SIZE: [u32; 2] = [120, 90];
pub const THUMB_CAPTION_HEIGHT: u32 = 24;
pub const MAX_THREADS: usize = 4;

use std::{
    collections::HashSet,
    fs::{create_dir_all, File},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use image::{imageops, DynamicImage, GenericImageView};
use log::{debug, error, trace, warn};

use crate::image_loader::open_image;

#[derive(Debug, Default, Clone)]
pub struct Thumbnails {
    /// The known thumbnail ids. This is used to avoid re-generating known thumbnails
    ids: HashSet<PathBuf>,
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
            self.ids.insert(cached_path);
            bail!("Thumbnail not yet present.");
        }
        Ok(cached_path)
    }
}

pub fn path_to_id<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    path.as_ref().hash(&mut hasher);
    let size = File::open(path)
        .and_then(|f| f.metadata().map(|m| m.len()))
        .unwrap_or_default();
    PathBuf::from(format!("{}_{size}", hasher.finish())).with_extension("png")
}

pub fn get_disk_cache_path() -> Result<PathBuf> {
    Ok(dirs::cache_dir()
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
    debug!("TMB=> Original image size: {:?}", image.dimensions());

    let target_width = 120;
    let target_height = 90;
    let desired_aspect = target_width as f32 / target_height as f32;

    let (orig_width, orig_height) = image.dimensions();
    let orig_aspect = orig_width as f32 / orig_height as f32;

    let (crop_width, crop_height) = if orig_aspect > desired_aspect {
        // Image is too wide. Crop horizontally.
        let crop_w = (desired_aspect * orig_height as f32).round() as u32;
        (crop_w, orig_height)
    } else {
        // Image is too tall or just right width. Crop vertically.
        let crop_h = (orig_width as f32 / desired_aspect).round() as u32;
        (orig_width, crop_h)
    };

    let x_offset = (orig_width - crop_width) / 2;
    let y_offset = (orig_height - crop_height) / 2;

    let cropped_img =
        imageops::crop_imm(image, x_offset, y_offset, crop_width, crop_height).to_image();

    let mut d = DynamicImage::ImageRgba8(cropped_img);
    let op = crate::image_editing::ImageOperation::Resize {
        dimensions: (target_width, target_height),
        aspect: false,
        filter: crate::image_editing::ScaleFilter::Bilinear,
    };
    op.process_image(&mut d)?;
    d.save(&dest_path)?;

    Ok(())
}

#[test]
fn test_thumbs() {
    std::env::set_var("RUST_LOG", "debug");
    let _ = env_logger::try_init();
    let mut thumbs = Thumbnails::default();
    _ = thumbs.get("tests/rust.png");
    _ = thumbs.get("tests/ultrahigh.png");
    _ = thumbs.get("tests/mohsen-karimi.webp");
    std::thread::sleep(std::time::Duration::from_millis(1000));
}
