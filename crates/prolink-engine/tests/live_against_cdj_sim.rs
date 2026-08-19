//! Exercises [`ProlinkEngine`] against real UDP sockets on the loopback
//! adapter, broadcasting fake Pro DJ Link traffic in-process (the same
//! packets `crates/prolink/src/bin/cdj-sim.rs` sends standalone) rather than
//! feeding it a mock — this is the one test in the whole phase-2 change that
//! proves the actual wire-parsing path end to end, not just each parser in
//! isolation.
//!
//! Ignored by default: it binds the real Pro DJ Link ports and needs
//! loopback broadcast to work, which some sandboxes/CI runners restrict.
//! Run explicitly with:
//!   cargo test -p prolink-engine --test live_against_cdj_sim -- --ignored --nocapture

use std::net::Ipv4Addr;
use std::time::Duration;

use decktective_core::model::PlayState;
use decktective_core::Engine;
use prolink::beat::build_beat_packet;
use prolink::discovery::{build_keepalive, DeviceWatcher};
use prolink::status::{build_status, RawPlayState};
use prolink::{ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use prolink_engine::ProlinkEngine;
use tokio::net::UdpSocket;
use tokio::time::{sleep, timeout};

const SIMULATED_DEVICE: u8 = 2;
const SIMULATED_BPM: f64 = 120.0;
const SIMULATED_TRACK_ID: u32 = 4207;

#[tokio::test]
#[ignore = "binds real UDP sockets; see this file's module doc"]
async fn reports_a_playing_deck_from_simulated_wire_traffic() {
    let broadcaster = tokio::spawn(broadcast_a_playing_deck());

    let device_watcher = DeviceWatcher::new();
    tokio::spawn(device_watcher.clone().run());

    let engine = ProlinkEngine::new(device_watcher);
    engine.start().await.expect("engine should start");

    let deck = timeout(Duration::from_secs(10), wait_for_deck(&engine)).await.expect(
        "no deck reported within 10s — is loopback broadcast blocked in this environment?",
    );

    assert_eq!(deck.deck_id, SIMULATED_DEVICE);
    assert_eq!(deck.play_state, PlayState::Playing);
    assert_eq!(deck.track_id, Some(SIMULATED_TRACK_ID));
    assert_eq!(deck.track_bpm, Some(SIMULATED_BPM));
    assert!(deck.playhead_ms.is_some(), "a playing deck with a beat count should get a playhead estimate");

    engine.stop().await.expect("engine should stop");
    broadcaster.abort();
}

async fn wait_for_deck(engine: &ProlinkEngine) -> decktective_core::model::DeckState {
    loop {
        if let Some(deck) = engine
            .handle()
            .snapshot()
            .decks
            .into_iter()
            .find(|deck| deck.deck_id == SIMULATED_DEVICE && deck.play_state == PlayState::Playing)
        {
            return deck;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

/// Mirrors `cdj-sim`'s own loop, inline: a keep-alive so `DeviceWatcher`
/// (and hence `ProlinkEngine`'s device sync) sees the deck, then a status
/// packet and beat packet at a realistic cadence, playing from the start
/// (no cue delay — this test isn't exercising that transition).
async fn broadcast_a_playing_deck() {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.expect("bind the fake device's socket");
    socket.set_broadcast(true).expect("enable broadcast");
    let target = Ipv4Addr::new(255, 255, 255, 255);

    let mut keepalive_tick = tokio::time::interval(Duration::from_millis(500));
    let mut status_tick = tokio::time::interval(Duration::from_millis(100));
    let mut beat_tick = tokio::time::interval(Duration::from_secs_f64(60.0 / SIMULATED_BPM));
    let mut beat_count: u32 = 0;

    loop {
        tokio::select! {
            _ = keepalive_tick.tick() => {
                let mac = [0x02, 0x00, 0x00, 0x00, 0x00, SIMULATED_DEVICE];
                let packet = build_keepalive(SIMULATED_DEVICE, "TEST-CDJ", mac, Ipv4Addr::LOCALHOST, 1);
                let _ = socket.send_to(&packet, (target, ANNOUNCE_PORT)).await;
            }
            _ = status_tick.tick() => {
                let beat_in_bar = (beat_count % 4) as u8 + 1;
                let packet = build_status(
                    SIMULATED_DEVICE,
                    RawPlayState::Playing,
                    true,
                    false,
                    true,
                    0.0,
                    Some(SIMULATED_BPM),
                    Some(beat_count),
                    Some(beat_in_bar),
                    Some(SIMULATED_TRACK_ID),
                );
                let _ = socket.send_to(&packet, (target, STATUS_PORT)).await;
            }
            _ = beat_tick.tick() => {
                beat_count += 1;
                let beat_in_bar = (beat_count % 4) as u8 + 1;
                let packet = build_beat_packet(SIMULATED_DEVICE, 0.0, Some(SIMULATED_BPM), beat_in_bar);
                let _ = socket.send_to(&packet, (target, BEAT_PORT)).await;
            }
        }
    }
}
