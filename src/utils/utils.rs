#![allow(dead_code)]

//------------------------------Config Reader Part-------------------------------//
//*******************************************************************************//
use serde_derive::{Deserialize, Serialize};
use std::fs;
use std::process::exit;
use toml;

#[derive(Deserialize)]
pub struct Data {
    config: Config,
}

#[derive(Deserialize)]
pub struct Config {
    pub num_local: u16,
    pub num_dist: u16,
    pub ip1: String,
    pub ip2: String,
    pub ip3: String,
    pub ip4: String,
}

pub fn read_config(filename: &str) -> Config {
    let filename = filename;

    let contents = match fs::read_to_string(filename) {
        Ok(file) => file,
        Err(_) => {
            eprintln!("Could not read file `{}`", filename);
            exit(1);
        }
    };

    
    let data: Data = match toml::from_str(&contents) {
        Ok(content) => content,
        Err(_) => {
            eprintln!("Unable to load data from `{}`", filename);
            exit(1);
        }
    };

    
    println!("num_local: {}", data.config.num_local);
    println!("num_dist: {}", data.config.num_dist);
    println!("ip1: {}", data.config.ip1); 
    println!("ip2: {}", data.config.ip2); 
    println!("ip3: {}", data.config.ip3); 
    println!("ip4: {}", data.config.ip4); 


    return Config{
        num_local: data.config.num_local,
        num_dist: data.config.num_dist,
        ip1: data.config.ip1,
        ip2: data.config.ip2,
        ip3: data.config.ip3,
        ip4: data.config.ip4,
    };
}


//----------------------Custom packet structs and builder------------------------//
//*******************************************************************************//
use pnet::packet::{ipv4::MutableIpv4Packet, ip::IpNextHeaderProtocol};
use pnet::transport::Ipv4TransportChannelIterator;
use std::collections::BTreeMap;
use std::error::Error;
use std::time::{SystemTime, Duration};
use std::net::Ipv4Addr;
use chrono::offset::Local;
use csv;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchPayload {
    pub payload_type: u8,
    pub seq: u64,
    pub step: u8,
    pub data: u64,
    pub time: SystemTime,
}

impl BenchPayload {
    pub fn new(payload_type: u8) -> BenchPayload {
        return BenchPayload {
            payload_type: payload_type,
            seq: 0,
            step: 0,
            data: 0,
            time: SystemTime::now(),
        };
    }
}

#[repr(u8)]
pub enum PayloadType {
    Sequence = 0,       //Sequence --- Numbered packet.
    Clock = 1,          //Clock --- used for synchro (need GPS module for that)
    UpdateCall = 2,     //UpdateCall ---
    UpdateAnswer = 3,   //UpdateAnswer --- Contains partial count
    FinishCall = 4,     //FinishCall ---
    FinishAnswer = 5,   //FinishAnswer --- Contains final count
}

pub fn init_ipv4_packet(mut packet: MutableIpv4Packet, local_addr: Ipv4Addr, dist_addr: Ipv4Addr, packet_size: u16) -> MutableIpv4Packet{
    packet.set_version(4);
    packet.set_header_length(5);
    packet.set_source(local_addr);
    packet.set_destination(dist_addr);
    packet.set_ttl(64);
    packet.set_next_level_protocol(IpNextHeaderProtocol::new(254u8)); //we don't use any known protocole
    packet.set_total_length(packet_size);
    return packet;
}

///////////////////// Other common util functions /////////////////////////////////
//*******************************************************************************//

pub fn purge_receiver(rcv_iterator: &mut Ipv4TransportChannelIterator){
    let mut purged = false;
    while !purged{
        match rcv_iterator.next_with_timeout(Duration::from_millis(100)) {
            Err(e) => {println!("Error while purging the receive_iterator {e}")},
            Ok(None) => {purged=true},
            Ok(_) => {},

        }
    }
}

pub fn dump_to_csv(type_of: &str, map: BTreeMap<u64, (Duration, u16)>) -> Result<String, Box<dyn Error>> {
    let path=format!("./data/{}_{}.csv",type_of,Local::now().format("%Y_%m_%d_%H:%M:%S"));
    let res = path.clone();

    let mut writer = csv::Writer::from_path(path)?;

    for (seq, tuple) in map.iter(){
        writer.write_record(&[seq.to_string(),tuple.0.as_micros().to_string(),tuple.1.to_string()])?;
    }

    writer.flush()?;

    Ok(res.to_string())
}