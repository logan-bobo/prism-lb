use derive_new::new;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Debug, new)]
pub struct TcpPool {
    available: Mutex<VecDeque<Arc<TcpStream>>>,
    in_use: Mutex<HashMap<usize, Arc<TcpStream>>>,
    max_connections: usize,
    id_counter: AtomicUsize,
}

impl TcpPool {
    async fn get_connection(&self) -> (usize, Arc<TcpStream>) {
        let mut available_lock = self.available.lock().await;

        let connection = available_lock.pop_front().unwrap();

        let mut in_use_lock = self.in_use.lock().await;

        in_use_lock.insert(self.id_counter.load(Ordering::SeqCst), connection.clone());

        self.id_counter.fetch_add(1, Ordering::SeqCst);

        (self.id_counter.load(Ordering::SeqCst), connection)
    }

    async fn return_connection(&self, connection_id: usize) {
        let in_use_lock = self.in_use.lock().await;

        let connection = in_use_lock
            .get(&connection_id)
            .expect("could not find the connection from the in use pool");

        let mut avaliable_lock = self.available.lock().await;

        avaliable_lock.push_back(connection.clone());
    }
}
