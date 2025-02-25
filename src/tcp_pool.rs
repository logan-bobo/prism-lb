use derive_new::new;
use std::collections::HashMap;
use std::collections::VecDeque;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Debug, new)]
pub struct TcpPool {
    available: Mutex<VecDeque<TcpStream>>,
    in_use: Mutex<HashMap<usize, TcpStream>>,
    max_connections: usize,
}

impl TcpPool {
    async fn get_connection() {
        todo!()
    }

    async fn return_connection() {
        todo!()
    }
}
