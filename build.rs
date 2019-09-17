#[cfg(windows)]
extern crate windres;
use windres::Build;

fn main() {
    match Build::new().compile("winres.rc") {
        Ok(_b) => println!("Made icon"),
        Err(e) => println!("{:?}", e)
    }
}