use std::path::{PathBuf, Path};
use anyhow::{Result, bail, Context};
use log::debug;
use crate::utils::is_ext_compatible;

// Get sorted list of files in a folder
// TODO: Should probably return an Result<T,E> instead, but am too lazy to figure out + handle a dedicated error type here
// TODO: Cache this result, instead of doing it each time we need to fetch another file from the folder
pub fn get_image_filenames_for_directory(folder_path: &Path) -> Result<Vec<PathBuf>> {
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

/// Advance to the prev/next image
// TODO: The iterator should be cached, so we don't need to rebuild each time?
pub fn img_shift(file: &PathBuf, inc: isize) -> PathBuf {
    if let Some(parent) = file.parent() {
        if let Ok(files) = get_image_filenames_for_directory(parent) {
            if inc > 0 {
                // Next
                let mut iter = files
                    .iter()
                    .cycle()
                    .skip_while(|f| *f != file) // TODO: cache current index instead
                    .skip(1); // FIXME: What if abs(inc) > 1?

                if let Some(next) = iter.next() {
                    return next.clone();
                } else {
                    debug!(
                        "Go to next failed: i = {}, N = {}",
                        iter.count(),
                        files.len()
                    );
                }
            } else {
                // Prev
                let mut iter = files
                    .iter()
                    .rev()
                    .cycle()
                    .skip_while(|f| *f != file) // TODO: cache current index instead
                    .skip(1); // FIXME: What if abs(inc) > 1?

                if let Some(prev) = iter.next() {
                    return prev.clone();
                } else {
                    debug!(
                        "Go to prev failed: i = {}, N = {}",
                        iter.count(),
                        files.len()
                    );
                }
            }
        }
    }
    file.clone()
}
