use std::{
    io::{prelude::*, BufReader},
    net::{IpAddr, TcpListener, TcpStream},
    str::FromStr,
};

use reqwest::{Method, Request, Url};

#[derive(Debug)]
struct Message {
    method: Method,
    path: String,
    host: IpAddr,
    port: String,
}

fn produce_message_from_stream(stream: TcpStream) -> Message {
    let mut buf_reader = BufReader::new(&stream).lines();
    let request_line = buf_reader.next().unwrap().unwrap();
    let host_line = buf_reader.next().unwrap().unwrap();

    let request: Vec<&str> = request_line.split(" ").collect();
    let host_and_port: Vec<&str> = host_line.split(" ").collect();
    let addr = host_and_port[1];
    let addr_parts: Vec<&str> = addr.split(":").collect();

    let method = Method::from_str(request[0]).expect("could not parse string to method");
    let path = request[1].to_string();
    let host = IpAddr::from_str(addr_parts[0]).expect("could not parse IpAddr");
    let port = addr_parts[1].to_string();

    Message {
        method,
        path,
        host,
        port,
    }
}

fn main() {
    println!("Starting LB");

    let listener = TcpListener::bind("127.0.0.1:5001").unwrap();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let message = produce_message_from_stream(stream);

        println!("{:?}", message);
    }
}
