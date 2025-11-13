# egui-modal, a modal library for [`egui`](https://github.com/emilk/egui)
[![crates.io](https://img.shields.io/crates/v/egui-modal)](https://crates.io/crates/egui-modal)
[![docs](https://docs.rs/egui-modal/badge.svg)](https://docs.rs/egui-modal/latest/egui_modal/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/n00kii/egui-modal/blob/main/README.md)

![modal](https://raw.githubusercontent.com/n00kii/egui-modal/main/media/modal.png?token=GHSAT0AAAAAABVWXBGJBQSFC3PLQP4KKOG6YZJIDCA)

## normal usage:
```rust
/* calling every frame */

let modal = Modal::new(ctx, "my_modal");

// What goes inside the modal
modal.show(|ui| {
    // these helper functions help set the ui based on the modal's
    // set style, but they are not required and you can put whatever
    // ui you want inside [`.show()`]
    modal.title(ui, "Hello world!");
    modal.frame(ui, |ui| {
        modal.body(ui, "This is a modal.");
    });
    modal.buttons(ui, |ui| {
        // After clicking, the modal is automatically closed
        if modal.button(ui, "close").clicked() {
            println!("Hello world!")
        };
    }); 
});

if ui.button("Open the modal").clicked() {
    // Show the modal
    modal.open();
}
```
## dialog usage
![dialog](https://raw.githubusercontent.com/n00kii/egui-modal/main/media/dialog.png)

in some use cases, it may be more convenient to both open and style the modal as a dialog as a one-time action, like on the single instance of a function's return.
```rust
/* calling every frame */

let modal = Modal::new(ctx, "my_dialog");

...
...
...

// Show the dialog
modal.show_dialog();
```
elsewhere,
```rust
/* happens once */
if let Ok(data) = my_function() {
    modal.dialog()
        .with_title("my_function's result is...")
        .with_body("my_function was successful!")
        .with_icon(Icon::Success)
        .open()
}
```
