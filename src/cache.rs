use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use image::DynamicImage;
use log::debug;

#[derive(Debug)]
pub struct Cache {
    pub data: HashMap<PathBuf, CachedImage>,
    /// How many images are kept at most
    pub cache_size: usize,
    /// How much memory the kept images may take, in bytes
    pub byte_budget: usize,
}

#[derive(Debug)]
pub struct CachedImage {
    /// Shared with whoever shows the image, so caching it does not copy it
    data: Arc<DynamicImage>,
    created: Instant,
}

impl Cache {
    pub fn new(cache_size: usize) -> Self {
        Self {
            data: Default::default(),
            cache_size,
            byte_budget: default_byte_budget(),
        }
    }

    pub fn get(&self, path: &Path) -> Option<Arc<DynamicImage>> {
        self.data.get(path).map(|c| c.data.clone())
    }

    pub fn clear(&mut self) {
        self.data.clear()
    }

    pub fn insert(&mut self, path: &Path, img: Arc<DynamicImage>) {
        self.data.insert(
            path.into(),
            CachedImage {
                data: img,
                created: std::time::Instant::now(),
            },
        );
        // The oldest images go until the cache fits both limits. The new one stays,
        // it is shown anyway.
        while self.data.len() > 1
            && (self.data.len() > self.cache_size || self.bytes() > self.byte_budget)
        {
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

    /// The memory the kept images take
    fn bytes(&self) -> usize {
        self.data.values().map(|c| c.data.as_bytes().len()).sum()
    }
}

/// A sixteenth of the memory of the machine, at least 256 MB and at most 1 GB.
/// A photo of 24 megapixels takes about 72 MB.
fn default_byte_budget() -> usize {
    const MB: u64 = 1024 * 1024;
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    (system.total_memory() / 16).clamp(256 * MB, 1024 * MB) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Arc<DynamicImage> {
        // 40 000 bytes
        Arc::new(DynamicImage::ImageRgba8(image::RgbaImage::new(100, 100)))
    }

    /// The cache keeps what fits into its memory budget, the oldest images go.
    /// It only counted images before, 30 photos took about 2 GB.
    #[test]
    fn cache_keeps_to_its_budget() {
        let mut cache = Cache::new(10);
        cache.byte_budget = 100_000;
        for name in ["a", "b", "c"] {
            cache.insert(Path::new(name), image());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            cache.get(Path::new("a")).is_none(),
            "the oldest image was kept"
        );
        assert!(cache.get(Path::new("b")).is_some() && cache.get(Path::new("c")).is_some());

        // an image larger than the budget is kept, it is the one shown
        cache.byte_budget = 10;
        cache.insert(Path::new("d"), image());
        assert_eq!(cache.data.len(), 1);
        assert!(cache.get(Path::new("d")).is_some());

        // the count still limits it too
        let mut cache = Cache::new(2);
        for name in ["a", "b", "c"] {
            cache.insert(Path::new(name), image());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(cache.data.len(), 2);
        assert!(cache.get(Path::new("a")).is_none());
    }

    #[test]
    fn budget_is_within_bounds() {
        let budget = default_byte_budget();
        assert!((256 << 20..=1 << 30).contains(&budget), "{budget}");
    }
}
