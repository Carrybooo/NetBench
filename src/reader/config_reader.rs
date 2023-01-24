//#![allow(unused)]

use serde_derive::Deserialize;
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
    pub tcp_port: u16,
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
    println!("tcp_port: {}", data.config.tcp_port); 


    return Config{
        num_local: data.config.num_local,
        num_dist: data.config.num_dist,
        ip1: data.config.ip1,
        ip2: data.config.ip2,
        ip3: data.config.ip3,
        ip4: data.config.ip4,
        tcp_port: data.config.tcp_port,
    };
}