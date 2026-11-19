//! The run loop's state: last-known decks, the lazily opened UDP socket,
//! what each message last did and which targets failed.
//!
//! No scheduling lives here — [`crate::service`] decides *when* to call
//! [`Output::update`]; [`crate::plan`] decides *what* goes out; this puts it
//! on the wire.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddrV4};

use decktector_core::model::{DeckId, DeckState};
use tokio::net::UdpSocket;

use crate::plan::{PlanState, PlannedPacket};
use crate::reading::read_tempo;
use crate::status::{TargetIssue, TempoOutputStatus};
use crate::validation::ValidTempoConfig;

/// Per target id: `None` when every packet to it went out, otherwise the
/// first error.
type Outcomes = BTreeMap<String, Option<String>>;

#[derive(Default)]
pub(crate) struct Output {
    decks: BTreeMap<DeckId, DeckState>,
    /// Opened on the first packet, dropped when the output is switched off.
    socket: Option<UdpSocket>,
    plan: PlanState,
    /// The error of the last failed send, per target id, until a send to
    /// that target goes through again.
    send_errors: BTreeMap<String, String>,
}

impl Output {
    pub(crate) fn track(&mut self, deck: DeckState) {
        self.decks.insert(deck.deck_id, deck);
    }

    pub(crate) fn forget(&mut self, deck_id: DeckId) {
        self.decks.remove(&deck_id);
    }

    /// Replaces every known deck, e.g. from a snapshot after missed events.
    pub(crate) fn replace_decks(&mut self, decks: impl IntoIterator<Item = DeckState>) {
        self.decks = decks.into_iter().map(|deck| (deck.deck_id, deck)).collect();
    }

    /// Starts over for a new `config`: BPM messages will be sent again,
    /// beats already on air will not. Forgets past send errors, and closes
    /// the socket while the config is inactive.
    pub(crate) fn apply_config(&mut self, config: &ValidTempoConfig) {
        let reading = read_tempo(self.decks.values());
        self.plan.restart(&config.messages, &reading);
        self.send_errors.clear();
        if !config.is_active() {
            self.socket = None;
        }
    }

    /// Sends what `config` calls for now and reports the resulting status.
    /// Sends nothing while `config` is inactive. `refresh_due` is the
    /// once-a-second tick.
    pub(crate) async fn update(
        &mut self,
        config: &ValidTempoConfig,
        refresh_due: bool,
    ) -> TempoOutputStatus {
        let mut status = TempoOutputStatus {
            target_issues: config.target_issues.clone(),
            message_issues: config.message_issues.clone(),
            ..TempoOutputStatus::default()
        };
        if !config.is_active() {
            self.socket = None;
            return status;
        }

        let reading = read_tempo(self.decks.values());
        let packets = self.plan.plan(&config.messages, &reading, refresh_due);
        if !packets.is_empty() {
            let outcomes = self.send_all(&packets).await;
            self.record_outcomes(outcomes);
        }

        status.running = true;
        status.master_deck_id = reading.master.map(|master| master.deck_id);
        status.master_bpm = reading.master.and_then(|master| master.bpm);
        status
            .target_issues
            .extend(
                self.send_errors
                    .iter()
                    .map(|(target_id, message)| TargetIssue {
                        target_id: target_id.clone(),
                        message: message.clone(),
                    }),
            );
        status
    }

    /// Sends every packet to its targets. A failure on one target never
    /// stops the others; UDP has no session to tear down, so the next
    /// update simply tries again.
    async fn send_all(&mut self, packets: &[PlannedPacket<'_>]) -> Outcomes {
        let socket = match self.open_socket().await {
            Ok(socket) => socket,
            Err(error) => {
                let message = format!("could not open the OSC socket: {error}");
                return packets
                    .iter()
                    .flat_map(|packet| packet.targets)
                    .map(|target| (target.id.clone(), Some(message.clone())))
                    .collect();
            }
        };

        let mut outcomes = Outcomes::new();
        for packet in packets {
            for target in packet.targets {
                let outcome = outcomes.entry(target.id.clone()).or_default();
                if let Err(error) = socket.send_to(&packet.bytes, target.address).await {
                    outcome.get_or_insert_with(|| send_error(target.address, &error));
                }
            }
        }
        outcomes
    }

    async fn open_socket(&mut self) -> std::io::Result<&UdpSocket> {
        if self.socket.is_none() {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
            socket.set_broadcast(true)?;
            self.socket = Some(socket);
        }
        Ok(self.socket.as_ref().expect("opened above"))
    }

    /// Keeps the error of every failed target (logged once per distinct
    /// error) and clears the ones whose packets went through.
    fn record_outcomes(&mut self, outcomes: Outcomes) {
        for (target_id, outcome) in outcomes {
            match outcome {
                Some(error) => {
                    if self.send_errors.get(&target_id) != Some(&error) {
                        tracing::warn!(%target_id, %error, "tempo OSC output failed");
                    }
                    self.send_errors.insert(target_id, error);
                }
                None => {
                    self.send_errors.remove(&target_id);
                }
            }
        }
    }
}

fn send_error(address: SocketAddrV4, error: &std::io::Error) -> String {
    format!("could not send to {address}: {error}")
}
