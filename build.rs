use std::fs::File;
use std::io::Read;
use std::io::Write;

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
}
