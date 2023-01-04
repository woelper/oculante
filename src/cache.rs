use std::{path::{PathBuf, Path}, collections::HashMap, time::Instant};

use image::RgbaImage;
use log::debug;

#[derive(Debug)]
pub struct Cache {
    pub data: HashMap<PathBuf, CachedImage>,
    pub cache_size: usize,
}

#[derive(Debug)]
pub struct CachedImage {
    data: RgbaImage,
    created: Instant,
}


impl Cache {
    pub fn get(&self, path: &Path) -> Option<RgbaImage> {
        self.data.get(path).map(|c| c.data.clone())
    }

    pub fn insert(&mut self, path: &Path, img: RgbaImage) {
        self.data.insert(
            path.into(),
            CachedImage {
                data: img,
                created: std::time::Instant::now(),
            },
        );
        if self.data.len() > self.cache_size {
            debug!("Cache limit hit, deleting oldest");
            let mut latest = std::time::Instant::now();
            let mut key = PathBuf::new();

            for (p, c) in &self.data {
                if c.created < latest {
                    latest = c.created;
                    key = p.clone();
                }
            }

            _ = self.data.remove(&key);
        }
    }
}
