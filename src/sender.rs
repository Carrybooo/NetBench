#![allow(unused)]

mod reader;
mod encapsuler;
use crate::reader::config_reader::{Config,read_config};
use crate::encapsuler::encapsuler::{PayloadType::*, BenchPayload, init_ipv4_packet, purge_receiver};

use serde::{Serialize, Deserialize};
use bincode;

use arrayvec::ArrayVec;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::exit;
use std::time::{Instant, Duration, SystemTime};

use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, Ordering, AtomicU16}};

use fastping_rs::{Pinger, PingResult::{Idle, Receive}};

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

use icmp_socket::{IcmpSocket, IcmpSocket4, Icmpv4Packet, Icmpv4Message};

use pnet::packet::Packet;
use pnet::packet::ip::{IpNextHeaderProtocols, IpNextHeaderProtocol};
use pnet::packet::ipv4::{Ipv4Packet, Ipv4, MutableIpv4Packet, checksum};
use pnet::transport::{transport_channel, TransportReceiver, Ipv4TransportChannelIterator, ipv4_packet_iter};
use pnet::transport::TransportChannelType::Layer3;


fn main() {
    
    //read config and retrieve data
    let config : Config = read_config("./config.toml");
    let dist_addr: Ipv4Addr = match config.num_dist{
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_dist); exit(1)},
    };
    let local_addr: Ipv4Addr = match config.num_local{
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error :\nUnrecognized PC number :\"{}\", unable to continue.", config.num_local); exit(1)},
    };

    //init atomic variables, which are used to access data accross threads
    let atomic_runner:Arc<AtomicBool> = Arc::new(AtomicBool::new(true));//runner
    let run_sync:Arc<AtomicBool> = atomic_runner.clone();//clones for each thread
    let run_main:Arc<AtomicBool> = atomic_runner.clone();
    let run_ping:Arc<AtomicBool> = atomic_runner.clone();
    let run_route:Arc<AtomicBool> = atomic_runner.clone();
    let atomic_print_counter:Arc<AtomicU16> = Arc::new(AtomicU16::new(0));//print-sync
    let print_count_sync:Arc<AtomicU16> = atomic_print_counter.clone();//clones for each thread
    let print_count_tcp:Arc<AtomicU16> = atomic_print_counter.clone();
    let print_count_ping:Arc<AtomicU16> = atomic_print_counter.clone();
    let print_count_route:Arc<AtomicU16> = atomic_print_counter.clone();
    
    //Launch the threads
    let sync: thread::JoinHandle<()> = thread::Builder::new().name("sync_thread".to_string()).spawn(move || {
        sync(run_sync, print_count_sync);
    }).unwrap();

    let main_thread: thread::JoinHandle<()> = thread::Builder::new().name("main_thread".to_string()).spawn(move || {
        main_thread(local_addr, dist_addr, config.tcp_port, run_main, print_count_tcp);
    }).unwrap();

    let icmp_ping: thread::JoinHandle<()> = thread::Builder::new().name("ICMP_ping_thread".to_string()).spawn(move || {
        icmp_ping(dist_addr, run_ping, print_count_ping);
    }).unwrap();

    let icmp_route: thread::JoinHandle<()> = thread::Builder::new().name("ICMP_route_thread".to_string()).spawn(move || {
        icmp_route(dist_addr/*"8.8.8.8".parse().unwrap()*/, local_addr, run_route, print_count_route);
    }).unwrap();

    //create a handler for CTRL+C to make it exit the program safely and print the total data
    ctrlc::set_handler(move || {
        atomic_runner.store(false, Ordering::SeqCst);
        println!("\n\n\n=============================EXITING=============================");
        thread::sleep(Duration::from_millis(500));
    }).expect("Error setting Ctrl-C handler");
    
    //wait for every thread to finish
    sync.join().expect("Erreur lors de la fermeture du thread sync_thread");
    main_thread.join().expect("Erreur lors de la fermeture du thread main_thread");
    icmp_ping.join().expect("Erreur lors de la fermeture du thread ICMP_ping_thread");
    icmp_route.join().expect("Erreur lors de la fermeture du thread ICMP_route_thread");
}


//********************************************************************************************************************************
// Function used to sychronise all syncs to have them print periodicly stats in the same time
fn sync(run_print: Arc<AtomicBool>, print_count_sync: Arc<AtomicU16>){
    let mut time: Instant = Instant::now();
    while run_print.load(Ordering::SeqCst) {//while the atomic runner bool is true, trigger a print every 3sec (starting by tcp_thread)
        if time.elapsed().as_millis()>3000 && print_count_sync.load(Ordering::SeqCst)==0 {
            println!("\n\n\n\n");
            print_count_sync.store(1, Ordering::SeqCst);
            time = Instant::now();
        }
    }
    if print_count_sync.load(Ordering::SeqCst) == 0 {//when the atomic runner bool switches to false, trigger the final print
        print_count_sync.store(11, Ordering::SeqCst);
    }
}




//********************************************************************************************************************************
// Fonction main thread --- used to measure average throughput and packet delivery/loss ratio.
fn main_thread(local_addr: Ipv4Addr, dist_addr: Ipv4Addr, port: u16, run_main: Arc<AtomicBool>, print_count_tcp: Arc<AtomicU16>){
    
    let mut total_packets: i128 = 0;
    let mut partial_total_packets: i128 = 0;
    let mut final_receiver_count: i128 = 0;
    let mut partial_receiver_count: i128 = 0;
    let start: Instant = Instant::now();
    let mut partial_start: Instant = Instant::now();

    let mut packet_map: BTreeMap<u64, (SystemTime)> = BTreeMap::new();

    let (mut tx, mut rx) = transport_channel(4096, Layer3(IpNextHeaderProtocol::new(254))).expect("Error while creating transport channel");
    let mut rcv_iterator = ipv4_packet_iter(&mut rx);

    let mut sequence_number = 0u64;
    let mut packet_buffer = [0u8; 1024];
    
    while run_main.load(Ordering::SeqCst) { //MAIN LOOP OF THE THREAD running when the atomic runner bool is true

        thread::sleep(Duration::from_millis(100));
        let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet 
        let mut payload = BenchPayload::new(Sequence as u8); //new payload type 0 -> type Sequence (sets time automatically)
        payload.seq = sequence_number.clone(); //set sequence number field
        let serialized_payload = bincode::serialize(&payload).unwrap();
        packet.set_payload(&serialized_payload.as_slice());

        match tx.send_to(packet, IpAddr::V4(dist_addr)){
            Ok(_)=>{
                total_packets += 1; partial_total_packets += 1; sequence_number += 1; //increment all variables
                packet_map.insert(payload.seq, payload.time); //insert the sent packet to the binaryTree map
            }
            Err(e)=>{println!("Error while sending Sequence packet: {}", e)}
        }
        
        if print_count_tcp.load(Ordering::SeqCst)==1 {           // PERIODIC STATS PRINT
            ///UPDATE BLOC  
            let mut partial_count_received = false;
            while !partial_count_received { //Loop until count is received
                let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet 
                let mut payload = BenchPayload::new(UpdateCall as u8); //new payload type 2 -> type Updatecall
                let serialized_payload = bincode::serialize(&payload).unwrap();
                packet.set_payload(&serialized_payload.as_slice());
        
                match tx.send_to(packet, IpAddr::V4(dist_addr)){// Send UpdateCall
                    Ok(_)=>{
                        match rcv_iterator.next_with_timeout(Duration::from_secs(3)) {
                            Ok(Some((rcv_packet,source))) => {
                                if source == dist_addr{
                                    let rcv_payload : BenchPayload = bincode::deserialize(rcv_packet.payload()).unwrap();
                                    if rcv_payload.payload_type == (UpdateAnswer as u8){
                                        let partial_receiver_count = rcv_payload.count as i128;
                                        partial_count_received = true;
                                    }
                                }
                            }
                            Ok(None) => {println!("Timeout reached while iterating on receiver for UpdateCall answer")}
                            Err(e) => {println!("Error while iterating on receiver for UpdateCall answer: {}", e)}
                        }
                    }
                    Err(e)=>{println!("Error while sending UpdateCall packet: {}", e)}
                }
            }//COUNT RECEIVED
            purge_receiver(&mut rcv_iterator);

        /// PRINTING BLOC
            let partial_time: u128 = partial_start.elapsed().as_millis();
            let partial_speed: f64 = (partial_total_packets as f64 * 1448f64 / 1000f64 / (partial_time as f64/1000f64)).round();
            let partial_delivered_count: i128 = partial_total_packets-(partial_total_packets-partial_receiver_count);
            let partial_delivery_ratio: f64 = ((partial_delivered_count as f64 / partial_total_packets as f64)*100.0).round();
            println!( //PARTIAL PRINT
                "Partial average speed : {}Ko/s\
                \nPartial packet delivery ratio : {}% ({} delivered / {} total)", 
                partial_speed, 
                partial_delivery_ratio, 
                partial_delivered_count, 
                partial_total_packets
            );
            partial_total_packets = 0;
            partial_start = Instant::now();
            print_count_tcp.store(2, Ordering::SeqCst); //trigger the print of the next thread (icmp_ping)
        }
    }

    while print_count_tcp.load(Ordering::SeqCst)!=11 {    //waiting for LAST PRINT BEFORE CLOSING, triggered by sync_thread
        thread::sleep(Duration::from_millis(100));
    }

    let mut final_count_received = false;
    while !final_count_received { //Loop until final count is received
        let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet 
        let mut payload = BenchPayload::new(FinishCall as u8); //new payload type 4 -> type FinishCall
        payload.step = 0; //FIRST STEP OF THE FINISH CALL
        let serialized_payload = bincode::serialize(&payload).unwrap();
        packet.set_payload(&serialized_payload.as_slice());

        match tx.send_to(packet, IpAddr::V4(dist_addr)){// Send FinishCall
            Ok(_)=>{
                match rcv_iterator.next_with_timeout(Duration::from_secs(3)) {
                    Ok(Some((rcv_packet,source))) => {
                        if source == dist_addr{
                            let rcv_payload : BenchPayload = bincode::deserialize(rcv_packet.payload()).unwrap();
                            if rcv_payload.payload_type == (UpdateAnswer as u8){
                                let final_receiver_count = rcv_payload.count as i128;
                            }
                        }
                    }
                    Ok(None) => {println!("Timeout reached while iterating on receiver for FinishCall answer")}
                    Err(e) => {println!("Error while iterating on receiver for UpdateCall answer: {}", e)}
                }
            }
            Err(e)=>{println!("Error while sending UpdateCall packet: {}", e)}
        }
    }//COUNT RECEIVED

    //when the count is received, acknowledge with an other FinishCall with STEP FIELD = 1 
    //(init new packet but just update payload)
    let mut receiver_stopped = false;
    while !receiver_stopped{ //continue this loop until no response for 0.5 sec
        let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr); //initialize a new packet 
        let mut payload = BenchPayload::new(FinishCall as u8); //new payload type 4 -> type FinishCall
        payload.step = 1; //2nd step of the finish call : ack the total count reception
        let serialized_payload = bincode::serialize(&payload).unwrap();
        packet.set_payload(&serialized_payload.as_slice());
        match tx.send_to(packet, IpAddr::V4(dist_addr)){
            Ok(_)=>{}
            Err(e)=>{println!("Error while sending UpdateCall acknoledgement: {}", e)}
        }
        let mut receiver_stopped = false;
        match rcv_iterator.next_with_timeout(Duration::from_millis(500)) {
            Err(e) => {println!("Error while purging the iterator after the finish call: {e}")},
            Ok(None) => {receiver_stopped=true},
            Ok(_) => {}
        }
        
    }

    //PRINT THE BINARY RESULTING TREE
    for i in 0..packet_map.len(){
        if let Some((key, value)) = packet_map.pop_first(){
            println!("seq: {}, timestamp: {:?}", key, value);
        }
    } 

    let total_time: u128 = start.elapsed().as_millis();
    let total_speed: f64 = (total_packets as f64 * 1448f64 / 1000f64 / (total_time as f64/1000f64)).round();
    let total_delivered_count: i128 = total_packets-(total_packets-final_receiver_count);
    let total_delivery_ratio: f64 = ((total_delivered_count as f64 / total_packets as f64)*100.0).round();
    println!( //LAST PRINT
        "Partial average speed : {}Ko/s\
        \nPartial packet delivery ratio : {}% ({} delivered / {} total)", 
        total_speed, 
        total_delivery_ratio, 
        total_delivered_count, 
        total_packets
    );
    print_count_tcp.store(12, Ordering::SeqCst); //trigger the last print of the next thread (icmp_ping)
}


//********************************************************************************************************************************
// Fonction ICMP ping, used to measure average ping to a distant address.
fn icmp_ping(dist_addr: Ipv4Addr, run_ping: Arc<AtomicBool>, print_count_ping: Arc<AtomicU16>){
    let mut average_rtt: i128 = 0;
    let mut partial_average_rtt: i128 = 0;
    let mut ping_number: i128 = 0;
    let mut partial_ping_number: i128 = 0;

    let (pinger, results) = match Pinger::new(Some(100), Some(64)){ //init a pinger that will ping every 200ms a 64bytes packet
        Ok((pinger, results)) => (pinger, results),
        Err(e) => panic!("Error creating pinger: {}", e), 
    };

    pinger.add_ipaddr(dist_addr.to_string().as_str()); 

    pinger.run_pinger(); //launch the pinger

    while run_ping.load(Ordering::SeqCst) { // MAIN LOOP OF THE FCT running when atomic runner is true
        match results.recv_timeout(Duration::from_millis(400)) {
            Ok(result) => match result {
                Idle { addr: _ } => {
                    //println!("Ping out of time on: {}.", addr);
                }
                Receive { addr:_, rtt } => { //Compute the average (and partial) data at each new ping
                    average_rtt = ((ping_number*average_rtt)+rtt.as_millis()as i128)/(ping_number+1);
                    partial_average_rtt = ((partial_ping_number*partial_average_rtt)+rtt.as_millis()as i128)/(partial_ping_number+1);
                    ping_number += 1;
                    partial_ping_number += 1;
                }
            },
            Err(_) => panic!("Worker threads disconnected before the solution was found!"),
        }
        
        if print_count_ping.load(Ordering::SeqCst)==2 {      // PERIODIC STAT PRINT triggered after the main_thread periodic print
            println!("Partial average RTT: {}ms on {} pings", partial_average_rtt, partial_ping_number);
            partial_average_rtt=0;
            partial_ping_number=0;
            print_count_ping.store(3, Ordering::SeqCst);
        }    
    }
    while print_count_ping.load(Ordering::SeqCst)!=12 {    //waiting for LAST PRINT BEFORE CLOSING
        thread::sleep(Duration::from_millis(100));
    }
    println!("Total average RTT: {}ms on {} pings", average_rtt, ping_number);
    print_count_ping.store(13, Ordering::SeqCst);
}




//********************************************************************************************************************************
// Fonction ICMP route, used to discover the current route to a distant address.
fn icmp_route(dest_addr: Ipv4Addr, local_addr: Ipv4Addr, run_route: Arc<AtomicBool>, print_count_route: Arc<AtomicU16>) {
    let mut sequence_counter: u16 = 0;
    let mut ttl_counter: u32 = 0;
    let mut src_ip: IpAddr= "0.0.0.0".parse().unwrap();
    let mut addr_vec: Vec<(u32, IpAddr)> = Vec::new();
    let mut final_vec: Vec<(u32, IpAddr)> = Vec::new();
    let mut breaker = false;
    
    while run_route.load(Ordering::SeqCst){ // MAIN LOOP OF THE FCT running when atomic runner is true
        while src_ip != IpAddr::V4(dest_addr){ // loop to find the route (which is when the ICMP answer comes from the target address)
            ttl_counter += 1;
            sequence_counter += 1;

            let packetmsg = Icmpv4Message::Echo { identifier: 1, sequence: sequence_counter, payload: vec![] };
            let packet = Icmpv4Packet { typ: 8, code: 0, checksum: 0, message: packetmsg}; // Build packet type 8 (echo request)
            let mut icmp_socket = IcmpSocket4::try_from(local_addr).unwrap();
            icmp_socket.set_max_hops(ttl_counter);
            icmp_socket.set_timeout(Some(Duration::from_millis(300)));
 
            icmp_socket.send_to(dest_addr, packet).expect("Error while sending echo request");//sending echo request

            match icmp_socket.rcv_from() { //listening for answer
                Ok((_packet, src)) => {
                    let sender_address = src.as_socket_ipv4().unwrap();//getting the adress from the answer
                    src_ip = IpAddr::V4(*sender_address.ip()); //extracting ip
                    addr_vec.push((ttl_counter, src_ip)); //pushing in stockage vector
                }
                Err(_) => {break}//break out of this loop if it fails during route establishment
            }
        }

        while print_count_route.load(Ordering::SeqCst)!=3 {  // PERIODIC STAT PRINT WAITING triggered after the icmp_ping periodic print
            thread::sleep(Duration::from_millis(200));
            if !run_route.load(Ordering::SeqCst) {breaker=true; break}; //set breaker to true to break out of main loop and avoid double-prints
        }
        if breaker {break};
        src_ip= "0.0.0.0".parse().unwrap();
        println!("Route to Dist addr : {:?}", addr_vec);// printing stockage vector when the route has been found
        final_vec = addr_vec.clone();
        addr_vec.clear();
        ttl_counter = 0;
        print_count_route.store(0, Ordering::SeqCst);
    }
    while print_count_route.load(Ordering::SeqCst)!=13 {
        thread::sleep(Duration::from_millis(100));
    }
    println!("Route to Dist addr : {:?}", final_vec);//Last print, printing the last route used
    print_count_route.store(1000, Ordering::SeqCst);
}
