//! End to end: deck states pushed into a fake engine come out of real UDP
//! sockets as decodable OSC, routed as the config says.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use decktector_core::model::{DeckId, DeckState, PlayState};
use decktector_core::{Engine, EngineHandle};
use decktector_tempo::{
    BpmRefresh, CustomValue, MessageKind, OscMessageConfig, OscTarget, TempoOutputConfig,
    TempoOutputService, TempoOutputStatus, TempoSource, Trigger, FULL_REFRESH_PERIOD,
};
use tokio::net::UdpSocket;

const RECEIVE_TIMEOUT: Duration = Duration::from_secs(3);
const QUIET_PERIOD: Duration = Duration::from_millis(300);
const PACKET_BUFFER: usize = 512;
/// Larger than any UDP datagram can be, so the OS refuses to send it.
const OVERSIZED_TEMPLATE_LENGTH: usize = 70_000;

struct FakeEngine {
    handle: EngineHandle,
}

#[async_trait]
impl Engine for FakeEngine {
    async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn handle(&self) -> &EngineHandle {
        &self.handle
    }
}

#[derive(Debug, PartialEq)]
enum Arg {
    Int(i32),
    Float(f32),
    String(String),
}

/// Reads a null-terminated string and skips its padding; returns the string
/// and the offset of the next word.
fn read_padded_string(bytes: &[u8], start: usize) -> (String, usize) {
    let length = bytes[start..]
        .iter()
        .position(|&byte| byte == 0)
        .expect("null terminator");
    let text = String::from_utf8(bytes[start..start + length].to_vec()).unwrap();
    (text, (start + length + 4) & !3)
}

fn decode(packet: &[u8]) -> (String, Vec<Arg>) {
    assert_eq!(packet.len() % 4, 0, "OSC packets are word aligned");
    let (address, offset) = read_padded_string(packet, 0);
    let (tags, mut offset) = read_padded_string(packet, offset);
    let args = tags
        .strip_prefix(',')
        .expect("type tag string starts with a comma")
        .chars()
        .map(|tag| match tag {
            'i' | 'f' => {
                let word: [u8; 4] = packet[offset..offset + 4].try_into().unwrap();
                offset += 4;
                if tag == 'i' {
                    Arg::Int(i32::from_be_bytes(word))
                } else {
                    Arg::Float(f32::from_be_bytes(word))
                }
            }
            's' => {
                let (text, next) = read_padded_string(packet, offset);
                offset = next;
                Arg::String(text)
            }
            other => panic!("unexpected type tag {other}"),
        })
        .collect();
    (address, args)
}

fn deck(deck_id: DeckId, is_master: bool, bpm: f64, beat: Option<u8>) -> DeckState {
    DeckState {
        deck_id,
        play_state: PlayState::Playing,
        is_master,
        is_synced: false,
        is_on_air: false,
        pitch: 0.0,
        track_bpm: Some(bpm),
        effective_bpm: Some(bpm),
        beat_in_measure: beat,
        track_id: None,
        playhead_ms: None,
        reported_at: 0.0,
        active_loop: None,
    }
}

/// A localhost UDP socket standing in for a lighting desk.
struct Receiver {
    socket: UdpSocket,
}

impl Receiver {
    async fn bind() -> Self {
        Self {
            socket: UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap(),
        }
    }

    fn port(&self) -> u16 {
        self.socket.local_addr().unwrap().port()
    }

    async fn next_message(&self) -> Option<(String, Vec<Arg>)> {
        let mut buffer = [0u8; PACKET_BUFFER];
        let (length, _) = tokio::time::timeout(RECEIVE_TIMEOUT, self.socket.recv_from(&mut buffer))
            .await
            .ok()?
            .unwrap();
        Some(decode(&buffer[..length]))
    }

    /// Reads until a message at `address` arrives, returning its arguments.
    async fn wait_for(&self, address: &str) -> Vec<Arg> {
        let deadline = tokio::time::Instant::now() + RECEIVE_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            if let Some((received, args)) = self.next_message().await {
                if received == address {
                    return args;
                }
            }
        }
        panic!("no message at {address}");
    }

    /// Everything that arrives within `period`.
    async fn collect_for(&self, period: Duration) -> Vec<(String, Vec<Arg>)> {
        let mut buffer = [0u8; PACKET_BUFFER];
        let mut messages = Vec::new();
        let deadline = tokio::time::Instant::now() + period;
        while let Ok(Ok((length, _))) =
            tokio::time::timeout_at(deadline, self.socket.recv_from(&mut buffer)).await
        {
            messages.push(decode(&buffer[..length]));
        }
        messages
    }

    async fn assert_quiet(&self) {
        let messages = self.collect_for(QUIET_PERIOD).await;
        assert!(messages.is_empty(), "unexpected {messages:?}");
    }
}

fn target(id: &str, port: u16) -> OscTarget {
    OscTarget {
        id: id.to_string(),
        name: id.to_string(),
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        enabled: true,
    }
}

fn message(id: &str, address: &str, kind: MessageKind, target_ids: &[&str]) -> OscMessageConfig {
    OscMessageConfig {
        id: id.to_string(),
        enabled: true,
        source: TempoSource::Master,
        kind,
        address: address.to_string(),
        target_ids: target_ids.iter().map(|id| id.to_string()).collect(),
    }
}

fn bpm_kind(refresh: BpmRefresh) -> MessageKind {
    MessageKind::Bpm { refresh }
}

fn custom(trigger: Trigger, value: CustomValue) -> MessageKind {
    MessageKind::Custom {
        trigger,
        value,
        bpm_decimals: 1,
        refresh: BpmRefresh::OnChange,
    }
}

fn enabled_config(targets: Vec<OscTarget>, messages: Vec<OscMessageConfig>) -> TempoOutputConfig {
    TempoOutputConfig {
        enabled: true,
        targets,
        messages,
    }
}

struct Rig {
    engine: Arc<FakeEngine>,
    service: Arc<TempoOutputService>,
}

impl Rig {
    /// A running service with `config` applied.
    fn start(config: TempoOutputConfig) -> Self {
        let engine = Arc::new(FakeEngine {
            handle: EngineHandle::new(),
        });
        let service = TempoOutputService::new(engine.clone());
        service.set_config(config);
        tokio::spawn(service.clone().run());
        Self { engine, service }
    }

    /// One receiver `"a"`, one message per `kind` going to it.
    async fn with_one_receiver(messages: Vec<(&str, MessageKind)>) -> (Self, Receiver) {
        let receiver = Receiver::bind().await;
        let messages = messages
            .into_iter()
            .map(|(address, kind)| message(address, address, kind, &["a"]))
            .collect();
        let rig = Self::start(enabled_config(vec![target("a", receiver.port())], messages));
        (rig, receiver)
    }

    fn update_deck(&self, deck: DeckState) {
        self.engine.handle.update_deck_state(deck);
    }

    async fn wait_for_status(
        &self,
        mut predicate: impl FnMut(&TempoOutputStatus) -> bool,
    ) -> TempoOutputStatus {
        let mut status_rx = self.service.subscribe();
        let current = self.service.status();
        if predicate(&current) {
            return current;
        }
        tokio::time::timeout(RECEIVE_TIMEOUT, async {
            loop {
                let status = status_rx.recv().await.unwrap();
                if predicate(&status) {
                    return status;
                }
            }
        })
        .await
        .expect("the awaited status")
    }
}

#[tokio::test]
async fn bpm_comes_out_as_a_raw_float() {
    let (rig, receiver) =
        Rig::with_one_receiver(vec![("/bpm", bpm_kind(BpmRefresh::OnChange))]).await;
    tokio::task::yield_now().await;

    rig.update_deck(deck(2, true, 127.5, Some(1)));

    assert_eq!(receiver.wait_for("/bpm").await, [Arg::Float(127.5)]);
}

#[tokio::test]
async fn two_targets_receive_only_their_routed_messages() {
    let to_a = Receiver::bind().await;
    let to_b = Receiver::bind().await;
    let rig = Rig::start(enabled_config(
        vec![target("a", to_a.port()), target("b", to_b.port())],
        vec![
            message("m1", "/only/a", bpm_kind(BpmRefresh::OnChange), &["a"]),
            message("m2", "/only/b", MessageKind::EachBeat, &["b"]),
            message("m3", "/both", MessageKind::BarStart, &["a", "b"]),
        ],
    ));
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, Some(1)));

    assert_eq!(to_a.wait_for("/only/a").await, [Arg::Float(120.0)]);
    assert_eq!(to_b.wait_for("/only/b").await, [Arg::Int(1)]);
    assert_eq!(to_a.wait_for("/both").await, [Arg::Int(1)]);
    assert_eq!(to_b.wait_for("/both").await, [Arg::Int(1)]);

    // Nothing leaked to the wrong receiver.
    for (address, _) in to_a.collect_for(QUIET_PERIOD).await {
        assert_ne!(address, "/only/b");
    }
    for (address, _) in to_b.collect_for(QUIET_PERIOD).await {
        assert_ne!(address, "/only/a");
    }
}

#[tokio::test]
async fn a_disabled_target_receives_nothing() {
    let receiver = Receiver::bind().await;
    let mut off = target("a", receiver.port());
    off.enabled = false;
    let rig = Rig::start(enabled_config(
        vec![off],
        vec![message(
            "m1",
            "/bpm",
            bpm_kind(BpmRefresh::OnChange),
            &["a"],
        )],
    ));
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, Some(1)));

    receiver.assert_quiet().await;
    assert!(!rig.service.status().running);
}

#[tokio::test]
async fn a_custom_template_string_reaches_the_receiver() {
    let (rig, receiver) = Rig::with_one_receiver(vec![(
        "/tempo",
        MessageKind::Custom {
            trigger: Trigger::Bpm,
            value: CustomValue::Template {
                template: "deck {deck} runs {bpm} bpm".to_string(),
            },
            bpm_decimals: 1,
            refresh: BpmRefresh::OnChange,
        },
    )])
    .await;
    tokio::task::yield_now().await;

    rig.update_deck(deck(3, true, 127.96, Some(2)));

    assert_eq!(
        receiver.wait_for("/tempo").await,
        [Arg::String("deck 3 runs 128.0 bpm".to_string())]
    );
}

#[tokio::test]
async fn a_normalized_float_is_scaled_and_clamped() {
    let (rig, receiver) = Rig::with_one_receiver(vec![(
        "/norm",
        custom(
            Trigger::Bpm,
            CustomValue::FloatNormalized {
                min: 60.0,
                max: 180.0,
            },
        ),
    )])
    .await;
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, None));
    assert_eq!(receiver.wait_for("/norm").await, [Arg::Float(0.5)]);

    rig.update_deck(deck(1, true, 240.0, None));
    assert_eq!(receiver.wait_for("/norm").await, [Arg::Float(1.0)]);
}

#[tokio::test]
async fn a_no_argument_message_arrives_without_arguments() {
    let (rig, receiver) = Rig::with_one_receiver(vec![(
        "/bar",
        custom(Trigger::BarStart, CustomValue::NoArgument),
    )])
    .await;
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, Some(1)));

    assert_eq!(receiver.wait_for("/bar").await, []);
}

#[tokio::test]
async fn only_the_start_of_the_next_bar_is_sent() {
    let (rig, receiver) = Rig::with_one_receiver(vec![("/bar", MessageKind::BarStart)]).await;
    tokio::task::yield_now().await;
    rig.update_deck(deck(1, true, 120.0, Some(1)));
    receiver.wait_for("/bar").await;

    for beat in [2, 3, 4, 1] {
        rig.update_deck(deck(1, true, 120.0, Some(beat)));
    }

    // Beats 2 to 4 are silent: exactly one more message, for the next bar.
    let messages = receiver.collect_for(QUIET_PERIOD).await;
    assert_eq!(messages, [("/bar".to_string(), vec![Arg::Int(1)])]);
}

#[tokio::test]
async fn each_beat_sends_every_beat() {
    let (rig, receiver) = Rig::with_one_receiver(vec![("/beat", MessageKind::EachBeat)]).await;
    tokio::task::yield_now().await;

    for beat in [1, 2, 3] {
        rig.update_deck(deck(1, true, 120.0, Some(beat)));
        assert_eq!(
            receiver.wait_for("/beat").await,
            [Arg::Int(i32::from(beat))]
        );
    }
}

#[tokio::test]
async fn a_deck_that_is_not_playing_sends_nothing() {
    let (rig, receiver) = Rig::with_one_receiver(vec![
        ("/bpm", bpm_kind(BpmRefresh::OnChangeAndEverySecond)),
        ("/beat", MessageKind::EachBeat),
    ])
    .await;
    tokio::task::yield_now().await;

    let mut paused = deck(1, true, 120.0, Some(1));
    paused.play_state = PlayState::Paused;
    rig.update_deck(paused);

    receiver.assert_quiet().await;
}

#[tokio::test]
async fn a_deck_source_ignores_the_master() {
    let receiver = Receiver::bind().await;
    let mut deck_two = message("m1", "/deck2", bpm_kind(BpmRefresh::OnChange), &["a"]);
    deck_two.source = TempoSource::Deck { deck_id: 2 };
    let rig = Rig::start(enabled_config(
        vec![target("a", receiver.port())],
        vec![deck_two],
    ));
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, Some(1)));
    receiver.assert_quiet().await;

    rig.update_deck(deck(2, false, 133.0, Some(1)));
    assert_eq!(receiver.wait_for("/deck2").await, [Arg::Float(133.0)]);
}

#[tokio::test]
async fn bpm_is_resent_every_second_only_when_asked_to() {
    let (rig, receiver) = Rig::with_one_receiver(vec![
        ("/steady", bpm_kind(BpmRefresh::OnChange)),
        ("/refreshed", bpm_kind(BpmRefresh::OnChangeAndEverySecond)),
    ])
    .await;
    tokio::task::yield_now().await;
    rig.update_deck(deck(1, true, 120.0, None));

    let messages = receiver
        .collect_for(FULL_REFRESH_PERIOD * 2 + QUIET_PERIOD)
        .await;
    let count = |address: &str| messages.iter().filter(|(a, _)| a == address).count();
    assert_eq!(count("/steady"), 1);
    assert!(count("/refreshed") >= 2, "{messages:?}");
}

#[tokio::test]
async fn a_config_change_resends_bpm_but_not_the_beat() {
    let receiver = Receiver::bind().await;
    let config = enabled_config(
        vec![target("a", receiver.port())],
        vec![
            message("m1", "/bpm", bpm_kind(BpmRefresh::OnChange), &["a"]),
            message("m2", "/beat", MessageKind::EachBeat, &["a"]),
        ],
    );
    let rig = Rig::start(config.clone());
    tokio::task::yield_now().await;
    rig.update_deck(deck(1, true, 128.0, Some(2)));
    receiver.wait_for("/bpm").await;
    receiver.wait_for("/beat").await;
    receiver.assert_quiet().await;

    let mut changed = config;
    changed.messages[0].address = "/bpm2".to_string();
    rig.service.set_config(changed);

    let messages = receiver.collect_for(QUIET_PERIOD).await;
    assert_eq!(messages, [("/bpm2".to_string(), vec![Arg::Float(128.0)])]);
}

#[tokio::test]
async fn disabled_output_sends_nothing() {
    let (rig, receiver) =
        Rig::with_one_receiver(vec![("/bpm", bpm_kind(BpmRefresh::OnChange))]).await;
    let mut config = rig.service.config();
    config.enabled = false;
    rig.service.set_config(config);
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 128.0, Some(1)));

    receiver.assert_quiet().await;
    assert!(!rig.service.status().running);
}

#[tokio::test]
async fn status_reports_the_master_and_that_it_runs() {
    let (rig, _receiver) =
        Rig::with_one_receiver(vec![("/bpm", bpm_kind(BpmRefresh::OnChange))]).await;
    tokio::task::yield_now().await;
    rig.update_deck(deck(3, true, 126.5, None));

    let status = rig
        .wait_for_status(|status| status.master_deck_id == Some(3))
        .await;

    assert!(status.running);
    assert_eq!(status.master_bpm, Some(126.5));
    assert!(status.target_issues.is_empty());
    assert!(status.message_issues.is_empty());
    assert_eq!(rig.service.status(), status);
}

#[tokio::test]
async fn an_invalid_config_is_applied_with_its_issues_reported() {
    let receiver = Receiver::bind().await;
    let bad_address = message(
        "bad-address",
        "nope",
        bpm_kind(BpmRefresh::OnChange),
        &["a"],
    );
    let rig = Rig::start(enabled_config(
        vec![
            target("a", receiver.port()),
            OscTarget {
                host: "not-an-ip".to_string(),
                ..target("bad-host", 9000)
            },
        ],
        vec![
            bad_address,
            message("no-target", "/x", MessageKind::EachBeat, &[]),
            message("good", "/bpm", bpm_kind(BpmRefresh::OnChange), &["a"]),
        ],
    ));
    tokio::task::yield_now().await;
    rig.update_deck(deck(1, true, 120.0, Some(1)));

    // The usable message still goes out...
    assert_eq!(receiver.wait_for("/bpm").await, [Arg::Float(120.0)]);

    // ...and the unusable parts are listed.
    let status = rig
        .wait_for_status(|status| status.running && !status.message_issues.is_empty())
        .await;
    let target_ids: Vec<&str> = status
        .target_issues
        .iter()
        .map(|issue| issue.target_id.as_str())
        .collect();
    let message_ids: Vec<&str> = status
        .message_issues
        .iter()
        .map(|issue| issue.message_id.as_str())
        .collect();
    assert_eq!(target_ids, ["bad-host"]);
    // A message with no checked target is not an issue: the settings panel
    // warns about it on its own, and the message is simply never sent.
    assert_eq!(message_ids, ["bad-address"]);
}

#[tokio::test]
async fn a_send_error_on_one_target_is_reported_without_stopping_the_other() {
    let failing = Receiver::bind().await;
    let working = Receiver::bind().await;
    let oversized = MessageKind::Custom {
        trigger: Trigger::Bpm,
        value: CustomValue::Template {
            template: "x".repeat(OVERSIZED_TEMPLATE_LENGTH),
        },
        bpm_decimals: 0,
        refresh: BpmRefresh::OnChange,
    };
    let rig = Rig::start(enabled_config(
        vec![
            target("failing", failing.port()),
            target("working", working.port()),
        ],
        vec![
            message("m1", "/huge", oversized, &["failing"]),
            message("m2", "/bpm", bpm_kind(BpmRefresh::OnChange), &["working"]),
        ],
    ));
    tokio::task::yield_now().await;

    rig.update_deck(deck(1, true, 120.0, Some(1)));

    assert_eq!(working.wait_for("/bpm").await, [Arg::Float(120.0)]);
    let status = rig
        .wait_for_status(|status| !status.target_issues.is_empty())
        .await;
    assert_eq!(status.target_issues.len(), 1);
    assert_eq!(status.target_issues[0].target_id, "failing");
    assert!(status.target_issues[0].message.contains("could not send"));
    assert!(status.running);
}
