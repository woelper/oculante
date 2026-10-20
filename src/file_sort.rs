//! The order of the files of a folder: in the file browser, and for the next and
//! the previous image (#814).

use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cmp::Ordering,
    path::{Path, PathBuf},
    time::SystemTime,
};
use strum::EnumIter;

/// What files are sorted by
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum SortBy {
    #[default]
    Name,
    Modified,
    Created,
    Size,
    Type,
}

impl SortBy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Modified => "Date modified",
            Self::Created => "Date created",
            Self::Size => "Size",
            Self::Type => "Type",
        }
    }
}

/// The order of the files of a folder. Folders come first either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSort {
    pub by: SortBy,
    pub descending: bool,
}

/// What the order needs to know about a file, read once
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Not every file system keeps it, the time of the last change stands in then
    pub created: Option<SystemTime>,
}

impl FileInfo {
    pub fn read(path: PathBuf) -> Self {
        // a link counts as what it points to
        match std::fs::metadata(&path) {
            Ok(meta) => Self {
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified: meta.modified().ok(),
                created: meta.created().or_else(|_| meta.modified()).ok(),
                path,
            },
            Err(_) => Self {
                path,
                is_dir: false,
                size: 0,
                modified: None,
                created: None,
            },
        }
    }

    pub fn name(&self) -> Cow<'_, str> {
        name(&self.path)
    }

    /// The extension in lower case, empty for folders and files without one
    pub fn extension(&self) -> String {
        if self.is_dir {
            return String::new();
        }
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
}

fn name(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default()
}

/// Numbers in names go by their value, "2" before "10", and case does not count
fn cmp_names(a: &Path, b: &Path) -> Ordering {
    lexical_sort::natural_lexical_cmp(&name(a), &name(b))
}

impl FileSort {
    /// Folders first, then the files. What is the same by this order goes by name.
    pub fn sort(self, entries: &mut [FileInfo]) {
        entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                let order = match self.by {
                    SortBy::Name => Ordering::Equal,
                    SortBy::Modified => a.modified.cmp(&b.modified),
                    SortBy::Created => a.created.cmp(&b.created),
                    SortBy::Size => a.size.cmp(&b.size),
                    SortBy::Type => a.extension().cmp(&b.extension()),
                }
                .then_with(|| cmp_names(&a.path, &b.path));
                if self.descending {
                    order.reverse()
                } else {
                    order
                }
            })
        });
    }

    /// Files in this order. Their size and dates are only read when the order needs them,
    /// a folder on the network can have thousands.
    pub fn sort_files(self, files: Vec<PathBuf>) -> Vec<PathBuf> {
        if self.by == SortBy::Name {
            let mut files = files;
            files.sort_by(|a, b| {
                let order = cmp_names(a, b);
                if self.descending {
                    order.reverse()
                } else {
                    order
                }
            });
            return files;
        }
        let mut infos = files.into_iter().map(FileInfo::read).collect::<Vec<_>>();
        self.sort(&mut infos);
        infos.into_iter().map(|info| info.path).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use strum::IntoEnumIterator;

    /// Files with a size and a time of change each, and a folder
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oculante_test_file_sort_{name}"));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        // name, size, days after start
        for (file, size, days) in [
            ("b10.png", 300, 1),
            ("b2.jpg", 100, 3),
            ("A1.webp", 200, 2),
            ("c.png", 100, 0),
        ] {
            let path = dir.join(file);
            std::fs::write(&path, vec![0; size]).unwrap();
            let f = std::fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(start + Duration::from_secs(days * 86_400))
                .unwrap();
        }
        dir
    }

    fn names(entries: &[FileInfo]) -> Vec<String> {
        entries.iter().map(|e| e.name().to_string()).collect()
    }

    fn sorted(dir: &Path, by: SortBy, descending: bool) -> Vec<String> {
        let mut entries = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| FileInfo::read(e.path()))
            .collect::<Vec<_>>();
        FileSort { by, descending }.sort(&mut entries);
        names(&entries)
    }

    #[test]
    fn files_go_by_what_is_chosen_and_folders_first() {
        let dir = folder("orders");
        assert_eq!(
            sorted(&dir, SortBy::Name, false),
            ["sub", "A1.webp", "b2.jpg", "b10.png", "c.png"]
        );
        assert_eq!(
            sorted(&dir, SortBy::Modified, false),
            ["sub", "c.png", "b10.png", "A1.webp", "b2.jpg"]
        );
        // the same size goes by name
        assert_eq!(
            sorted(&dir, SortBy::Size, false),
            ["sub", "b2.jpg", "c.png", "A1.webp", "b10.png"]
        );
        assert_eq!(
            sorted(&dir, SortBy::Type, false),
            ["sub", "b2.jpg", "b10.png", "c.png", "A1.webp"]
        );
        // turned around, but the folder stays first
        for by in SortBy::iter() {
            let up = sorted(&dir, by, false);
            let down = sorted(&dir, by, true);
            assert_eq!(down[0], "sub", "{by:?}");
            assert_eq!(
                down[1..].iter().rev().collect::<Vec<_>>(),
                up[1..].iter().collect::<Vec<_>>(),
                "{by:?}"
            );
        }
        _ = std::fs::remove_dir_all(dir);
    }

    /// Next and previous get the files in the same order as the file browser
    #[test]
    fn files_alone_go_in_the_same_order() {
        let dir = folder("files_alone");
        let files = ["b10.png", "b2.jpg", "A1.webp", "c.png"]
            .iter()
            .map(|f| dir.join(f))
            .collect::<Vec<_>>();
        for by in SortBy::iter() {
            for descending in [false, true] {
                let sort = FileSort { by, descending };
                let order = sort
                    .sort_files(files.clone())
                    .iter()
                    .map(|p| name(p).to_string())
                    .collect::<Vec<_>>();
                let browser = sorted(&dir, by, descending)
                    .into_iter()
                    .filter(|n| n != "sub")
                    .collect::<Vec<_>>();
                assert_eq!(order, browser, "{sort:?}");
            }
        }
        _ = std::fs::remove_dir_all(dir);
    }

    /// A file that can't be read, as a broken link, is still listed
    #[test]
    fn missing_file_is_listed() {
        let info = FileInfo::read(PathBuf::from("/oculante_test_not_there/x.png"));
        assert!(!info.is_dir);
        assert_eq!((info.size, info.modified), (0, None));
        assert_eq!(info.extension(), "png");
    }
}
