use crate::utils::Frame;
use anyhow::Result;
use log::{error, info};
use std::convert::TryInto;
use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;

fn handle_client(mut stream: TcpStream, texture_sender: Sender<Frame>) -> Result<()> {
    let mut data = [0 as u8; 100000]; // using 50 byte buffer
    let mut imgbuf: Vec<u8> = vec![];
    while match stream.read(&mut data) {
        Ok(size) => {
            let x: Vec<u8> = data[0..size].try_into()?;
            imgbuf.extend(x);

            match image::load_from_memory(imgbuf.as_ref()) {
                Ok(i) => {
                    // println!("got image");
                    imgbuf.clear();
                    let _ = texture_sender.send(Frame::new_still(i.to_rgba8()));
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    false
                }
                Err(_) => true,
            }
        }
        Err(e) => {
            error!(
                "An error {e} occurred, terminating connection with {}",
                stream.peer_addr()?
            );
            stream.shutdown(Shutdown::Both)?;
            false
        }
    } {}
    Ok(())
}

pub fn recv(port: u16, texture_sender: Sender<Frame>) {
    thread::spawn(move || {
        // FIXME remove unwrap
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).unwrap();
        // accept connections and process them, spawning a new thread for each one
        info!("Server listening on port {port}");

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let t_s = texture_sender.clone();
                    thread::spawn(move || {
                        // connection succeeded
                        _ = handle_client(stream, t_s)
                    });
                }
                Err(e) => {
                    info!("Filed connection: {}", e);
                }
            }
        }
        // close the socket server
        drop(listener);
    });
}
