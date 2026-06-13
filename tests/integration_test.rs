mod common;

use common::{run_test_containers, test_server};
use core::time;
use http::StatusCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::sleep;

#[tokio::test]
async fn test_backend_routing() {
    let _containers = run_test_containers().await;
    let server = test_server().await;
    let server_clone = server.clone();
    let port = server.listener().local_addr().unwrap().port();
    let address = server.listener().local_addr().unwrap().ip();

    tokio::spawn(async move { server.serve().await.unwrap() });

    // TODO: wait for all backends to become healthy
    sleep(time::Duration::from_secs(5)).await;

    for i in 0..3 {
        let actual = server_clone.routing_counter().load(Ordering::SeqCst);
        let atomic = AtomicUsize::new(i);

        let exepected = atomic.load(Ordering::SeqCst);

        assert_eq!(actual, exepected);

        let result = reqwest::get(format!("http://{}:{}", address, port))
            .await
            .unwrap();

        assert_eq!(result.status(), StatusCode::OK);

        assert!(result.text().await.unwrap().contains(&i.to_string()));

        assert_eq!(actual, exepected);
    }
}
