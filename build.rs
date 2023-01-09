use std::fs::read_to_string;
use std::fs::remove_file;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

use log::error;
use log::info;

fn main() {
    info!("Build script");
    // #[cfg(windows)]
    match std::process::Command::new("convert")
        .args(vec!["res/oculante.png", "icon.ico"])
        .spawn()
    {
        Ok(_b) => info!("Converted icon"),
        Err(e) => error!("{:?}", e),
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
}
