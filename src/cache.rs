use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use image::DynamicImage;
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct Cache {
    /// By file and by which of its images, see `image_loader::image_count`
    pub data: HashMap<(PathBuf, usize), CachedImage>,
    /// How many images are kept at most
    pub cache_size: usize,
    /// How much memory the kept images may take, in bytes
    pub byte_budget: usize,
    /// The memory of the machine, in bytes
    pub total_memory: u64,
}

/// How much memory the cached images may take, a setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CacheMemory {
    /// A sixteenth of the memory of the machine, at least 256 MB and at most 1 GB.
    /// A photo of 24 megapixels takes about 72 MB.
    #[default]
    Auto,
    /// This many percent of the memory of the machine
    Percent(u8),
    /// This many megabytes
    Megabytes(u32),
}

impl CacheMemory {
    /// In bytes, on a machine with this much memory
    pub fn bytes(self, total_memory: u64) -> usize {
        const MB: u64 = 1024 * 1024;
        (match self {
            Self::Auto => (total_memory / 16).clamp(256 * MB, 1024 * MB),
            Self::Percent(percent) => total_memory * percent as u64 / 100,
            Self::Megabytes(megabytes) => megabytes as u64 * MB,
        }) as usize
    }
}

#[derive(Debug)]
pub struct CachedImage {
    /// Shared with whoever shows the image, so caching it does not copy it
    data: Arc<DynamicImage>,
    created: Instant,
}

impl Cache {
    pub fn new(cache_size: usize) -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        let total_memory = system.total_memory();
        Self {
            data: Default::default(),
            cache_size,
            byte_budget: CacheMemory::Auto.bytes(total_memory),
            total_memory,
        }
    }

    /// Keeps to the memory of the settings, the oldest images go if they no longer fit
    pub fn set_memory(&mut self, memory: CacheMemory) {
        self.byte_budget = memory.bytes(self.total_memory);
        self.trim();
    }

    pub fn get(&self, path: &Path, item: usize) -> Option<Arc<DynamicImage>> {
        self.data
            .get(&(path.to_owned(), item))
            .map(|c| c.data.clone())
    }

    /// Forget all images of a file
    pub fn remove(&mut self, path: &Path) {
        self.data.retain(|(p, _), _| p != path);
    }

    pub fn clear(&mut self) {
        self.data.clear()
    }

    pub fn insert(&mut self, path: &Path, item: usize, img: Arc<DynamicImage>) {
        self.data.insert(
            (path.into(), item),
            CachedImage {
                data: img,
                created: std::time::Instant::now(),
            },
        );
        self.trim();
    }

    /// The oldest images go until the cache fits both limits. The newest one stays,
    /// it is shown anyway.
    fn trim(&mut self) {
        while self.data.len() > 1
            && (self.data.len() > self.cache_size || self.bytes() > self.byte_budget)
        {
            let mut latest = std::time::Instant::now();
            let mut key = Default::default();

            for (p, c) in &self.data {
                if c.created < latest {
                    latest = c.created;
                    key = p.clone();
                }
            }
            debug!(
                "Cache limit hit, deleting oldest: {} ({}), {}s old",
                key.0.display(),
                key.1,
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
            cache.insert(Path::new(name), 0, image());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            cache.get(Path::new("a"), 0).is_none(),
            "the oldest image was kept"
        );
        assert!(cache.get(Path::new("b"), 0).is_some() && cache.get(Path::new("c"), 0).is_some());

        // an image larger than the budget is kept, it is the one shown
        cache.byte_budget = 10;
        cache.insert(Path::new("d"), 0, image());
        assert_eq!(cache.data.len(), 1);
        assert!(cache.get(Path::new("d"), 0).is_some());

        // the count still limits it too
        let mut cache = Cache::new(2);
        for name in ["a", "b", "c"] {
            cache.insert(Path::new(name), 0, image());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(cache.data.len(), 2);
        assert!(cache.get(Path::new("a"), 0).is_none());
    }

    /// The images of a file that holds several are kept apart, and go together
    #[test]
    fn ci_cache_keeps_images_of_a_file_apart() {
        let mut cache = Cache::new(10);
        let small = Arc::new(DynamicImage::ImageRgba8(image::RgbaImage::new(10, 10)));
        cache.insert(Path::new("a"), 0, image());
        cache.insert(Path::new("a"), 2, small);
        cache.insert(Path::new("b"), 0, image());
        assert_eq!(cache.get(Path::new("a"), 2).map(|i| i.width()), Some(10));
        assert_eq!(cache.get(Path::new("a"), 0).map(|i| i.width()), Some(100));
        assert!(cache.get(Path::new("a"), 1).is_none());
        cache.remove(Path::new("a"));
        assert!(cache.get(Path::new("a"), 0).is_none() && cache.get(Path::new("a"), 2).is_none());
        assert!(cache.get(Path::new("b"), 0).is_some());
    }

    /// The memory for the cache: by itself a sixteenth of the machine's, 256 MB to
    /// 1 GB, or a percent of it or megabytes from the settings
    #[test]
    fn memory_for_the_cache() {
        const GB: u64 = 1 << 30;
        let auto = |total| CacheMemory::Auto.bytes(total) >> 20;
        assert_eq!(
            (auto(2 * GB), auto(8 * GB), auto(64 * GB)),
            (256, 512, 1024)
        );
        let budget = Cache::new(10).byte_budget;
        assert!((256 << 20..=1 << 30).contains(&budget), "{budget}");
        assert_eq!(CacheMemory::Percent(25).bytes(16 * GB) as u64, 4 * GB);
        assert_eq!(CacheMemory::Megabytes(300).bytes(16 * GB), 300 << 20);
        // less of it, the oldest images go
        let mut cache = Cache::new(10);
        cache.total_memory = 8_000_000;
        for name in ["a", "b", "c"] {
            cache.insert(Path::new(name), 0, image());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(cache.data.len(), 3);
        // 80 000 bytes, two images
        cache.set_memory(CacheMemory::Percent(1));
        assert_eq!(cache.data.len(), 2);
        assert!(
            cache.get(Path::new("a"), 0).is_none(),
            "the oldest image was kept"
        );
        // saved in the settings like this
        let saved = serde_json::to_string(&CacheMemory::Percent(10)).expect("saved");
        assert_eq!(saved, r#"{"Percent":10}"#);
        assert_eq!(
            serde_json::from_str::<CacheMemory>(&saved).expect("read"),
            CacheMemory::Percent(10)
        );
    }
}
