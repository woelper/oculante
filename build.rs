#[cfg(windows)]
extern crate windres;
#[cfg(windows)]
use windres::Build;

fn main() {
    // #[cfg(windows)]
    match std::process::Command::new("convert")
        .args(vec!["res/logo.png", "icon.ico"])
        .spawn()
    {
        Ok(_b) => println!("Converted icon"),
        Err(e) => println!("{:?}", e),
    }

    // #[cfg(windows)]
    // match Build::new().compile("winres.rc") {
    //     Ok(_b) => println!("Made icon ressource file"),
    //     Err(e) => println!("{:?}", e)
    // }
}
