use crate::utils::is_ext_compatible;
use anyhow::{bail, Context, Result};
use log::debug;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Scrubber {
    pub index: usize,
    pub entries: Vec<PathBuf>,
    pub wrap: bool,
}

impl Scrubber {
    pub fn new(path: &Path) -> Self {
        let entries = get_image_filenames_for_directory(path).unwrap_or_default();
        let index = entries.iter().position(|p| p == path).unwrap_or_default();
        Self {
            index,
            entries,
            wrap: true,
        }
    }
    pub fn next(&mut self) -> PathBuf {
        self.index += 1;
        if self.index > self.entries.len().saturating_sub(1) {
            if self.wrap {
                self.index = 0;
            } else {
                self.index = self.entries.len().saturating_sub(1);
            }
        }
        // debug!("{:?}", self.entries.get(self.index));
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    pub fn prev(&mut self) -> PathBuf {
        if self.index == 0 {
            if self.wrap {
                self.index = self.entries.len().saturating_sub(1);
            }
        } else {
            self.index = self.index.saturating_sub(1);
        }
        // debug!("{:?}", self.entries.get(self.index));
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, index: usize) -> PathBuf {
        if index < self.entries.len() {
            self.index = index;
        }
        debug!("{:?}", self.entries.get(self.index));
        self.entries.get(self.index).cloned().unwrap_or_default()
    }

    pub fn len(&mut self) -> usize {
        self.entries.len()
    }
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
    let info = std::fs::read_dir(folder_path)?;

    // TODO: Are symlinks handled correctly?
    let mut dir_files = info
        .flat_map(|x| x)
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

    return Ok(dir_files);
}

/// Find first valid image from the directory
/// Assumes the given path is a directory and not a file
pub fn find_first_image_in_directory(folder_path: &PathBuf) -> Result<PathBuf> {
    if !folder_path.is_dir() {
        bail!("This is not a folder");
    };
    get_image_filenames_for_directory(folder_path).map(|x| {
        x.first()
            .cloned()
            .context("Folder does not have any supported images in it")
    })?
}
