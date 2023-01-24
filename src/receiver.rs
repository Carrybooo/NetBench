mod reader;
use crate::reader::config_reader::{Config,read_config};

use std::io::{Read, Write};
use std::process::exit;
use std::time::{Instant, Duration};

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};

fn main() {
    let config : Config = read_config("./config.toml");
    let local_addr: Ipv4Addr = match config.num_local{
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_local); exit(1)},
    };
    
    let local_tcp_socket: SocketAddr = SocketAddr::new(IpAddr::V4(local_addr), config.tcp_port);

    let listener: TcpListener = TcpListener::bind(local_tcp_socket).unwrap();

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                handle_connection(stream);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut received_packets: u64 = 0;
    let mut partial_packets: u64 = 0;
    let peer_addr = stream.peer_addr().unwrap().to_string();
    println!("Connection started by this remote address: {}", peer_addr);
    let mut buf: [u8; 1448] = [0; 1448];
    loop{
        let partial_time: Instant = Instant::now();
        match stream.read(&mut buf){
            Err(_) => {}
            Ok(bytes_read) => {
                received_packets += 1;
                partial_packets += 1;
                if bytes_read == 0 {
                    println!("Connection closed with this address: {}", peer_addr);
                    break;
                }
                let copy = buf.clone();
                let received_data = String::from_utf8_lossy(&copy);
                
                if received_data.trim_matches('\0').starts_with("finishcall") && partial_packets != 1 && partial_time.elapsed()>Duration::from_secs(1){
                    println!("finish call received, sending total count: {}", received_packets);
                    stream.write(received_packets.to_string().as_bytes()).expect("Error while sending final count of received packets");
                    stream.flush().unwrap();
                    break;
                }
                if received_data.trim_matches('\0').starts_with("updatecall") && partial_packets != 1 && partial_time.elapsed()>Duration::from_secs(1){
                    println!("update call received, sending count: {}", partial_packets);
                    stream.write(partial_packets.to_string().as_bytes()).expect("Error while sending final count of received packets");
                    stream.flush().unwrap();
                    partial_packets = 0;
                    buf = [0; 1448];
                }}
        }
        
    }
}