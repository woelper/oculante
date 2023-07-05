use self_update::cargo_crate_version;
use std::{sync::mpsc::Sender, thread};

use crate::appstate::Message;

fn gh_update() -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(not(target_os = "linux"))]
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
    Ok(format!("{status:?}"))
}

pub fn update(sender: Option<Sender<Message>>) {
    thread::spawn(move || match gh_update() {
        Ok(res) => {
            _ = sender.map(|s| s.send(Message::Info(res)));
        }
        Err(e) => {
            _ = sender.map(|s| s.send(Message::Error(format!("{e:?}"))));
        }
    });
}
