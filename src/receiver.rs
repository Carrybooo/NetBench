mod utils;
use crate::utils::utils::{{BenchPayload, init_ipv4_packet, dump_to_csv, Config, read_config},PayloadType::*};

use std::collections::BTreeMap;
use std::process::exit;
use std::time::{SystemTime, Duration};

use std::net::{IpAddr, Ipv4Addr};

use pnet::packet::Packet;
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet::transport::{transport_channel, ipv4_packet_iter};
use pnet::transport::TransportChannelType::Layer3;

fn main() {
    let config : Config = read_config("./config.toml");
    let dist_addr: Ipv4Addr = match config.num_dist{
        0 => {println!("Config Error : PC number for dist_adrr is set to 0. Maybe consider using the ./config script."); exit(1)}
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_dist); exit(1)},
    };
    let local_addr: Ipv4Addr = match config.num_local{
        0 => {println!("Config Error : PC number for local_adrr is set to 0. Maybe consider using the ./config script."); exit(1)}
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_local); exit(1)},
    };
    

    let (mut tx, mut rx) = transport_channel(4096, Layer3(IpNextHeaderProtocol::new(254))).expect("Error while creating transport channel");
    let mut rcv_iterator = ipv4_packet_iter(&mut rx);
    let mut total_packets: u64 = 0;
    let mut partial_packets: u64 = 0;
    let mut last_rcv_seq: u64 = 0;
    let mut call_ack = false;
    let mut terminate = false;
    let mut packet_map: BTreeMap<u64, (Duration, u16)> = BTreeMap::new();
    let mut rcv_time : SystemTime;
    let mut start_time = SystemTime::now();
    while !terminate{
        match rcv_iterator.next() {
            Ok((packet,source)) => {
                if total_packets==0{start_time = SystemTime::now();}
                rcv_time = SystemTime::now();
                if source == dist_addr{
                    let payload: BenchPayload = bincode::deserialize(packet.payload()).unwrap();
                    match payload.payload_type{
                        0 => { // SEQUENCE PAYLOAD
                            if !call_ack{ //Si un précédent call n'a pas encore été acquitté
                                call_ack = true;
                                partial_packets = 0;
                            }
                            
                            total_packets += 1;
                            partial_packets += 1;
                            packet_map.insert(payload.seq, (rcv_time.duration_since(start_time).ok().unwrap(), packet.get_total_length()));

                            //Detect and print drops
                            if (last_rcv_seq + 1) < payload.seq {
                                if (last_rcv_seq + 1) == (payload.seq -1){
                                    println!("The following packet has never been received : {}", last_rcv_seq+1);
                                }else{
                                    println!("The following packets have never been received : [{}..{}]", last_rcv_seq+1, payload.seq-1);
                                }
                            };
                            last_rcv_seq = payload.seq.clone();
                        },

                        1 => {/* TODO CLOCK TO IMPLEMENT */},

                        2 => { // UPDATECALL
                            call_ack = false;
                            let mut packet_buffer = [0u8; 1024];
                            let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, 200); //initialize a new packet
                            let mut packet_payload = BenchPayload::new(UpdateAnswer as u8); //new payload type 3 -> type UpdateAnswer  
                            packet_payload.data = partial_packets.clone();
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
                                let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, 200); //initialize a new packet
                                let mut packet_payload = BenchPayload::new(FinishAnswer as u8); //new payload type 5 -> type FinishAnswer  
                                packet_payload.data = total_packets.clone();
                                let serialized_payload = bincode::serialize(&packet_payload).unwrap();
                                packet.set_payload(&serialized_payload.as_slice());
                                match tx.send_to(packet, IpAddr::V4(dist_addr)){
                                    Ok(_)=>{println!("sent total packet count: {}", total_packets)}
                                    Err(e)=>{println!("error while sending final packet count : {}", e)}
                                }
                                match rcv_iterator.next(){ //NEED A FINAL ACK before leaving
                                    Ok((ack_packet, source)) => {
                                        println!("DEBUG, FINAL ACK LOOP pckt rcvd: source {}", source);
                                        if source == dist_addr{
                                            let ack_payload : BenchPayload = bincode::deserialize(ack_packet.payload()).unwrap();
                                            println!("DEBUG, ackpayload : step :{} type:{}", payload.step, payload.payload_type);
                                            if ack_payload.step == 1 && ack_payload.payload_type == (FinishCall as u8){
                                                call_ack = true;
                                                terminate = true;
                                                println!("Last finishcall received. Leaving.");
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

    //Data print 
    /*
    for i in 0..packet_map.len(){
        if let Some((key, value)) = packet_map.pop_first(){
            println!("seq: {}, timestamp: {:?}", key, value);
        }
    }
    */
    match dump_to_csv("rcvr",packet_map){
        Ok(path) => {println!("Results dumped to file : {}", path)}
        Err(e) => {println!("Error while writing data to CSV file: {}", e)}
    }
}