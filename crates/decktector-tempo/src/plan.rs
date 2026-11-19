//! Which packets go out for the current reading.
//!
//! Pure: [`PlanState`] remembers what each message last did, [`PlanState::plan`]
//! compares it with what the decks say now and returns the packets to send.
//!
//! * **BPM-triggered** messages (`Bpm`, custom on the BPM) send when the
//!   encoded packet differs from the last one sent — so a template with no
//!   decimals does not resend on 0.01 jitter — and, when they ask for it,
//!   on every refresh tick.
//! * **Beat-triggered** messages are edge events: sent when the source's
//!   beat changes (`BarStart`: when it becomes 1), never on refresh. For a
//!   master source the previous beat survives a hand-over, so a new master
//!   on the same beat says nothing.
//! * **No value is silence**: no source deck, not playing, unknown BPM or
//!   beat, a template variable without a value — nothing is sent, and
//!   receivers keep what they were last told.

use crate::config::{BpmRefresh, Trigger};
use crate::osc::encode_message;
use crate::reading::TempoReading;
use crate::validation::{ValidMessage, ValidTarget};

/// The beat that opens a bar.
const BAR_START: u8 = 1;

/// One encoded packet and the targets it goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPacket<'a> {
    pub bytes: Vec<u8>,
    pub targets: &'a [ValidTarget],
}

#[derive(Debug, Clone, Default)]
struct MessageState {
    /// The last packet sent for a BPM-triggered message.
    last_packet: Option<Vec<u8>>,
    /// The source's beat when last looked at, for beat-triggered messages.
    last_beat: Option<u8>,
}

/// What every message of a config last did, in the config's order.
#[derive(Debug, Clone, Default)]
pub struct PlanState {
    messages: Vec<MessageState>,
}

impl PlanState {
    /// Starts over for `messages` (after a config change): every
    /// BPM-triggered message will be sent again, while beat-triggered ones
    /// are seeded with the beats on air, so nothing fires for a beat that
    /// was already there.
    pub fn restart(&mut self, messages: &[ValidMessage], reading: &TempoReading) {
        self.messages = messages
            .iter()
            .map(|message| MessageState {
                last_packet: None,
                last_beat: reading
                    .source(message.source)
                    .and_then(|source| source.beat_in_measure),
            })
            .collect();
    }

    /// The packets `messages` call for given `reading`. `refresh_due` is
    /// the once-a-second tick: BPM messages asking for it send again.
    pub fn plan<'a>(
        &mut self,
        messages: &'a [ValidMessage],
        reading: &TempoReading,
        refresh_due: bool,
    ) -> Vec<PlannedPacket<'a>> {
        self.messages
            .resize_with(messages.len(), MessageState::default);

        let mut packets = Vec::new();
        for (message, state) in messages.iter().zip(&mut self.messages) {
            let bytes = match message.trigger {
                Trigger::Bpm => plan_bpm(message, state, reading, refresh_due),
                Trigger::BarStart | Trigger::EachBeat => plan_beat(message, state, reading),
            };
            if let Some(bytes) = bytes {
                packets.push(PlannedPacket {
                    bytes,
                    targets: &message.targets,
                });
            }
        }
        packets
    }
}

/// The encoded packet for `message` from the current reading, `None` when
/// there is no value to send.
fn encode(message: &ValidMessage, reading: &TempoReading) -> Option<Vec<u8>> {
    let source = reading.source(message.source)?;
    let args = message.value.args(message.trigger, source)?;
    Some(encode_message(&message.address, &args))
}

fn plan_bpm(
    message: &ValidMessage,
    state: &mut MessageState,
    reading: &TempoReading,
    refresh_due: bool,
) -> Option<Vec<u8>> {
    let packet = encode(message, reading)?;
    let refresh = refresh_due && message.refresh == BpmRefresh::OnChangeAndEverySecond;
    if !refresh && state.last_packet.as_ref() == Some(&packet) {
        return None;
    }
    state.last_packet = Some(packet.clone());
    Some(packet)
}

fn plan_beat(
    message: &ValidMessage,
    state: &mut MessageState,
    reading: &TempoReading,
) -> Option<Vec<u8>> {
    let beat = reading
        .source(message.source)
        .and_then(|source| source.beat_in_measure);
    let previous = std::mem::replace(&mut state.last_beat, beat);

    let beat = beat?;
    let is_edge =
        previous != Some(beat) && (message.trigger == Trigger::EachBeat || beat == BAR_START);
    if is_edge {
        encode(message, reading)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::*;
    use crate::config::TempoSource;
    use crate::osc::OscArg;
    use crate::reading::DeckTempo;
    use crate::template::Template;
    use crate::value::ValueSpec;

    const ADDRESS: &str = "/m";

    fn target(id: &str) -> ValidTarget {
        ValidTarget {
            id: id.to_string(),
            address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000),
        }
    }

    fn message(
        source: TempoSource,
        trigger: Trigger,
        value: ValueSpec,
        refresh: BpmRefresh,
    ) -> ValidMessage {
        ValidMessage {
            id: "m".to_string(),
            source,
            address: ADDRESS.to_string(),
            trigger,
            refresh,
            value,
            targets: vec![target("t")],
        }
    }

    fn bpm_message(refresh: BpmRefresh) -> ValidMessage {
        message(
            TempoSource::Master,
            Trigger::Bpm,
            ValueSpec::FloatRaw,
            refresh,
        )
    }

    fn beat_message(source: TempoSource, trigger: Trigger) -> ValidMessage {
        message(source, trigger, ValueSpec::Int, BpmRefresh::OnChange)
    }

    fn template_message(trigger: Trigger, text: &str) -> ValidMessage {
        let template = ValueSpec::Template {
            template: Template::parse(text).unwrap(),
            bpm_decimals: 0,
        };
        message(TempoSource::Master, trigger, template, BpmRefresh::OnChange)
    }

    fn tempo(deck_id: u8, bpm: Option<f64>, beat: Option<u8>) -> DeckTempo {
        DeckTempo {
            deck_id,
            bpm,
            beat_in_measure: beat,
        }
    }

    /// Deck 1 is master and the only deck.
    fn solo(bpm: Option<f64>, beat: Option<u8>) -> TempoReading {
        let deck = tempo(1, bpm, beat);
        TempoReading {
            master: Some(deck),
            decks: vec![deck],
        }
    }

    fn packet(args: &[OscArg]) -> Vec<u8> {
        encode_message(ADDRESS, args)
    }

    fn float(value: f32) -> Vec<u8> {
        packet(&[OscArg::Float(value)])
    }

    fn int(value: i32) -> Vec<u8> {
        packet(&[OscArg::Int(value)])
    }

    fn text(value: &str) -> Vec<u8> {
        packet(&[OscArg::String(value.to_string())])
    }

    /// Plans once and returns just the encoded packets.
    fn step(
        state: &mut PlanState,
        messages: &[ValidMessage],
        reading: &TempoReading,
        refresh_due: bool,
    ) -> Vec<Vec<u8>> {
        state
            .plan(messages, reading, refresh_due)
            .into_iter()
            .map(|planned| planned.bytes)
            .collect()
    }

    // --- BPM ---

    #[test]
    fn bpm_is_sent_the_first_time_it_is_known() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.0), None), false),
            [float(128.0)]
        );
    }

    #[test]
    fn unchanged_bpm_is_not_sent_again() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), None);
        step(&mut state, &messages, &reading, false);
        assert!(step(&mut state, &messages, &reading, false).is_empty());
    }

    #[test]
    fn a_changed_bpm_is_sent() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        step(&mut state, &messages, &solo(Some(128.0), None), false);
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.5), None), false),
            [float(128.5)]
        );
    }

    #[test]
    fn on_change_ignores_the_refresh_tick() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), None);
        step(&mut state, &messages, &reading, false);
        assert!(step(&mut state, &messages, &reading, true).is_empty());
    }

    #[test]
    fn every_second_resends_an_unchanged_bpm_on_the_tick_only() {
        let messages = [bpm_message(BpmRefresh::OnChangeAndEverySecond)];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), None);
        step(&mut state, &messages, &reading, false);
        assert!(step(&mut state, &messages, &reading, false).is_empty());
        assert_eq!(step(&mut state, &messages, &reading, true), [float(128.0)]);
    }

    #[test]
    fn dedup_is_on_the_encoded_packet_not_the_raw_bpm() {
        let messages = [template_message(Trigger::Bpm, "{bpm}")];
        let mut state = PlanState::default();

        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.0), None), false),
            [text("128")]
        );
        // 0.01 of jitter rounds to the same text: nothing to say.
        assert!(step(&mut state, &messages, &solo(Some(128.01), None), false).is_empty());
        assert_eq!(
            step(&mut state, &messages, &solo(Some(129.0), None), false),
            [text("129")]
        );
    }

    #[test]
    fn raw_float_resends_on_tiny_changes_that_change_the_f32() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        step(&mut state, &messages, &solo(Some(128.0), None), false);
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.01), None), false).len(),
            1
        );
    }

    #[test]
    fn a_bpm_template_using_the_beat_follows_the_beat() {
        let messages = [template_message(Trigger::Bpm, "{beat}")];
        let mut state = PlanState::default();
        assert_eq!(
            step(&mut state, &messages, &solo(Some(120.0), Some(1)), false),
            [text("1")]
        );
        assert!(step(&mut state, &messages, &solo(Some(120.0), Some(1)), false).is_empty());
        assert_eq!(
            step(&mut state, &messages, &solo(Some(120.0), Some(2)), false),
            [text("2")]
        );
        // No beat, nothing to substitute: silence.
        assert!(step(&mut state, &messages, &solo(Some(120.0), None), false).is_empty());
    }

    #[test]
    fn restart_makes_every_bpm_message_send_again_once() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), None);
        step(&mut state, &messages, &reading, false);

        state.restart(&messages, &reading);
        assert_eq!(step(&mut state, &messages, &reading, false), [float(128.0)]);
        assert!(step(&mut state, &messages, &reading, false).is_empty());
    }

    #[test]
    fn a_planned_packet_carries_its_targets() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        let planned = state.plan(&messages, &solo(Some(128.0), None), false);
        assert_eq!(planned[0].targets, [target("t")]);
    }

    // --- silence ---

    #[test]
    fn no_master_is_silence_for_a_master_message() {
        let messages = [bpm_message(BpmRefresh::OnChangeAndEverySecond)];
        let mut state = PlanState::default();
        let reading = TempoReading {
            master: None,
            decks: vec![tempo(1, Some(128.0), Some(1))],
        };
        assert!(step(&mut state, &messages, &reading, true).is_empty());
    }

    #[test]
    fn unknown_bpm_is_silence() {
        let messages = [bpm_message(BpmRefresh::OnChangeAndEverySecond)];
        let mut state = PlanState::default();
        assert!(step(&mut state, &messages, &solo(None, Some(1)), true).is_empty());
    }

    #[test]
    fn an_absent_source_deck_is_silence() {
        let messages = [message(
            TempoSource::Deck { deck_id: 5 },
            Trigger::Bpm,
            ValueSpec::FloatRaw,
            BpmRefresh::OnChangeAndEverySecond,
        )];
        let mut state = PlanState::default();
        assert!(step(&mut state, &messages, &solo(Some(128.0), Some(1)), true).is_empty());
    }

    #[test]
    fn silence_does_not_make_the_same_bpm_resend_when_it_comes_back() {
        let messages = [bpm_message(BpmRefresh::OnChange)];
        let mut state = PlanState::default();
        step(&mut state, &messages, &solo(Some(128.0), None), false);
        assert!(step(&mut state, &messages, &solo(None, None), false).is_empty());
        assert!(step(&mut state, &messages, &solo(Some(128.0), None), false).is_empty());
    }

    // --- beats ---

    #[test]
    fn each_beat_sends_every_beat_change() {
        let messages = [beat_message(TempoSource::Master, Trigger::EachBeat)];
        let mut state = PlanState::default();
        let mut sent = Vec::new();
        for beat in [1, 2, 3, 4, 1] {
            sent.extend(step(&mut state, &messages, &solo(None, Some(beat)), false));
        }
        assert_eq!(sent, [int(1), int(2), int(3), int(4), int(1)]);
    }

    #[test]
    fn each_beat_does_not_repeat_the_same_beat() {
        let messages = [beat_message(TempoSource::Master, Trigger::EachBeat)];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), Some(2));
        assert_eq!(step(&mut state, &messages, &reading, false), [int(2)]);
        assert!(step(&mut state, &messages, &reading, false).is_empty());
    }

    #[test]
    fn bar_start_sends_only_when_the_beat_becomes_one() {
        let messages = [beat_message(TempoSource::Master, Trigger::BarStart)];
        let mut state = PlanState::default();
        let mut sent = Vec::new();
        for beat in [1, 2, 3, 4, 1, 1, 2] {
            sent.extend(step(&mut state, &messages, &solo(None, Some(beat)), false));
        }
        assert_eq!(sent, [int(1), int(1)]);
    }

    #[test]
    fn bar_start_ignores_a_first_sighting_on_another_beat() {
        let messages = [beat_message(TempoSource::Master, Trigger::BarStart)];
        let mut state = PlanState::default();
        assert!(step(&mut state, &messages, &solo(None, Some(3)), false).is_empty());
    }

    #[test]
    fn beats_are_never_resent_on_the_refresh_tick() {
        let messages = [
            beat_message(TempoSource::Master, Trigger::BarStart),
            beat_message(TempoSource::Master, Trigger::EachBeat),
        ];
        let mut state = PlanState::default();
        let reading = solo(None, Some(1));
        assert_eq!(step(&mut state, &messages, &reading, false).len(), 2);
        assert!(step(&mut state, &messages, &reading, true).is_empty());
    }

    #[test]
    fn a_beat_that_stops_and_restarts_fires_again() {
        let messages = [beat_message(TempoSource::Master, Trigger::BarStart)];
        let mut state = PlanState::default();
        step(&mut state, &messages, &solo(None, Some(1)), false);
        // Paused: no beat. Playing again on beat 1.
        assert!(step(&mut state, &messages, &solo(None, None), false).is_empty());
        assert_eq!(
            step(&mut state, &messages, &solo(None, Some(1)), false),
            [int(1)]
        );
    }

    #[test]
    fn restart_never_resends_a_beat_already_on_air() {
        let messages = [
            beat_message(TempoSource::Master, Trigger::BarStart),
            beat_message(TempoSource::Master, Trigger::EachBeat),
        ];
        let mut state = PlanState::default();
        let reading = solo(Some(128.0), Some(1));
        state.restart(&messages, &reading);
        assert!(step(&mut state, &messages, &reading, false).is_empty());
        assert!(step(&mut state, &messages, &reading, true).is_empty());
        // The next beat still goes out.
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.0), Some(2)), false),
            [int(2)]
        );
    }

    #[test]
    fn a_new_master_on_the_same_beat_says_nothing() {
        let messages = [beat_message(TempoSource::Master, Trigger::EachBeat)];
        let mut state = PlanState::default();
        let deck_one_master = TempoReading {
            master: Some(tempo(1, Some(120.0), Some(3))),
            decks: vec![
                tempo(1, Some(120.0), Some(3)),
                tempo(2, Some(125.0), Some(3)),
            ],
        };
        assert_eq!(
            step(&mut state, &messages, &deck_one_master, false),
            [int(3)]
        );

        let deck_two_master = TempoReading {
            master: Some(tempo(2, Some(125.0), Some(3))),
            decks: deck_one_master.decks.clone(),
        };
        assert!(step(&mut state, &messages, &deck_two_master, false).is_empty());
    }

    #[test]
    fn a_new_master_on_another_beat_sends_that_beat() {
        let messages = [beat_message(TempoSource::Master, Trigger::EachBeat)];
        let mut state = PlanState::default();
        step(&mut state, &messages, &solo(None, Some(3)), false);
        let handed_over = TempoReading {
            master: Some(tempo(2, None, Some(4))),
            decks: vec![tempo(2, None, Some(4))],
        };
        assert_eq!(step(&mut state, &messages, &handed_over, false), [int(4)]);
    }

    #[test]
    fn a_deck_source_ignores_the_master_beat() {
        let messages = [beat_message(
            TempoSource::Deck { deck_id: 2 },
            Trigger::EachBeat,
        )];
        let mut state = PlanState::default();
        let reading = TempoReading {
            master: Some(tempo(1, None, Some(1))),
            decks: vec![tempo(1, None, Some(1)), tempo(2, None, Some(4))],
        };
        assert_eq!(step(&mut state, &messages, &reading, false), [int(4)]);
    }

    #[test]
    fn a_beat_template_without_its_bpm_is_silent() {
        let messages = [template_message(Trigger::EachBeat, "{bpm}")];
        let mut state = PlanState::default();
        assert!(step(&mut state, &messages, &solo(None, Some(1)), false).is_empty());
    }

    #[test]
    fn a_no_argument_message_is_just_the_address() {
        let messages = [message(
            TempoSource::Master,
            Trigger::BarStart,
            ValueSpec::NoArgument,
            BpmRefresh::OnChange,
        )];
        let mut state = PlanState::default();
        assert_eq!(
            step(&mut state, &messages, &solo(None, Some(1)), false),
            [packet(&[])]
        );
    }

    #[test]
    fn messages_are_planned_independently() {
        let messages = [
            bpm_message(BpmRefresh::OnChange),
            beat_message(TempoSource::Master, Trigger::EachBeat),
        ];
        let mut state = PlanState::default();
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.0), Some(2)), false),
            [float(128.0), int(2)]
        );
        assert_eq!(
            step(&mut state, &messages, &solo(Some(128.0), Some(3)), false),
            [int(3)]
        );
    }
}
