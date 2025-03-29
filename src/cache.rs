use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};

use image::DynamicImage;
use log::debug;

#[derive(Debug, Default)]
pub struct Cache {
    pub data: HashMap<PathBuf, CachedImage>,
    pub cache_size: usize,
}

#[derive(Debug)]
pub struct CachedImage {
    data: DynamicImage,
    created: Instant,
}

impl Cache {
    pub fn get(&self, path: &Path) -> Option<DynamicImage> {
        self.data.get(path).map(|c| c.data.clone())
    }

    pub fn clear(&mut self) {
        self.data.clear()
    }

    /// Insert an image into the cache
    pub fn insert(&mut self, path: &Path, img: DynamicImage) {
        self.data.insert(
            path.into(),
            CachedImage {
                data: img,
                created: std::time::Instant::now(),
            },
        );
        if self.data.len() > self.cache_size {
            let mut latest = std::time::Instant::now();
            let mut key = PathBuf::new();

            for (p, c) in &self.data {
                if c.created < latest {
                    latest = c.created;
                    key = p.clone();
                }
            }
            debug!(
                "Cache limit hit, deleting oldest: {}, {}s old",
                key.display(),
                latest.elapsed().as_secs_f32()
            );

            _ = self.data.remove(&key);
        }
    }
}
