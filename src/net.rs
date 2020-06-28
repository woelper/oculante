use std::io::Read;
use std::convert::TryInto;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;

fn handle_client(
    mut stream: TcpStream,
    texture_sender: Sender<image::RgbaImage>,
) {
    let mut data = [0 as u8; 100000]; // using 50 byte buffer
    let mut imgbuf: Vec<u8> = vec![];
    while match stream.read(&mut data) {
        Ok(size) => {
            // echo everything!
            // stream.write(&data[0..size]).unwrap();
            let x: Vec<u8> = data[0..size].try_into().unwrap();
            imgbuf.extend(x);
            // println!("{}", size);

            match image::load_from_memory(imgbuf.as_ref()) {
                Ok(i) => {
                    // println!("got image");
                    imgbuf.clear();
                    let _ = texture_sender.send(i.to_rgba());
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    // let _ = state_sender.send(String::from("ANIM_FRAME")).unwrap();
                    false
                }
                Err(_) => true,
            }
        }
        Err(_) => {
            println!(
                "An error occurred, terminating connection with {}",
                stream.peer_addr().unwrap()
            );
            stream.shutdown(Shutdown::Both).unwrap();
            false
        }
    } {}

}

pub fn recv(port: i32, texture_sender: Sender<image::RgbaImage>) {
    thread::spawn(move || {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();
        // accept connections and process them, spawning a new thread for each one
        println!("Server listening on port {}", port);
        // let mut stamp = std::time::Instant::now();

        for stream in listener.incoming() {
            
            match stream {
                Ok(stream) => {
                    // println!("New connection: {}", stream.peer_addr().unwrap());
                    let t_s = texture_sender.clone();
                    thread::spawn(move || {
                        // connection succeeded
                        handle_client(stream, t_s)
                    });
                    // stamp = std::time::Instant::now();

                }
                Err(e) => {
                    println!("Filed connection: {}", e);
                }
            }

            // let diff = std::time::Instant::now().checked_duration_since(stamp);

            // dbg!(&diff);
            // let _ = state_sender.send(String::from("done")).unwrap();
        }
        // dbg!("yo");


        // close the socket server
        drop(listener);
    });
}