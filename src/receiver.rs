#![allow(unused)]

mod reader;
mod encapsuler;
use crate::reader::config_reader::{Config,read_config};
use crate::encapsuler::encapsuler::{BenchPayload, init_ipv4_packet};
use crate::encapsuler::encapsuler::PayloadType::*;

use serde::{Serialize, Deserialize};
use bincode::*;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::exit;
use std::time::{Instant, Duration, SystemTime};

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};

use pnet::packet::Packet;
use pnet::packet::ip::{IpNextHeaderProtocols, IpNextHeaderProtocol};
use pnet::packet::ipv4::{Ipv4Packet, Ipv4, MutableIpv4Packet, checksum};
use pnet::transport::{transport_channel, TransportReceiver, ipv4_packet_iter};
use pnet::transport::TransportChannelType::Layer3;

fn main() {
    let config : Config = read_config("./config.toml");
    let dist_addr: Ipv4Addr = match config.num_dist{
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {panic!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_dist)},
    };
    let local_addr: Ipv4Addr = match config.num_local{
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {panic!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_local)},
    };
    

    let (mut tx, mut rx) = transport_channel(4096, Layer3(IpNextHeaderProtocol::new(254))).expect("Error while creating transport channel");
    let mut rcv_iterator = ipv4_packet_iter(&mut rx);
    let mut total_packets: u64 = 0;
    let mut partial_packets: u64 = 0;
    let mut call_ack = false;
    let mut terminate = false;
    let mut packet_map: BTreeMap<u64, (SystemTime)> = BTreeMap::new();
    while !terminate{
        match rcv_iterator.next() {
            Ok((packet,source)) => {
                if source == dist_addr{
                    let payload: BenchPayload = bincode::deserialize(packet.payload()).unwrap();
                    println!("received with seq_number: {}", payload.seq);

                    match payload.payload_type{
                        0 => { // SEQUENCE PAYLOAD
                            if !call_ack{ //Si un précédent call n'a pas encore été acquitté
                                call_ack = true;
                                partial_packets = 0;
                            }
                            total_packets += 1;
                            partial_packets += 1;
                            packet_map.insert(payload.seq, payload.time);
                        },

                        1 => {/* TODO CLOCK TO IMPLEMENT */},

                        2 => { // UPDATECALL
                            call_ack = false;
                            let mut packet_buffer = [0u8; 1024];
                            let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet
                            let mut packet_payload = BenchPayload::new(UpdateAnswer as u8); //new payload type 3 -> type UpdateAnswer  
                            packet_payload.count = partial_packets.clone();
                            let serialized_payload = bincode::serialize(&packet_payload).unwrap();
                            packet.set_payload(&serialized_payload.as_slice());
                            match tx.send_to(packet, IpAddr::V4(dist_addr)){
                                Ok(_)=>{println!("sent partial packet count: {}", partial_packets)}
                                Err(e)=>{println!("error while sending partial packet count : {}", e)}
                            }
                        },

                        3 => {println!("Error: received a UpdateAnswer packet type. The receiver is not supposed to receive this kind of packet")},
                        
                        4 => { // FINISHCALL
                            call_ack = false;
                            let mut packet_buffer = [0u8; 1024];
                            while !call_ack{
                                let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet
                                let mut packet_payload = BenchPayload::new(FinishAnswer as u8); //new payload type 5 -> type FinishAnswer  
                                packet_payload.count = total_packets.clone();
                                let serialized_payload = bincode::serialize(&packet_payload).unwrap();
                                packet.set_payload(&serialized_payload.as_slice());
                                match tx.send_to(packet, IpAddr::V4(dist_addr)){
                                    Ok(_)=>{println!("sent total packet count: {}", total_packets)}
                                    Err(e)=>{println!("error while sending partial packet count : {}", e)}
                                }
                                match rcv_iterator.next(){ //NEED A FINAL ACK before leaving
                                    Ok((ack_packet, source)) => {
                                        if source == dist_addr{
                                            let ack_payload : BenchPayload = bincode::deserialize(ack_packet.payload()).unwrap();
                                            if ack_payload.step == 1 && ack_payload.payload_type == (FinishAnswer as u8){
                                                call_ack = true;
                                                terminate = true;
                                            }
                                        }
                                    }
                                    Err(e) => {println!("Error while iterating on receiver in finish ack: {}", e)}
                                }
                            }
                        },

                        5 => {println!("Error: received a FinishAnswer packet type. The receiver is not supposed to receive this kind of packet")},

                        x => {panic!("Critical Error: received unknown packet payload type. can't process. BenchPayload type id : {}", x)},
                    }
                }
            }
            Err(e) => {println!("Error while iterating on receiver on main receive: {}", e)}
        }
    }

    //TODO when terminated !!!

}


// fn handle_connection(mut stream: TcpStream) {
//     let mut received_packets: u64 = 0;
//     let mut partial_packets: u64 = 0;
//     let peer_addr = stream.peer_addr().unwrap().to_string();
//     println!("Connection started by this remote address: {}", peer_addr);
//     let mut buf: [u8; 1448] = [0; 1448];


    


//     loop{
//         let partial_time: Instant = Instant::now();

//         match stream.read(&mut buf){
//             Err(_) => {}
//             Ok(bytes_read) => {
//                 received_packets += 1;
//                 partial_packets += 1;
//                 if bytes_read == 0 {
//                     println!("Connection closed with this address: {}", peer_addr);
//                     break;
//                 }
//                 let copy = buf.clone();
//                 let received_data = String::from_utf8_lossy(&copy);
                
//                 if received_data.trim_matches('\0').starts_with("finishcall") && partial_packets != 1 && partial_time.elapsed()>Duration::from_secs(1){
//                     println!("finish call received, sending total count: {}", received_packets);
//                     stream.write(received_packets.to_string().as_bytes()).expect("Error while sending final count of received packets");
//                     stream.flush().unwrap();
//                     break;
//                 }
//                 if received_data.trim_matches('\0').starts_with("updatecall") && partial_packets != 1 && partial_time.elapsed()>Duration::from_secs(1){
//                     println!("update call received, sending count: {}", partial_packets);
//                     stream.write(partial_packets.to_string().as_bytes()).expect("Error while sending final count of received packets");
//                     stream.flush().unwrap();
//                     partial_packets = 0;
//                     buf = [0; 1448];
//                 }}
//         }
        
//     }
// }