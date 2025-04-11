use crate::utils::Frame;
use anyhow::Result;
use image::guess_format;
use log::{error, info};
use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;

fn handle_client(mut stream: TcpStream, texture_sender: Sender<Frame>) -> Result<()> {
    let mut imgbuf: Vec<u8> = Vec::with_capacity(100000);
    match stream.read_to_end(&mut imgbuf) {
        Ok(_) => match image::load_from_memory(&imgbuf) {
            Ok(f) => {
                let _ = texture_sender.send(Frame::new_still(f));
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            Err(e) => {
                error!("{e}, terminating connection with {}", stream.peer_addr()?);
                stream.shutdown(Shutdown::Both)?;
            }
        },
        Err(e) => {
            error!(
                "An error {e} occurred, terminating connection with {}",
                stream.peer_addr()?
            );
            stream.shutdown(Shutdown::Both)?;
        }
    };
    Ok(())
}

pub fn recv(port: i32, texture_sender: Sender<Frame>) {
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
