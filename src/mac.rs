use fruitbasket::FruitApp;
use fruitbasket::FruitCallbackKey;
use fruitbasket::RunPeriod;
use log::info;
use std::process::Command;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

pub fn launch() -> Result<(), Box<dyn Error>> {
    info!("Starting MacOS integration");

    let file_arg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let mut app = FruitApp::new();
    let stopper = app.stopper();

    app.register_callback(
        FruitCallbackKey::Method("applicationDidFinishLaunching:"),
        Box::new(move |_event| {
            info!("Application finished launching, sending stop.");
            // Send stop when app finishes launching
            stopper.stop();
        }),
    );

    // clone file_arg to muve it into closure
    let farg = file_arg.clone();
    let stopper = app.stopper();
    app.register_callback(
        FruitCallbackKey::Method("application:openFile:"),
        Box::new(move |file| {
            let file = fruitbasket::nsstring_to_string(file);
            info!("Received {}. Stopping", file);
            let mut f = farg.lock().unwrap();
            *f = Some(file.clone());
            stopper.stop();
        }),
    );

    // Run 'forever', until the URL callback fires
    let _ = app.run(RunPeriod::Forever);

    // Now it gets real ugly: Chainload this executable and quit, passing the received image as arg
    if let Ok(oculante_exe) = std::env::current_exe() {
        match file_arg.lock().unwrap().as_ref() {
            Some(f) => {
                info!("Chainloaing {:?} with {}", oculante_exe, f);
                let _ = Command::new(oculante_exe).args(&[&f, "-c"]).spawn();
            }
            None => {
                info!("Chainloaing {:?} with -c arg", oculante_exe);
                let _ = Command::new(oculante_exe).args(&["-c"]).spawn();
            }
        }
    }

    fruitbasket::FruitApp::terminate(0);

    // This will never execute.
    Ok(())
}
