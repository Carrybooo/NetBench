#![allow(dead_code)]

use pnet::packet::{ipv4::MutableIpv4Packet, ip::IpNextHeaderProtocol};
use pnet::transport::Ipv4TransportChannelIterator;
use serde_derive::{Deserialize, Serialize};
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
    pub count: u64,
    pub time: SystemTime,
    pub filler: [[u8; 32]; 29], //trying to fill with anything else than 0 but can serialyze array > 32 so it's approximative for the moment
}

impl BenchPayload {
    pub fn new(payload_type: u8) -> BenchPayload {
        return BenchPayload {
            payload_type: payload_type,
            seq: 0,
            step: 0,
            count: 0,
            time: SystemTime::now(),
            filler: [[170u8; 32]; 29],
        };
    }
}

// /* Payload types : (I'll implement a real enum if I have the time but it's awful to serialize and deserialize properly from byte array:)
// 0 : Sequence --- Numbered packet.
// 1 : Clock --- used for synchro (pas encore trouvé comment)
// 2 : UpdateCall ---
// 3 : UpdateAnswer --- Contains partial count
// 4 : FinishCall ---
// 5 : FinishAnswer --- Contains final count
// */
#[repr(u8)]
pub enum PayloadType {
    Sequence = 0,
    Clock = 1,
    UpdateCall = 2,
    UpdateAnswer = 3,
    FinishCall = 4,
    FinishAnswer = 5,
}


///////////////////////////// Other common util functions ////////////////////////////////////////////

pub fn init_ipv4_packet(mut packet: MutableIpv4Packet, local_addr: Ipv4Addr, dist_addr: Ipv4Addr) -> MutableIpv4Packet{
    packet.set_version(4);
    packet.set_header_length(5);
    packet.set_source(local_addr);
    packet.set_destination(dist_addr);
    packet.set_ttl(64);
    packet.set_next_level_protocol(IpNextHeaderProtocol::new(254u8)); //we don't use any known protocole
    packet.set_total_length(1024);
    return packet;
}

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

pub fn dump_to_csv(type_of: &str, map: BTreeMap<u64, Duration>) -> Result<String, Box<dyn Error>> {
    let path=format!("./data/{}_{}.csv",type_of,Local::now().time());
    let res = path.clone();

    let mut writer = csv::Writer::from_path(path)?;

    for (seq, time) in map.iter(){
        writer.write_record(&[seq.to_string(),time.as_micros().to_string()])?;
    }


    writer.flush()?;

    Ok(res.to_string())
}