use self_update::cargo_crate_version;
use std::thread;

fn gh_update() -> Result<(), Box<dyn std::error::Error>> {
    

    
    
    let target = "";
    #[cfg(target_os = "linux")]
    let target = "_linux";
    #[cfg(target_os = "macos")]
    let target = "_mac";


    let status = self_update::backends::github::Update::configure()
        .repo_owner("woelper")
        .repo_name("oculante")
        .bin_name("oculante")
        .target(target)
        .current_version(cargo_crate_version!())
        .no_confirm(true)
        .build()?
        .update()?;
    println!("Update status: `{}`!", status.version());
    Ok(())
}



pub fn update() {
    
    thread::spawn(move || {

        match gh_update() {
            Ok(_) => {dbg!("update");},
            Err(e) => {dbg!(e);},

        }
    
    });
    
    

}
