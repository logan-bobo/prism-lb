use std::net::IpAddr;
use std::str::FromStr;

use prism_lb::Backend;
use prism_lb::Server;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:5001").await.unwrap();

    // Static for now but will be dynamic from yaml later....
    let backend_one = Backend::new(IpAddr::from_str("127.0.0.1").unwrap(), 5002);
    let backend_two = Backend::new(IpAddr::from_str("127.0.0.1").unwrap(), 5003);
    let backend_three = Backend::new(IpAddr::from_str("127.0.0.1").unwrap(), 5004);

    let backends = vec![backend_one, backend_two, backend_three];

    let mut server = Server::new(listener, backends).await;

    server.serve().await;
}
