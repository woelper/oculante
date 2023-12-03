use std::env;
use std::fs::read_to_string;
use std::fs::remove_file;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

#[allow(dead_code)]
fn setup_heif() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let heif_path = format!("{out_dir}/libheif");
    println!("heif is at {heif_path}");
    #[cfg(target_os = "linux")]
    {
        use std::fs::create_dir_all;
        use std::fs::remove_dir_all;
        use std::process::Command;
        _ = remove_dir_all(&heif_path);
        // _ = remove_dir_all("libheif");
        Command::new("git")
            .args(["clone", "git@github.com:strukturag/libheif.git", &heif_path])
            .status()
            .unwrap();
        println!("Creating heif build dir");
        create_dir_all(format!("{heif_path}/build")).unwrap();
        // Command::new("git")
        // .args(["clone", "git@github.com:strukturag/libheif.git", "libheif"])
        // .status()
        // .unwrap();
        // Command::new("cd").args(["libheif"]).status().unwrap();
        // Command::new("mkdir").args(["build"]).status().unwrap();
        // Command::new("cd").args(["build"]).status().unwrap();
        println!("Running cmake / heif");

        Command::new("cmake")
            .args(["--preset=release", ".."])
            .current_dir(format!("{heif_path}/build"))
            .status()
            .unwrap();
        println!("Running make / heif");

        Command::new("make")
            .current_dir(format!("{heif_path}/build"))
            .status()
            .unwrap();
        Command::new("export")
            .args([format!("PKG_CONFIG_PATH={heif_path}/build")])
            .current_dir(format!("{heif_path}/build"))
            .status()
            .unwrap();
        println!("cargo:rustc-link-search=native={}/build", heif_path);
        // env::set_var("PKG_CONFIG_PATH", format!("{heif_path}/build"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("git")
            .args(["clone", "https://github.com/Microsoft/vcpkg.git"])
            .status()
            .unwrap();
        Command::new("cd").args(["vcpkg"]).status().unwrap();
        Command::new("./bootstrap-vcpkg.bat").status().unwrap();
        Command::new("./vcpkg")
            .args(["integrate", "install"])
            .status()
            .unwrap();
        Command::new("./vcpkg")
            .args(["install", "libheif"])
            .status()
            .unwrap();
    }
}

fn main() {
    println!("Build script");
    // #[cfg(windows)]
    match std::process::Command::new("convert")
        .args(vec!["res/oculante.png", "icon.ico"])
        .spawn()
    {
        Ok(_b) => println!("Converted icon"),
        Err(e) => eprintln!("Error converting icon {:?}. Is imagemagick installed?", e),
    }

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

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        _ = res.compile();
    }

    let shortcut_file = "shortcuts.txt";
    if Path::new(shortcut_file).is_file() {
        let mut readme: String = "".into();

        File::open("README.md")
            .unwrap()
            .read_to_string(&mut readme)
            .unwrap();

        let readme_wo_keys = readme.split("### Shortcuts:").nth(0).unwrap().to_string();

        use std::io::prelude::*;

        let shortcuts = read_to_string(shortcut_file).unwrap();
        let mouse_keys = "`mouse wheel` = zoom\n\n`left mouse`,`middle mouse` = pan\n\n`ctrl + mouse wheel` = prev/next image in folder\n\n`Right mouse` pick color from image (in paint mode)\n\n";
        let new_readme = format!("{readme_wo_keys}### Shortcuts:\n{mouse_keys}\n{shortcuts}");
        File::create("README.md")
            .unwrap()
            .write_all(new_readme.as_bytes())
            .unwrap();

        remove_file(shortcut_file).unwrap();
    }

    // #[cfg(feature = "heif")]
    // setup_heif();
    // panic!();
}
