//! How the client behaves when the PLC is slow, quiet, or gone.

mod common;

use std::time::Duration;

use common::{FakePlc, SYMBOL, SYMBOL_VALUE};
use openstar_ads::{data_types::udint::AdsUdint, Error, NotificationTransmissionMode};
use tokio::time::{timeout, Instant};

const TIMEOUT: Duration = Duration::from_millis(200);

#[tokio::test]
async fn reads_a_symbol_from_a_responsive_plc() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (_session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();

    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn a_quiet_plc_does_not_lose_the_connection() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (_session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    // Nothing crosses the socket for well over the read timeout.
    tokio::time::sleep(TIMEOUT * 3).await;

    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn a_late_reply_does_not_lose_the_connection() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    session.hold_replies();
    let timed_out = ads.read_symbol::<AdsUdint>(SYMBOL).await;
    assert!(
        timed_out.is_err(),
        "read should time out while replies are held"
    );
    session.release_replies();

    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn a_cancelled_request_does_not_lose_the_connection() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    session.hold_replies();
    let cancelled = tokio::select! {
        _ = ads.read_symbol::<AdsUdint>(SYMBOL) => false,
        _ = session.wait_for_requests(2) => true,
    };
    assert!(cancelled, "the read should be dropped once the PLC has it");
    session.release_replies();

    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn a_closed_socket_is_reported_at_once_and_can_be_reconnected() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    let mut subscription = ads
        .subscribe::<AdsUdint>(SYMBOL, NotificationTransmissionMode::ServerOnChange)
        .await
        .unwrap();
    session.notify(&7u32.to_le_bytes());
    assert_eq!(subscription.recv().await.flatten().map(u32::from), Some(7));

    session.close();

    let closed = timeout(TIMEOUT, subscription.recv()).await;
    assert!(
        matches!(closed, Ok(None)),
        "subscription should end when the socket closes, got {closed:?}"
    );
    assert!(!ads.is_connected().await);

    let started = Instant::now();
    let failed = ads.read_symbol::<AdsUdint>(SYMBOL).await;
    assert!(matches!(failed, Err(Error::Disconnected)), "got {failed:?}");
    assert!(
        started.elapsed() < TIMEOUT / 2,
        "a request on a dead connection should fail at once"
    );

    let (_session, ()) = tokio::join!(plc.accept(), ads.connect_with_retry());
    assert!(ads.is_connected().await);
    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn disconnecting_does_not_wait_on_a_peer_that_never_closes() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads_without_timeouts().await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();
    session.keep_open_after_client_shutdown();

    let mut subscription = ads
        .subscribe::<AdsUdint>(SYMBOL, NotificationTransmissionMode::ServerOnChange)
        .await
        .unwrap();

    timeout(TIMEOUT, ads.disconnect())
        .await
        .expect("disconnect should not wait on the peer");

    assert!(!ads.is_connected().await);
    assert!(subscription.recv().await.is_none());
}

#[tokio::test]
async fn a_subscriber_that_stops_reading_does_not_stall_requests() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads(TIMEOUT).await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();

    let _unread = ads
        .subscribe::<AdsUdint>(SYMBOL, NotificationTransmissionMode::ServerOnChange)
        .await
        .unwrap();
    for _ in 0..200 {
        session.notify(&1u32.to_le_bytes());
    }

    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}

#[tokio::test]
async fn a_reader_killed_by_a_bad_frame_closes_the_connection() {
    let mut plc = FakePlc::start().await;
    let ads = plc.ads_without_timeouts().await;
    let (session, connected) = tokio::join!(plc.accept(), ads.connect());
    connected.unwrap();
    session.keep_open_after_client_shutdown();

    let mut subscription = ads
        .subscribe::<AdsUdint>(SYMBOL, NotificationTransmissionMode::ServerOnChange)
        .await
        .unwrap();

    session.send_malformed_frame();

    let closed = timeout(TIMEOUT, subscription.recv()).await;
    assert!(
        matches!(closed, Ok(None)),
        "subscription should end when the reader dies, got {closed:?}"
    );
    assert!(!ads.is_connected().await);

    let started = Instant::now();
    let failed = ads.read_symbol::<AdsUdint>(SYMBOL).await;
    assert!(matches!(failed, Err(Error::Disconnected)), "got {failed:?}");
    assert!(started.elapsed() < TIMEOUT / 2);

    timeout(TIMEOUT, ads.disconnect())
        .await
        .expect("disconnect should not wait on the peer");

    let (_session, ()) = tokio::join!(plc.accept(), ads.connect_with_retry());
    let value: AdsUdint = ads.read_symbol(SYMBOL).await.unwrap();
    assert_eq!(u32::from(value), SYMBOL_VALUE);
}
