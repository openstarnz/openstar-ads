//! Connecting through a router that accepts the TCP connection but never answers,
//! e.g. adsrouter with no AMS route for our net ID.

use std::time::Duration;

use openstar_ads::{AdsBuilder, Timeouts};
use tokio::{io::AsyncReadExt, net::TcpListener, time::sleep, time::timeout};

#[tokio::test]
async fn connect_to_silent_router_fails_without_ever_reporting_connected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let router = listener.local_addr().unwrap();

    // Accept the connection, then read everything the client sends without ever replying.
    let silent_router = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        socket.read_to_end(&mut received).await
    });

    let ads = AdsBuilder::new("1.2.3.4.1.1:851".parse().unwrap())
        .router(router)
        .source("5.6.7.8.1.1:30000".parse().unwrap())
        .timeouts(Timeouts::new(Duration::from_millis(500)))
        .build()
        .await
        .unwrap();

    let poll_until_connected = async {
        while !ads.is_connected().await {
            sleep(Duration::from_millis(10)).await;
        }
    };

    let result = tokio::select! {
        result = ads.connect() => result,
        () = poll_until_connected => panic!("is_connected() returned true before the PLC answered"),
    };

    assert!(result.is_err(), "connect should fail when the PLC never answers");
    assert!(!ads.is_connected().await);

    // The client must be closed on failure, not left open.
    let read_result = timeout(Duration::from_secs(5), silent_router)
        .await
        .expect("client socket was left open after the failed connect")
        .unwrap();
    assert!(read_result.is_ok());
}
