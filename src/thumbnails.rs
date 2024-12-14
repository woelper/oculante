pub const THUMB_SIZE: [u32; 2] = [120, 90];
pub const THUMB_CAPTION_HEIGHT: u32 = 24;

use std::{
    fs::create_dir_all,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use image::{DynamicImage, GenericImageView};
use log::{debug, error, trace, warn};

use crate::{image_editing::ImageOperation, image_loader::open_image};

#[derive(Debug, Default, Clone)]
pub struct Thumbnails {
    /// The known thumbnail ids. This is used to avoid re-generating known thumbnails
    ids: Vec<PathBuf>,
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
            std::thread::spawn(move || {
                if let Err(e) = generate(&fp) {
                    error!("Error generating thumbnail: {e}");
                }
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
    PathBuf::from(hasher.finish().to_string()).with_extension("png")
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

    let op = ImageOperation::Resize {
        dimensions: (THUMB_SIZE[0], THUMB_SIZE[1]),
        aspect: true,
        filter: crate::image_editing::ScaleFilter::Bilinear,
    };
    op.process_image(&mut d)?;
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
    std::thread::sleep_ms(3000);
}
