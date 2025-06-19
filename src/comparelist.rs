use std::{
    cmp,
    path::{Path, PathBuf},
};

use crate::appstate::ImageGeometry;

/// List of images to compare, sorted by [`PathBuf`].
#[derive(Default)]
pub struct CompareList {
    index: usize,
    list: Vec<CompareItem>,
}

impl CompareList {
    /// Cycle through [`CompareItem`]s.
    pub fn next(&mut self) -> Option<&CompareItem> {
        (self.index + 1).checked_rem(self.list.len()).and_then(|i| {
            self.index = i;
            self.list.get(i)
        })
    }

    /// Insert a unique [`CompareItem`].
    pub fn insert(&mut self, item: CompareItem) {
        // The internal vector is always sorted so we can always binary search.
        // Binary search is slower than a hash table but still fast (log(n)).
        // It also avoids sorting on each next call or draw.
        if let Err(i) = self.list.binary_search(&item) {
            // By inserting where the item is expected, sort order is preserved without needing to
            // sort the slice again.
            self.list.insert(i, item);
            debug_assert!(
                self.list.is_sorted(),
                "Compare list should always be sorted"
            );
        }
    }

    /// Remove item by [`Path`] if it exists.
    pub fn remove(&mut self, path: impl AsRef<Path>) -> Option<CompareItem> {
        let path = path.as_ref();
        self.list
            .binary_search_by_key(&path, |item| &item.path)
            .ok()
            .map(|index| self.list.remove(index))
    }

    /// Get [`ImageGeometry`] by [`Path`] if exists.
    pub fn get(&self, path: impl AsRef<Path>) -> Option<ImageGeometry> {
        let path = path.as_ref();
        self.list
            .binary_search_by_key(&path, |item| &item.path)
            .ok()
            .and_then(|index| self.list.get(index).map(|item| item.geometry))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.list.clear();
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &CompareItem> + use<'_> {
        self.list.iter()
    }
}

pub struct CompareItem {
    pub path: PathBuf,
    pub geometry: ImageGeometry,
}

impl CompareItem {
    pub fn new(path: impl AsRef<Path>, geometry: ImageGeometry) -> Self {
        let path = path.as_ref().to_path_buf();
        Self { path, geometry }
    }
}

impl PartialEq for CompareItem {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for CompareItem {}

impl PartialOrd for CompareItem {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.path.partial_cmp(&other.path)
    }
}

impl Ord for CompareItem {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.path.cmp(&other.path)
    }
}
