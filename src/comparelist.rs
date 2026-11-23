use std::{
    cmp,
    path::{Path, PathBuf},
};

use crate::appstate::ImageGeometry;

/// List of images to compare, sorted by [`PathBuf`] and item.
#[derive(Default)]
pub struct CompareList {
    index: usize,
    list: Vec<CompareItem>,
}

impl CompareList {
    /// Cycle through [`CompareItem`]s.
    // Not an iterator, it wraps around and never ends
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&CompareItem> {
        (self.index + 1).checked_rem(self.list.len()).and_then(|i| {
            self.index = i;
            self.list.get(i)
        })
    }

    /// Insert a [`CompareItem`]. An item with the same path and item is replaced,
    /// which is how the position of an image in the list is updated.
    pub fn insert(&mut self, item: CompareItem) {
        // The internal vector is always sorted so we can always binary search.
        // Binary search is slower than a hash table but still fast (log(n)).
        // It also avoids sorting on each next call or draw.
        match self.list.binary_search(&item) {
            Ok(i) => self.list[i] = item,
            Err(i) => {
                // By inserting where the item is expected, sort order is preserved without
                // needing to sort the slice again.
                self.list.insert(i, item);
                debug_assert!(
                    self.list.is_sorted(),
                    "Compare list should always be sorted"
                );
            }
        }
    }

    /// Remove an image, `item` is one of the images of a file that holds several.
    pub fn remove(&mut self, path: impl AsRef<Path>, item: usize) -> Option<CompareItem> {
        let key = (path.as_ref(), item);
        self.list
            .binary_search_by_key(&key, CompareItem::key)
            .ok()
            .map(|index| self.list.remove(index))
    }

    /// The [`ImageGeometry`] stored for an image, if it is in the list.
    pub fn get(&self, path: impl AsRef<Path>, item: usize) -> Option<ImageGeometry> {
        let key = (path.as_ref(), item);
        self.list
            .binary_search_by_key(&key, CompareItem::key)
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
    /// One of the images of a file that holds several, 0 for all others
    pub item: usize,
    pub geometry: ImageGeometry,
}

impl CompareItem {
    pub fn new(path: impl AsRef<Path>, item: usize, geometry: ImageGeometry) -> Self {
        let path = path.as_ref().to_path_buf();
        Self {
            path,
            item,
            geometry,
        }
    }

    fn key(&self) -> (&Path, usize) {
        (&self.path, self.item)
    }
}

impl PartialEq for CompareItem {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for CompareItem {}

impl PartialOrd for CompareItem {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CompareItem {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Vector2;

    fn geometry(scale: f32) -> ImageGeometry {
        ImageGeometry {
            scale,
            offset: Vector2::new(scale, scale),
            dimensions: (10, 10),
        }
    }

    #[test]
    fn inserting_a_known_path_updates_its_position() {
        let mut list = CompareList::default();
        list.insert(CompareItem::new("a.png", 0, geometry(1.0)));
        list.insert(CompareItem::new("a.png", 0, geometry(2.0)));
        assert_eq!(list.len(), 1);
        assert_eq!(list.get("a.png", 0), Some(geometry(2.0)));
        // another image of the same file is another entry
        list.insert(CompareItem::new("a.png", 1, geometry(3.0)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.get("a.png", 1), Some(geometry(3.0)));
        list.remove("a.png", 0);
        assert_eq!((list.len(), list.get("a.png", 0)), (1, None));
    }

    #[test]
    fn items_stay_sorted_and_unique() {
        let mut list = CompareList::default();
        for name in ["c.png", "a.png", "b.png", "a.png"] {
            list.insert(CompareItem::new(name, 0, geometry(1.0)));
        }
        let paths = list
            .iter()
            .map(|item| item.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            ["a.png", "b.png", "c.png"].map(PathBuf::from).to_vec()
        );
    }

    #[test]
    fn next_cycles_through_the_list() {
        // The list only keeps the paths, the files do not have to exist
        let mut list = CompareList::default();
        assert!(list.next().is_none());
        for name in ["a.png", "b.png", "c.png"] {
            list.insert(CompareItem::new(name, 0, geometry(1.0)));
        }
        let mut visited = (0..4)
            .map(|_| list.next().map(|item| item.path.clone()))
            .collect::<Vec<_>>();
        // after all three, the first one comes around again
        assert_eq!(visited[0], visited[3]);
        visited.truncate(3);
        visited.sort();
        assert_eq!(
            visited,
            ["a.png", "b.png", "c.png"].map(|name| Some(PathBuf::from(name)))
        );
    }
}
