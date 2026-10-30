use std::env;
use std::fs::File;
use std::fs::read_to_string;
use std::fs::remove_file;
use std::io::Read;
use std::io::Write;
use std::path::Path;

fn git_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_string())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn main() {
    println!("Build script");

    // Make the current git commit available to the app, reruns whenever HEAD changes so it stays up to date.
    println!("cargo:rustc-env=GIT_HASH={}", git_hash());
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head) = read_to_string(".git/HEAD")
        && let Some(ref_path) = head.trim().strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=.git/{ref_path}");
    }

    // The plist, the PKGBUILD and the README are kept up to date from here, but
    // only in a checkout of the repository. A packaged crate is built as it is:
    // cargo refuses to publish a crate whose build script changes its sources,
    // and `cargo install` should not write into them either.
    let in_repository = Path::new(".git").exists();

    if in_repository {
        // insert version into plist
        let mut plist: String = "".into();
        File::open("res/info.plist")
            .unwrap()
            .read_to_string(&mut plist)
            .unwrap();
        File::create("Info.plist")
            .unwrap()
            .write_all(
                plist
                    .replace("VERSION", env!("CARGO_PKG_VERSION"))
                    .as_bytes(),
            )
            .unwrap();

        // insert version into AUR PKGBUILD
        let mut pkgbuild = String::new();
        File::open("res/pkgbuild")
            .unwrap()
            .read_to_string(&mut pkgbuild)
            .unwrap();
        File::create("PKGBUILD")
            .unwrap()
            .write_all(
                pkgbuild
                    .replace("$$VERSION$$", env!("CARGO_PKG_VERSION"))
                    .as_bytes(),
            )
            .unwrap();
    }

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icons/icon.ico");
        _ = res.compile();
    }

    let shortcut_file = "shortcuts.txt";
    if in_repository && Path::new(shortcut_file).is_file() {
        let mut readme: String = "".into();

        File::open("README.md")
            .unwrap()
            .read_to_string(&mut readme)
            .unwrap();

        let readme_wo_keys = readme
            .split("<summary>Default Shortcuts</summary>")
            .next()
            .unwrap()
            .to_string();

        use std::io::prelude::*;

        let shortcuts = read_to_string(shortcut_file).unwrap();
        let mouse_keys = "`mouse wheel` = zoom\n\n`left mouse`,`middle mouse` = pan\n\n`ctrl + mouse wheel` = prev/next image in folder\n\n`Right mouse` pick color from image (in paint mode)\n\n";
        let new_readme = format!(
            "{readme_wo_keys}<summary>Default Shortcuts</summary>\n\n### Shortcuts:\n{mouse_keys}\n{shortcuts}\n</details>"
        );
        File::create("README.md")
            .unwrap()
            .write_all(new_readme.as_bytes())
            .unwrap();

        remove_file(shortcut_file).unwrap();
    }
}
