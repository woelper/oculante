use crate::utils::is_ext_compatible;
use anyhow::{Context, Result, bail};
use log::{debug, warn};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

#[derive(Debug, Default)]
pub struct Scrubber {
    pub index: usize,
    pub entries: Vec<PathBuf>,
    pub wrap: bool,
    pub direction: Direction,
    pub fixed_paths: bool,
}

impl Scrubber {
    pub fn new(path: &Path) -> Self {
        let entries = get_image_filenames_for_directory(path).unwrap_or_default();
        let index = index_in(&entries, path).unwrap_or_default();
        Self {
            index,
            entries,
            wrap: true,
            direction: Direction::Forward,
            fixed_paths: false,
        }
    }

    pub fn has_next(&self) -> bool {
        self.entries.len() > self.index
    }

    /// Move scrubber forward
    // Not an iterator, this is the counterpart of `prev`
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> PathBuf {
        self.index += 1;
        if self.index == self.entries.len() {
            if self.wrap {
                self.index = 0;
            } else {
                self.index = self.entries.len() - 1;
            }
        }

        debug!("Next image in scrubber. Index is now {}", self.index);
        self.direction = Direction::Forward;
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    /// Move scrubber back
    pub fn prev(&mut self) -> PathBuf {
        if self.index == 0 {
            if self.wrap {
                self.index = self.entries.len().saturating_sub(1);
            }
        } else {
            self.index = self.index.saturating_sub(1);
        }
        debug!("Next image in scrubber. Index is now {}", self.index);

        self.direction = Direction::Backward;
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    pub fn remove_current(&mut self) -> PathBuf {
        debug!("Removing index {}", self.index);
        if self.entries.get(self.index).is_some() {
            self.entries.remove(self.index);
            match self.direction {
                Direction::Forward => {
                    self.index = self.index.saturating_sub(1);
                    self.next()
                }
                Direction::Backward => self.prev(),
            }
        } else {
            warn!("This index can't be removed.");
            Default::default()
        }
    }

    pub fn set(&mut self, index: usize) -> PathBuf {
        if index < self.entries.len() {
            self.index = index;
        }
        debug!("{:?}", self.entries.get(self.index));
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn has_folder_changed(&self, path_to_check: &Path) -> bool {
        let path_to_check = in_folder(path_to_check);
        self.entries
            .first()
            .map(|e| e.parent() != path_to_check.parent())
            .unwrap_or(true)
    }
}

/// A file name without a folder is in the current folder. The entries of the
/// current folder are listed as ./name, a file given as name has to be found
/// among them.
fn in_folder(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new(".").join(path),
        _ => path.to_path_buf(),
    }
}

/// Where a file is among the entries of its folder
fn index_in(entries: &[PathBuf], path: &Path) -> Option<usize> {
    let path = in_folder(path);
    entries.iter().position(|p| *p == path)
}

// Get sorted list of files in a folder
// TODO: Should probably return an Result<T,E> instead, but am too lazy to figure out + handle a dedicated error type here
// TODO: Cache this result, instead of doing it each time we need to fetch another file from the folder
pub fn get_image_filenames_for_directory(folder_path: &Path) -> Result<Vec<PathBuf>> {
    let mut folder_path = folder_path.to_path_buf();
    if folder_path.is_file() {
        folder_path = folder_path
            .parent()
            .map(|p| p.to_path_buf())
            .context("Can't get parent")?;
    }

    // fixes https://github.com/woelper/oculante/issues/482
    if folder_path.as_os_str().is_empty() {
        folder_path = PathBuf::from(".");
    }

    let info = std::fs::read_dir(folder_path)?;

    // TODO: Are symlinks handled correctly?
    let mut dir_files = info
        .flatten()
        .map(|x| x.path())
        .filter(|x| is_ext_compatible(x))
        .collect::<Vec<PathBuf>>();

    dir_files.sort_unstable_by(|a, b| {
        lexical_sort::natural_lexical_cmp(
            &a.file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default(),
            &b.file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default(),
        )
    });

    Ok(dir_files)
}

/// Find first valid image from the directory
/// Assumes the given path is a directory and not a file
pub fn find_first_image_in_directory(folder_path: &Path) -> Result<PathBuf> {
    if !folder_path.is_dir() {
        bail!("This is not a folder");
    };
    get_image_filenames_for_directory(folder_path).map(|x| {
        x.first()
            .cloned()
            .context("Folder does not have any supported images in it")
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oculante_test_scrubber_{name}"));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in files {
            std::fs::write(dir.join(file), b"").unwrap();
        }
        dir
    }

    fn names(scrubber: &Scrubber) -> Vec<String> {
        scrubber
            .entries
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect()
    }

    /// Images of a folder in natural order, whatever the case of their
    /// extension, other files left out
    #[test]
    fn natural_order_of_images() {
        let dir = folder(
            "order",
            &["img10.png", "img2.png", "img1.png", "IMG3.JPG", "notes.txt"],
        );
        let scrubber = Scrubber::new(&dir.join("img2.png"));
        assert_eq!(
            names(&scrubber),
            ["img1.png", "img2.png", "IMG3.JPG", "img10.png"]
        );
        assert_eq!(scrubber.index, 1);
        assert_eq!(
            find_first_image_in_directory(&dir).unwrap(),
            dir.join("img1.png")
        );
        assert!(
            find_first_image_in_directory(&dir.join("img1.png")).is_err(),
            "not a folder"
        );
        let empty = folder("empty", &["notes.txt"]);
        assert!(find_first_image_in_directory(&empty).is_err());
        _ = std::fs::remove_dir_all(dir);
        _ = std::fs::remove_dir_all(empty);
    }

    /// Next and previous at both ends, with and without wrapping around
    #[test]
    fn next_and_previous() {
        let dir = folder("navigate", &["a.png", "b.png", "c.png"]);
        let file = |name: &str| dir.join(name);
        let mut scrubber = Scrubber::new(&file("c.png"));
        assert_eq!(scrubber.next(), file("a.png"), "wraps to the start");
        assert_eq!(scrubber.prev(), file("c.png"), "wraps to the end");
        scrubber.wrap = false;
        assert_eq!(scrubber.next(), file("c.png"), "stays at the end");
        scrubber.set(0);
        assert_eq!(scrubber.prev(), file("a.png"), "stays at the start");
        assert_eq!(scrubber.next(), file("b.png"));
        _ = std::fs::remove_dir_all(dir);
    }

    /// Removing the shown image (after a delete) moves on to a neighbour, and
    /// the index stays valid down to the last image
    #[test]
    fn removing_entries() {
        let dir = folder("remove", &["a.png", "b.png", "c.png", "d.png"]);
        let file = |name: &str| dir.join(name);
        let mut scrubber = Scrubber::new(&file("b.png"));
        scrubber.wrap = false;
        assert_eq!(scrubber.remove_current(), file("c.png"), "the next one");
        scrubber.set(scrubber.len() - 1);
        assert_eq!(
            scrubber.remove_current(),
            file("c.png"),
            "the last one: the one before"
        );
        scrubber.prev();
        assert_eq!(scrubber.index, 0);
        assert_eq!(
            scrubber.remove_current(),
            file("c.png"),
            "going back from the first"
        );
        assert_eq!(scrubber.remove_current(), PathBuf::new(), "nothing left");
        assert!(scrubber.is_empty());
        _ = std::fs::remove_dir_all(dir);
    }

    /// A file given without a folder, as from a terminal in that folder, is
    /// found among the entries of the folder. The first press of an arrow key
    /// showed the same image again.
    #[test]
    fn file_without_folder() {
        let entries = vec![
            PathBuf::from("./a.png"),
            PathBuf::from("./b.png"),
            PathBuf::from("./c.png"),
        ];
        assert_eq!(index_in(&entries, Path::new("b.png")), Some(1));
        assert_eq!(index_in(&entries, Path::new("./c.png")), Some(2));
        let scrubber = Scrubber {
            index: 1,
            entries,
            wrap: true,
            direction: Direction::Forward,
            fixed_paths: false,
        };
        assert!(!scrubber.has_folder_changed(Path::new("b.png")));
        assert!(scrubber.has_folder_changed(Path::new("/elsewhere/b.png")));
    }
}
