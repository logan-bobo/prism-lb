use std::{
    io::{prelude::*, BufReader},
    net::{IpAddr, TcpListener, TcpStream},
    str::FromStr,
};

use reqwest::{Client, Method, Request, Url};
use tokio;

#[derive(Debug)]
struct Message {
    method: Method,
    path: String,
    host: IpAddr,
    port: u32,
}

// TODO: In the future it is a good idea to to a from implementation
// but we dont even know if this is the correct choice yet so lets leave
// for now.
async fn produce_message_from_stream(stream: TcpStream) -> Message {
    let mut buf_reader = BufReader::new(&stream).lines();
    let request_line = buf_reader.next().unwrap().unwrap();

    let request: Vec<&str> = request_line.split(" ").collect();

    let method = Method::from_str(request[0]).expect("could not parse string to method");
    let path = request[1].to_string();

    // for now there is no dyanmic back ends for the LB so just hard code to validate
    let host = IpAddr::from_str("127.0.0.1").expect("wrong host");
    let port = 5002;

    Message {
        method,
        path,
        host,
        port,
    }
}

async fn process_stream(stream: TcpStream) {
    let message = produce_message_from_stream(stream).await;

    // Some basic logging for now, implement tracing when we know this is the solution wanted
    println!("{:?}", message);

    // This should all be wrapped up to produce a request from a message
    let url = Url::parse(&format!(
        "http://{}:{}{}",
        &message.host, &message.port, &message.path
    ))
    .expect("could not parse URL");

    println!("{:#?}", url);
    let request = Request::new(message.method, url);

    let client = Client::new();

    let response = client
        .execute(request)
        .await
        .expect("failed to execute request");

    let response_text = response.text().await.expect("failed to get response text");

    println!("{:?}", response_text);
}

#[tokio::main]
async fn main() {
    println!("Starting LB");

    let listener = TcpListener::bind("127.0.0.1:5001").unwrap();

    loop {
        let (stream, _) = listener.accept().unwrap();
        tokio::spawn(async move {
            process_stream(stream).await;
        });
    }
}
