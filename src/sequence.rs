//! A file that holds several images, such as the pages of a TIFF or the faces of
//! a cube map. They are shown one at a time and listed in the info panel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::image_loader::{open_image_item, rotate_dynimage};
use crate::utils::request_repaint;

/// The longer side of a picture in the list, in pixels
const THUMBNAIL_SIZE: u32 = 192;
/// How many pictures for the list are decoded at the same time
const DECODING_AT_ONCE: usize = 2;

pub struct Sequence {
    pub path: PathBuf,
    /// How many images the file holds, see `image_loader::image_count`
    pub count: usize,
    /// The image that is shown
    pub current: usize,
    thumbnails: HashMap<usize, Thumbnail>,
    decoded: (Sender<Decoded>, Receiver<Decoded>),
}

pub enum Thumbnail {
    Decoding,
    /// The picture, and the size of the image
    Done(egui::TextureHandle, [u32; 2]),
    Failed,
}

/// A picture for the list from a decoding thread, with the size of the image
type Decoded = (usize, Option<(egui::ColorImage, [u32; 2])>);

impl Sequence {
    pub fn new(path: &Path, current: usize, count: usize) -> Self {
        Self {
            path: path.to_owned(),
            count,
            current,
            thumbnails: Default::default(),
            decoded: channel(),
        }
    }

    /// The picture of an image for the list. It is decoded in the background the
    /// first time it is asked for, a few at a time.
    pub fn thumbnail(&mut self, ctx: &egui::Context, item: usize) -> Option<&Thumbnail> {
        while let Ok((done, decoded)) = self.decoded.1.try_recv() {
            let thumbnail = match decoded {
                Some((picture, size)) => Thumbnail::Done(
                    ctx.load_texture(format!("sequence {done}"), picture, Default::default()),
                    size,
                ),
                None => Thumbnail::Failed,
            };
            self.thumbnails.insert(done, thumbnail);
        }
        let decoding = self
            .thumbnails
            .values()
            .filter(|t| matches!(t, Thumbnail::Decoding))
            .count();
        if !self.thumbnails.contains_key(&item) && decoding < DECODING_AT_ONCE {
            self.thumbnails.insert(item, Thumbnail::Decoding);
            let (path, sender) = (self.path.clone(), self.decoded.0.clone());
            std::thread::spawn(move || {
                let decoded = (|| {
                    let frame = open_image_item(&path, item, None, None).ok()?.recv().ok()?;
                    let mut image = frame.get_image()?;
                    _ = rotate_dynimage(&mut image, &path);
                    let size = [image.width(), image.height()];
                    let small = image.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE).to_rgba8();
                    let picture = egui::ColorImage::from_rgba_unmultiplied(
                        [small.width() as usize, small.height() as usize],
                        small.as_raw(),
                    );
                    Some((picture, size))
                })();
                // the list may be gone by now, then nobody waits for it
                _ = sender.send((item, decoded));
                request_repaint();
            });
        }
        self.thumbnails.get(&item)
    }
}
