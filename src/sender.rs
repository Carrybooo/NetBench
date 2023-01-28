mod utils;
use crate::utils::utils::{{BenchPayload, init_ipv4_packet, dump_to_csv, purge_receiver, Config, read_config},PayloadType::*};

use bincode;

use std::env;
use std::collections::BTreeMap;
use std::process::exit;
use std::sync::atomic::AtomicU64;
use std::time::{Instant, Duration, SystemTime};

use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, Ordering, AtomicU16}};

use fastping_rs::{Pinger, PingResult::{Idle, Receive}};

use std::net::{IpAddr, Ipv4Addr};

use icmp_socket::{IcmpSocket, IcmpSocket4, Icmpv4Packet, Icmpv4Message};

use pnet::packet::Packet;
use pnet::packet::ip::{IpNextHeaderProtocol};
use pnet::packet::ipv4::{MutableIpv4Packet};
use pnet::transport::{transport_channel, ipv4_packet_iter};
use pnet::transport::TransportChannelType::Layer3;

fn main() {
    let args: Vec<String> = env::args().collect();//Collect given arguments

    let control_delay: u128; //10K microsecs = 0,01sec base delay per packet
    let mut packet_size = 1024u16; //size of the packets, in bytes
    let mut throughput = 100f64; //throughput in KiB/s

    if args.len() > 1{
        //println!("env args : {:?}", args);
        packet_size = args[1].parse::<u16>().expect(format!(
            "Incorrect argument value: {:?}. Expected integer, representing desired throughput, in Kio/s",args[1]
            ).as_str());
        if packet_size < 100 || packet_size > 1500 {
            panic!("Error, the packet size must be between 100 and 1500 bytes,\
            \nas this script doesn't handle fragmentation for now.");
        }
        println!("Packet size : {} Bytes", packet_size);
    }else{println!("No packet size specified, using default size: {} Bytes", packet_size);}

    if args.len() > 2{
        throughput = args[2].parse::<f64>().expect(format!(
            "Incorrect argument value: {:?}. Expected integer, representing desired throughput, in Kio/s",args[1]
            ).as_str());
            println!("Expected throughput : {}", throughput);
    }else{println!("No expected throughput specified, using default throughput : {}", throughput);}
    control_delay = throughput_calcul(throughput*1024f64, packet_size as f64); //throughput in KiB/s, packet size in bytes
    println!("control_delay will be : {} nano seconds between each sent packet", control_delay);

    
    //const PACKETSIZE: u16 = args[1];
    //read config and retrieve data
    let config : Config = read_config("./config.toml");
    let dist_addr: Ipv4Addr = match config.num_dist{
        0 => {println!("Config Error : PC number for dist_adrr is set to 0. Maybe consider using the ./config script."); exit(1)}
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error : Unrecognized PC number :\"{}\", unable to continue.", config.num_dist); exit(1)},
    };
    let local_addr: Ipv4Addr = match config.num_local{
        0 => {println!("Config Error : PC number for local_adrr is set to 0. Maybe consider using the ./config script."); exit(1)}
        1 => config.ip1.parse().unwrap(),
        2 => config.ip2.parse().unwrap(),
        3 => config.ip3.parse().unwrap(),
        4 => config.ip4.parse().unwrap(),
        _ => {println!("Config Error : Unrecognized PC number :\"{}\", unable to continue.", config.num_local); exit(1)},
    };

    //init atomic variables, which are used to access data accross threads
    let atomic_runner:Arc<AtomicBool> = Arc::new(AtomicBool::new(true));//runner
    let run_sync:Arc<AtomicBool> = atomic_runner.clone();//clones for each thread
    let run_sender:Arc<AtomicBool> = atomic_runner.clone();
    let run_compute:Arc<AtomicBool> = atomic_runner.clone();
    let run_ping:Arc<AtomicBool> = atomic_runner.clone();
    let run_route:Arc<AtomicBool> = atomic_runner.clone();
    let atomic_print_counter:Arc<AtomicU16> = Arc::new(AtomicU16::new(0));//print-sync
    let print_count_sync:Arc<AtomicU16> = atomic_print_counter.clone();//clones for each thread
    let print_count_sender:Arc<AtomicU16> = atomic_print_counter.clone();
    let print_count_compute:Arc<AtomicU16> = atomic_print_counter.clone();
    let print_count_ping:Arc<AtomicU16> = atomic_print_counter.clone();
    let print_count_route:Arc<AtomicU16> = atomic_print_counter.clone();

    let global_count:Arc<AtomicU64> = Arc::new(AtomicU64::new(0));//global packet number counter
    let global_count_sender: Arc<AtomicU64> = global_count.clone();
    let global_count_compute: Arc<AtomicU64> = global_count.clone();
    
    //Launch the threads
    let sync: thread::JoinHandle<()> = thread::Builder::new().name("sync_thread".to_string()).spawn(move || {
        sync(run_sync, print_count_sync);
    }).unwrap();

    let sender_thread: thread::JoinHandle<()> = thread::Builder::new().name("sender_thread".to_string()).spawn(move || {
        sender_thread(local_addr, dist_addr, control_delay, packet_size, run_sender, print_count_sender, global_count_sender);
    }).unwrap();

    let compute_thread: thread::JoinHandle<()> = thread::Builder::new().name("compute_thread".to_string()).spawn(move || {
        compute_thread(local_addr, dist_addr, packet_size, run_compute, print_count_compute, global_count_compute);
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
    sender_thread.join().expect("Erreur lors de la fermeture du thread send_thread");
    compute_thread.join().expect("Erreur lors de la fermeture du thread compute_thread");
    icmp_ping.join().expect("Erreur lors de la fermeture du thread ICMP_ping_thread");
    icmp_route.join().expect("Erreur lors de la fermeture du thread ICMP_route_thread");
    println!("=================================================================");
}


//********************************************************************************************************************************
// Function used to sychronise all syncs to have them print periodicly stats in the same time
fn sync(run: Arc<AtomicBool>, print_count_sync: Arc<AtomicU16>){
    let mut time: Instant = Instant::now();
    while run.load(Ordering::SeqCst) {//while the atomic runner bool is true, trigger a print every 3sec (starting by compute_thread)
        if time.elapsed().as_millis()>3000 && print_count_sync.load(Ordering::SeqCst)==0 {
            println!("\n\n\n\n");
            print_count_sync.store(1, Ordering::SeqCst);
            time = Instant::now();
        }
    }

    while print_count_sync.load(Ordering::SeqCst) != 0 {//when the atomic runner bool switches to false, trigger the final print
        //Start by sending it to the sender thread to be sure he has stopped before printing results
        thread::sleep(Duration::from_millis(10));
    }
    print_count_sync.store(100, Ordering::SeqCst);
}

//********************************************************************************************************************************
/// Fonction sender thread --- used to measure average throughput and packet delivery/loss ratio.
fn sender_thread(local_addr: Ipv4Addr, dist_addr: Ipv4Addr, expected_delay: u128, packet_size: u16, run: Arc<AtomicBool>, print_count_sender: Arc<AtomicU16>, global_count: Arc<AtomicU64>){
    let (mut tx, _) = transport_channel(4096, Layer3(IpNextHeaderProtocol::new(254))).expect("Error while creating transport channel");
    let mut total_packets = 0;
    let mut packet_example: Vec<u8> = Vec::new();
        for _ in 0..packet_size {
            packet_example.push(0); //INIT THE FUTURE WRITE BUFFER WITH FULL RANDOM VALUES (here packet size will be 1448 Bytes so 64kiB)
        }
    let mut sequence_number = 0u64;
    let mut packet_map: BTreeMap<u64, (Duration, u16)> = BTreeMap::new();
    let mut control_delay = Instant::now();
    let start_time = SystemTime::now();
    let mut payload = BenchPayload::new(Sequence as u8); //new payload type 0 -> type Sequence (sets time automatically)
    let addr = IpAddr::V4(dist_addr);

    while run.load(Ordering::SeqCst){ //Main sending loop
        while control_delay.elapsed().as_nanos() < expected_delay {}//void loop to waituntill
        control_delay=Instant::now();
        let mut packet_buffer = packet_example.clone();
        let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, packet_size); //initialize a new packet
        payload.time = SystemTime::now();
        payload.seq = sequence_number; //set sequence number field
        let serialized_payload = bincode::serialize(&payload).unwrap();
        packet.set_payload(&serialized_payload.as_slice());

        match tx.send_to(packet, addr){
            Ok(_)=>{
                total_packets += 1; sequence_number += 1; //increment counters
                global_count.store(total_packets, Ordering::SeqCst);
                packet_map.insert(payload.seq, (payload.time.duration_since(start_time).ok().unwrap(), packet_size)); //insert the sent packet to the binaryTree map
            }
            Err(e)=>{println!("Error while sending Sequence packet: {}", e)}
        }
    }//fin main while 

    while print_count_sender.load(Ordering::SeqCst) != 100 {
        thread::sleep(Duration::from_millis(100));
    }
    print_count_sender.store(11, Ordering::SeqCst);

    match dump_to_csv("sender",packet_map){
        Ok(path) => {
            println!("Results dumped to file : {}", path);
            println!("=================================================================");
        }
        Err(e) => {println!("Error while writing data to CSV file: {}", e)}
    }

    //PRINT THE BINARY RESULTING TREE
    /*
    for _ in 0..packet_map.len(){
        if let Some((key, value)) = packet_map.pop_first(){
            println!("seq: {}, timestamp: {:?}", key, value);
        }
    } 
    */
}

//********************************************************************************************************************************
/// Fonction main thread --- used to measure average throughput and packet delivery/loss ratio.
fn compute_thread(local_addr: Ipv4Addr, dist_addr: Ipv4Addr, packet_size: u16, run: Arc<AtomicBool>, print_count_compute: Arc<AtomicU16>, global_count: Arc<AtomicU64>){
    
    let mut total_packets: i128 = 0;
    let mut partial_total_packets: i128;
    let mut partial_total_marker: i128 = 0;
    let mut final_receiver_count: i128 = 0;
    let mut partial_receiver_count: i128 = 0;
    let start: Instant = Instant::now();
    let mut partial_start: Instant = Instant::now();

    let (mut tx, mut rx) = transport_channel(4096, Layer3(IpNextHeaderProtocol::new(254))).expect("Error while creating transport channel");
    let mut rcv_iterator = ipv4_packet_iter(&mut rx);

    let mut packet_example: Vec<u8> = Vec::new();
    for _ in 0..packet_size {
        packet_example.push(0); //INIT THE FUTURE WRITE BUFFER WITH FULL RANDOM VALUES (here packet size will be 1448 Bytes so 64kiB)
    }

    while run.load(Ordering::SeqCst) { //MAIN LOOP OF THE THREAD running when the atomic runner bool is true
        if print_count_compute.load(Ordering::SeqCst)==1 {           // PERIODIC STATS PRINT
            //UPDATE BLOC
            let mut partial_count_received = false;
            let timeout = Instant::now();
            let mut breaker = false;
            while !partial_count_received && !breaker { //Loop until count is received
                let mut packet_buffer = packet_example.clone();
                let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, packet_size); //initialize a new packet 
                let payload = BenchPayload::new(UpdateCall as u8); //new payload type 2 -> type Updatecall
                let serialized_payload = bincode::serialize(&payload).unwrap();
                packet.set_payload(&serialized_payload.as_slice());
                match tx.send_to(packet, IpAddr::V4(dist_addr)){// Send UpdateCall
                    Ok(_)=>{
                        match rcv_iterator.next_with_timeout(Duration::from_millis(100)) {
                            Ok(Some((rcv_packet,source))) => {
                                total_packets = global_count.load(Ordering::SeqCst) as i128;//get send count here to avoid huge shifts compared to receiver count
                                if source == dist_addr{
                                    let rcv_payload : BenchPayload = bincode::deserialize(rcv_packet.payload()).unwrap();
                                    if rcv_payload.payload_type == (UpdateAnswer as u8){
                                        partial_receiver_count = rcv_payload.data.clone() as i128; 
                                        partial_count_received = true;
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {println!("Error while iterating on receiver for UpdateCall answer: {}", e)}
                        }
                    }
                    Err(e)=>{println!("Error while sending UpdateCall packet: {}", e)}
                }
                if timeout.elapsed().as_millis()>500{breaker=true}
            }//COUNT RECEIVED
            purge_receiver(&mut rcv_iterator);

            // PRINTING BLOC
            //TOTAL packet number is retrieved at the same time the script receives the receiver's count to minimize the difference
            //but if we didn't receive the receiver's count, get the value manually
            if total_packets == 0 {total_packets = global_count.load(Ordering::SeqCst) as i128}
            partial_total_packets = total_packets-partial_total_marker;
            partial_total_marker = total_packets.clone();
            let partial_time: u128 = partial_start.elapsed().as_millis();
            let partial_speed: f64 = (partial_receiver_count as f64 * (packet_size as f64) / 1024f64 / (partial_time as f64/1000f64)).round();
            let partial_delivery_ratio: f64 = ((partial_receiver_count as f64 / partial_total_packets as f64)*100.0).round();
            let partial_amount: i128 = partial_receiver_count * packet_size as i128 / 1024;
            println!( //PARTIAL PRINT
                "Partial amount transfered : {}KiB\
                \nPartial average speed : {}KiB/s\
                \nPartial packet delivery ratio : {}% ({} delivered / {} total)", 
                partial_amount,
                partial_speed,
                partial_delivery_ratio, 
                partial_receiver_count, 
                partial_total_packets
            );
            partial_start = Instant::now();
            print_count_compute.store(2, Ordering::SeqCst); //trigger the print of the next thread (icmp_ping)
        }
    }//MAIN LOOP END

    while print_count_compute.load(Ordering::SeqCst)!=11 {    //waiting for LAST PRINT BEFORE CLOSING, triggered by sync_thread
        thread::sleep(Duration::from_millis(10));
    }

    let mut final_count_received = false;
    let timeout = Instant::now();
    let mut breaker = false;
    while !final_count_received && !breaker { //Loop until final count is received
        let mut packet_buffer = packet_example.clone();
        let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, packet_size); //initialize a new packet 
        let mut payload = BenchPayload::new(FinishCall as u8); //new payload type 4 -> type FinishCall
        payload.step = 0; //FIRST STEP OF THE FINISH CALL
        let serialized_payload = bincode::serialize(&payload).unwrap();
        packet.set_payload(&serialized_payload.as_slice());
        match tx.send_to(packet, IpAddr::V4(dist_addr)){// Send FinishCall
            Ok(_)=>{
                match rcv_iterator.next_with_timeout(Duration::from_millis(100)) {
                    Ok(Some((rcv_packet,source))) => {
                        total_packets = global_count.load(Ordering::SeqCst) as i128;//getting total packets before receiving it to avoid huge shifts
                        if source == dist_addr{
                            let rcv_payload : BenchPayload = bincode::deserialize(rcv_packet.payload()).unwrap();
                            if rcv_payload.payload_type == (FinishAnswer as u8){
                                final_receiver_count = rcv_payload.data.clone() as i128;
                                final_count_received = true;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {println!("Error while waiting for FinishCall answer: {}", e)}
                }
            }
            Err(e)=>{println!("Error while sending UpdateCall packet: {}", e)}
        }
        if timeout.elapsed().as_millis() > 2000{
            println!("Impossible to retrieve final packet count. Termitating without it");
            breaker=true
        }
    }//COUNT RECEIVED

    if !breaker{
        //when the count is received, acknowledge with an other FinishCall with STEP FIELD = 1 
        //(init new packet but just update payload)
        let mut receiver_stopped = false;
        while !receiver_stopped{ //continue this loop until no response for 0.5 sec
            let mut packet_buffer = packet_example.clone();
            let mut packet = init_ipv4_packet(MutableIpv4Packet::new(&mut packet_buffer).unwrap(), local_addr, dist_addr, packet_size); //initialize a new packet 
            let mut payload = BenchPayload::new(FinishCall as u8); //new payload type 4 -> type FinishCall
            payload.step = 1; //2nd step of the finish call : ack the total count reception
            let serialized_payload = bincode::serialize(&payload).unwrap();
            packet.set_payload(&serialized_payload.as_slice());
            match tx.send_to(packet, IpAddr::V4(dist_addr)){
                Ok(_)=>{}
                Err(e)=>{println!("Error while sending UpdateCall acknoledgement: {}", e)}
            }
            match rcv_iterator.next_with_timeout(Duration::from_millis(100)) {
                Err(e) => {println!("Error while purging the iterator after the finish call: {e}")},
                Ok(None) => {receiver_stopped=true},
                Ok(_) => {}
            }        
        }
    }

    //TOTAL packet number is retrieved at the same time the script receives the receiver count to minimize the difference
    //but if we didn't receive the receiver's count, get the value manually
    if total_packets == 0 {total_packets = global_count.load(Ordering::SeqCst) as i128}
    let total_time: u128 = start.elapsed().as_millis();
    let total_transmitted = final_receiver_count as u128 * packet_size as u128 / 1024 / 1024; 
    let total_speed: f64 = (final_receiver_count as f64 * (packet_size as f64) / 1024f64 / (total_time as f64/1000f64)).round();
    let total_delivery_ratio: f64 = ((final_receiver_count as f64 / total_packets as f64)*100.0).round();
    println!( //LAST PRINT
        "Benchmark lasted for : {}s, Total data transmitted : {}MiB\
        \nTotal average speed : {}KiB/s\
        \nTotal packet delivery ratio : {}% ({} delivered / {} total)",
        total_time/1000,
        total_transmitted,
        total_speed, 
        total_delivery_ratio, 
        final_receiver_count, 
        total_packets
    );
    print_count_compute.store(12, Ordering::SeqCst); //trigger the last print of the next thread (icmp_ping)
}


//********************************************************************************************************************************
/// Fonction ICMP ping, used to measure average ping to a distant address.
fn icmp_ping(dist_addr: Ipv4Addr, run: Arc<AtomicBool>, print_count_ping: Arc<AtomicU16>){
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

    while print_count_ping.load(Ordering::SeqCst)!=12 {    //waiting for LAST PRINT BEFORE CLOSING
        thread::sleep(Duration::from_millis(10));

        while run.load(Ordering::SeqCst) || print_count_ping.load(Ordering::SeqCst)==2 { // MAIN LOOP OF THE FCT running when atomic runner is true
            match results.recv_timeout(Duration::from_millis(400)) {
                Ok(result) => match result {
                    Idle { addr: _ } => {}
                    Receive { addr:_, rtt } => { //Compute the average (and partial) data at each new ping
                        average_rtt = ((ping_number*average_rtt)+rtt.as_millis()as i128)/(ping_number+1);
                        partial_average_rtt = ((partial_ping_number*partial_average_rtt)+rtt.as_millis()as i128)/(partial_ping_number+1);
                        ping_number += 1;
                        partial_ping_number += 1;
                    }
                },
                Err(_) => {}, //ping timeout -> not critical…
            }
            
            if print_count_ping.load(Ordering::SeqCst)==2 {   // PERIODIC STAT PRINT triggered after the compute_thread periodic print
                println!("Partial average RTT: {}ms on {} pings", partial_average_rtt, partial_ping_number);
                partial_average_rtt=0;
                partial_ping_number=0;
                print_count_ping.store(3, Ordering::SeqCst);
            }    
        }
    }
    println!("Total average RTT: {}ms on {} pings", average_rtt, ping_number);
    print_count_ping.store(13, Ordering::SeqCst);
}




//********************************************************************************************************************************
/// Fonction ICMP route, used to discover the current route to a distant address.
fn icmp_route(dest_addr: Ipv4Addr, local_addr: Ipv4Addr, run: Arc<AtomicBool>, print_count_route: Arc<AtomicU16>) {
    let mut sequence_counter: u16 = 0;
    let mut ttl_counter: u32 = 0;
    let mut src_ip: IpAddr= "0.0.0.0".parse().unwrap();
    let mut addr_vec: Vec<(u32, IpAddr)> = Vec::new();
    let mut final_vec: Vec<(u32, IpAddr)> = Vec::new();
    let mut breaker = false;
    
    while print_count_route.load(Ordering::SeqCst)!=13 {
        thread::sleep(Duration::from_millis(10));

        while run.load(Ordering::SeqCst) || print_count_route.load(Ordering::SeqCst)==3{ // MAIN LOOP OF THE FCT running when atomic runner is true
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
                thread::sleep(Duration::from_millis(10));
                if !run.load(Ordering::SeqCst) {breaker=true; break}; //set breaker to true to break out of main loop and avoid double-prints
            }
            if !breaker {
            src_ip= "0.0.0.0".parse().unwrap();
            println!("Route to Dist addr : {:?}\
            \n=================================================================", addr_vec);// printing stockage vector when the route has been found
            final_vec = addr_vec.clone();
            addr_vec.clear();
            ttl_counter = 0;
            };
            if print_count_route.load(Ordering::SeqCst) == 3 {
                print_count_route.store(0, Ordering::SeqCst);
            }
        }
    }
    println!("Last route to Dist addr : {:?}", final_vec);//Last print, printing the last route used
    print_count_route.store(1000, Ordering::SeqCst);
}


//-----------------Local util functions-------------------//
///compute the delay needed between 2 packets to achieve a desired throughput for a specific packet size. (all in KiB).
fn throughput_calcul(throughput: f64, size: f64) -> u128{
    ((size/throughput) * 1000000000f64) as u128 //return the needed delay for 1 packet the delay in microsecond.
}