mod reader;
use crate::reader::config_reader::{Config,read_config};

use arrayvec::ArrayVec;
use std::io::{Read, Write};
use std::process::exit;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering, AtomicU16};
use std::time::{Instant, Duration};

use fastping_rs::PingResult::{Idle, Receive};
use fastping_rs::Pinger;

use icmp_socket::{IcmpSocket, IcmpSocket4, Icmpv4Packet, Icmpv4Message};

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
    let run_tcp:Arc<AtomicBool> = atomic_runner.clone();
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

    let tcp_connection: thread::JoinHandle<()> = thread::Builder::new().name("TCP_thread".to_string()).spawn(move || {
        tcp_connection(dist_addr, config.tcp_port, run_tcp, print_count_tcp);
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
        thread::sleep(Duration::from_secs(1));
    }).expect("Error setting Ctrl-C handler");
    
    //wait for every thread to finish
    sync.join().expect("Erreur lors de la fermeture du thread sync_thread");
    tcp_connection.join().expect("Erreur lors de la fermeture du thread TCP_thread");
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
// Fonction tcp connection --- used to measure average throughput and packet drop ratio. 
fn tcp_connection(dist_addr: Ipv4Addr, port: u16, run_tcp: Arc<AtomicBool>, print_count_tcp: Arc<AtomicU16>){
    //init socket and TCPstream
    let distant_socket: SocketAddr = SocketAddr::new(IpAddr::V4(dist_addr), port);
    let mut stream: TcpStream = TcpStream::connect(distant_socket).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    stream.set_write_timeout(Some(Duration::from_millis(200))).unwrap();

    //Init buffers and other variables
    let mut array_vec: ArrayVec<u8, 1448> = ArrayVec::new();
        for _ in 0..array_vec.capacity() {
            array_vec.push(rand::random()); //INIT THE FUTURE WRITE BUFFER WITH FULL RANDOM VALUES (here packet size will be 1448 Bytes so 64kiB)
        }
    let write_buffer: [u8; 1448] = array_vec.into_inner().unwrap();
    let mut total_packets: i128 = 0;
    let start: Instant = Instant::now();
    let mut partial_total_packets: i128 = 0;
    let mut partial_start: Instant = Instant::now();
    let mut buff: [u8; 1448];

    while run_tcp.load(Ordering::SeqCst) { //MAIN LOOP OF THE THREAD running when the atomic runner bool is true
        let write_buffer_clone: [u8; 1448] = write_buffer.clone();
        match stream.write(&write_buffer_clone) {
            Ok(_) => {total_packets += 1; partial_total_packets += 1;},
            Err(_) => {},
        }
        
        if print_count_tcp.load(Ordering::SeqCst)==1 {           // PERIODIC STATS PRINT

            buff = [0; 1448]; //reset the buffer before writing what we receive into it
            let mut comparer = buff.clone();
            while comparer == [0; 1448] { //check loop to ensure we got a good response
                stream.flush().unwrap();
                stream.write("updatecall".as_bytes()).ok();
                stream.flush().unwrap();
                stream.read(&mut buff).ok();
                comparer = buff.clone();
            }
            let partial_time: u128 = partial_start.elapsed().as_millis();
            let partial_speed: f64 = (partial_total_packets as f64 * 1448f64 / 1000f64 / (partial_time as f64/1000f64)).round();
            let partial_receiver_count: i128 = String::from_utf8(buff.to_vec()).unwrap().trim_end_matches('\0').parse().unwrap();
            let partial_drop_count: i128 = partial_receiver_count-partial_total_packets;
            let partial_drop_ratio: f64 = ((partial_drop_count as f64 / partial_total_packets as f64)*100.0).round();
            println!( //PARTIAL PRINT
                "Partial average speed : {}Ko/s\
                \nPartial packet drop ratio : {}% ({} dropped count/{})", 
                partial_speed, 
                partial_drop_ratio, 
                partial_drop_count, 
                partial_total_packets
            );
            partial_total_packets = 0;
            partial_start = Instant::now();
            print_count_tcp.store(2, Ordering::SeqCst); //trigger the print of the next thread (icmp_ping)
        }
    }

    while print_count_tcp.load(Ordering::SeqCst)!=11 {         //waiting for LAST PRINT BEFORE CLOSING, triggered by sync_thread
        thread::sleep(Duration::from_millis(100));
    }

    buff = [0; 1448]; //reset the buffer before writing what we receive into it
    let mut comparer = buff.clone();
    while comparer == [0; 1448] { //check loop to ensure we got a good response
        stream.flush().unwrap();
        stream.write("finishcall".as_bytes()).ok();
        stream.flush().unwrap();
        stream.read(&mut buff).ok();
        comparer = buff.clone();
    }
    let receiver_count: i128 = String::from_utf8(buff.to_vec()).unwrap().trim_end_matches('\0').parse().unwrap();
    let total_time:u64 = start.elapsed().as_secs();
    let total_speed: i128 = total_packets*1448/1000/total_time as i128;
    let drop_count: i128 = receiver_count-total_packets;
    let drop_ratio: f64 = ((drop_count as f64 / total_packets as f64)*100.0).round();
 
    println!( //LAST PRINT
        "Total time of the benchmark : {}secs\
        \nTotal bytes transfered : {}Mo\
        \nTotal average speed : {} Ko/s\
        \nTotal packet drop ratio : {}% ({} dropped count/{} total)", 
        total_time,
        total_packets*1448/1000000,
        total_speed, 
        drop_ratio, 
        drop_count, 
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

    let (pinger, results) = match Pinger::new(Some(500), Some(64)){ //init a pinger that will ping every 200ms a 64bytes packet
        Ok((pinger, results)) => (pinger, results),
        Err(e) => panic!("Error creating pinger: {}", e), 
    };

    pinger.add_ipaddr(dist_addr.to_string().as_str()); 

    pinger.run_pinger(); //launch the pinger

    while run_ping.load(Ordering::SeqCst) { // MAIN LOOP OF THE FCT running when atomic runner is true
        match results.recv() {
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
        
        if print_count_ping.load(Ordering::SeqCst)==2 {      // PERIODIC STAT PRINT triggered after the tcp periodic print
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

            icmp_socket.send_to(dest_addr, packet).expect("Error while sending echo request");//sending echo request

            let (_packet, src) = icmp_socket.rcv_from().unwrap();//listening for answer
            
            let sender_address = src.as_socket_ipv4().unwrap();//getting the adress from the answer

            src_ip = IpAddr::V4(*sender_address.ip()); //extracting ip
            addr_vec.push((ttl_counter, src_ip)); //pushing in stockage vector
        }

        while print_count_route.load(Ordering::SeqCst)!=3 {  // PERIODIC STAT PRINT WAITING triggered after the icmp_ping periodic print
            thread::sleep(Duration::from_millis(200));
            if !run_route.load(Ordering::SeqCst) {breaker=true; break};
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
