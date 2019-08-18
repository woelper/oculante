//#![windows_subsystem = "windows"]

// extern crate image as img;
// use crate::img::GenericImageView;
// extern crate piston_window;
use clap;
use clap::{App, Arg, SubCommand};
use piston_window::*;
// use opengl_graphics::{ GlGraphics, OpenGL };
// use graphics::{ Context, Graphics };

fn main() {
    let font = include_bytes!("FiraSans-Regular.ttf");
    let matches = App::new("Oculante")
        .arg(
            Arg::with_name("INPUT")
                .help("Display this image")
                .required(true)
                .index(1),
        )
        .get_matches();

    let img_path = matches.value_of("INPUT").unwrap();

    
    let opengl = OpenGL::V3_2;
    let mut window: PistonWindow = WindowSettings::new("Oculante", [1000, 800])
        .exit_on_esc(true)
        .graphics_api(opengl)
        .samples(4)
        .build()
        .unwrap();

    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);
    // tx_settings.set_min(Filter::Nearest);

    match Texture::from_path(
        &mut window.create_texture_context(),
        &img_path,
        Flip::None,
        &tx_settings,
    ) {
        Ok(texture) => {
            
            window.set_lazy(true);
            let mut offset = (100.0,0.0);
            let mut scale = 1.0;
            let mut drag = false;
            //let mut events = Events::new(EventSettings::new().lazy(true));
            let mut reset = false;
            let dimensions = texture.get_size();

            let mut glyphs = Glyphs::from_bytes(font, window.create_texture_context(), TextureSettings::new()).unwrap();


  
            while let Some(e) = window.next() {
                
                if let Some(Button::Mouse(_)) = e.press_args() {drag = true;}
                if let Some(Button::Mouse(_)) = e.release_args() {drag = false;}

                if let Some(Button::Keyboard(key)) = e.press_args() {
                    if key == Key::R {reset = true;}
                    println!("Pressed keyboard key '{:?}'", key);
                };

                e.mouse_scroll(|d| {
                    if d[1] > 0.0 {
                        scale += 0.2;
                    } else {
                        scale -= 0.2;
                        if scale < 0.1 {scale = 0.1;}
                    }
                });
                e.mouse_relative(|d| {
                    if drag {
                        offset.0 += d[0];
                        offset.1 += d[1];
                    }
                });
                // e.resize(|args| {
                //     println!("Resized '{}, {}'", args.window_size[0], args.window_size[1])
                // });
    
                window.draw_2d(&e, |c, gfx, device| {
                    clear([0.2; 4], gfx);
                    if reset {
                        offset = (0.0,0.0);
                        scale = 1.0;
                        reset = false;
                    }
                    let transform = c.transform.trans(offset.0, offset.1).scale(scale, scale);

                    image(&texture, transform, gfx);
                

                    text::Text::new_color([0.8, 0.8, 0.8, 0.7], 16).draw(
                        &format!("{} {}X{}", img_path, dimensions.0, dimensions.1),
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(10.0, 20.0), gfx
                    ).unwrap();
                    glyphs.factory.encoder.flush(device);

                });


            }

 
        }
        Err(e) => println!("Could not create texture. {}", e),
    }
    
}
