//! Files that Finder hands to the app: "Open with", a double click on an
//! associated file, or a file dropped on the app icon.
//!
//! Finder does not pass such a file as an argument. It calls
//! `application:openFiles:` on the delegate of the app. winit registers that
//! delegate, does not pass the call on, and as of winit 0.30 does not accept a
//! delegate of ours either. So the delegate winit made is turned into a
//! subclass of itself at runtime, one that has the method. Neovide does the
//! same on the same winit version.
//!
//! This has to be set up after the event loop was created, which is when winit
//! registers its delegate, and before the event loop runs: the file the app is
//! started with arrives while the app launches.

use std::path::PathBuf;
use std::sync::Mutex;

use log::{debug, error};
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSUserDefaults, ns_string};

/// Files Finder asked for that the app has not picked up yet
static OPENED_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The files Finder asked to open since the last call
pub fn take_opened_files() -> Vec<PathBuf> {
    match OPENED_FILES.lock() {
        Ok(mut files) => std::mem::take(&mut *files),
        Err(_) => Vec::new(),
    }
}

/// `application:openFiles:` of `NSApplicationDelegate`
unsafe extern "C-unwind" fn open_files(
    _this: &AnyObject,
    _sel: Sel,
    _sender: &AnyObject,
    filenames: &NSArray<NSString>,
) {
    let paths = filenames
        .iter()
        .map(|name| PathBuf::from(name.to_string()))
        .collect::<Vec<_>>();
    debug!("Asked to open {paths:?}");
    if let Ok(mut files) = OPENED_FILES.lock() {
        files.extend(paths);
    }
    // The app looks for new files when it draws a frame
    crate::utils::request_repaint();
}

/// Make the app receive the files Finder wants it to open. Call this after the
/// event loop was created and before it runs.
pub fn register_open_files_handler() {
    let Some(mtm) = MainThreadMarker::new() else {
        error!("Files from Finder can only be set up on the main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        error!("There is no application delegate, files from Finder will not be opened");
        return;
    };

    unsafe {
        let delegate: &AnyObject = delegate.as_ref();
        let class: &AnyClass = delegate.class();
        let Some(mut subclass) = ClassBuilder::new(c"OculanteApplicationDelegate", class) else {
            error!(
                "Could not extend the application delegate, files from Finder will not be opened"
            );
            return;
        };
        subclass.add_method(
            sel!(application:openFiles:),
            open_files as unsafe extern "C-unwind" fn(_, _, _, _),
        );
        // The subclass adds a method and no data, so the object can change over to it
        AnyObject::set_class(delegate, subclass.register());
    }

    // Otherwise AppKit takes the arguments on the command line for files to open
    // as well, and they would arrive twice.
    let keys = &[ns_string!("NSTreatUnknownArgumentsAsOpen")];
    let objects = &[ns_string!("NO") as &AnyObject];
    let defaults = NSDictionary::from_slices(keys, objects);
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(&defaults);
    }
}
