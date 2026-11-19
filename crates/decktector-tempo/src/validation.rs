//! Config validation at the boundary.
//!
//! [`TempoOutputConfig::validate`] never rejects a whole config: it keeps
//! what is usable, drops what is not, and lists the problems so the settings
//! panel can show them next to the offending target or message. The run
//! loop only ever sees the [`ValidTempoConfig`].

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};

use decktector_core::model::DeckId;

use crate::config::{
    BpmRefresh, CustomValue, MessageKind, OscMessageConfig, OscTarget, TempoOutputConfig,
    TempoSource, Trigger, MAX_BPM_DECIMALS, MAX_DECK_ID,
};
use crate::osc::is_valid_address;
use crate::status::{MessageIssue, TargetIssue};
use crate::template::Template;
use crate::value::ValueSpec;

/// Why a target cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetProblem {
    #[error("\"{0}\" is not an IPv4 address")]
    InvalidHost(String),
    #[error("port 0 is not a valid destination port")]
    InvalidPort,
}

/// Why a message cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MessageProblem {
    #[error("\"{0}\" is not a valid OSC address")]
    InvalidAddress(String),
    #[error("the minimum must be below the maximum")]
    InvalidRange,
    #[error("{0} decimals is too many (at most {max})", max = MAX_BPM_DECIMALS)]
    TooManyDecimals(u8),
    #[error("unknown template variable \"{0}\"")]
    UnknownTemplateVariable(String),
    #[error("deck {0} does not exist (decks are 1 to {max})", max = MAX_DECK_ID)]
    DeckOutOfRange(DeckId),
}

/// A target that passed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidTarget {
    pub id: String,
    pub address: SocketAddrV4,
}

/// A message that passed validation, with its kind flattened into what the
/// run loop needs: what it watches, how it refreshes and what it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidMessage {
    pub id: String,
    pub source: TempoSource,
    pub address: String,
    pub trigger: Trigger,
    /// Only meaningful when `trigger` is [`Trigger::Bpm`].
    pub refresh: BpmRefresh,
    pub value: ValueSpec,
    /// Never empty: a message without a usable target is dropped.
    pub targets: Vec<ValidTarget>,
}

/// A [`TempoOutputConfig`] reduced to what can be sent, plus what was left
/// out and why.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValidTempoConfig {
    pub enabled: bool,
    /// The messages that are enabled, valid and have at least one enabled,
    /// valid target.
    pub messages: Vec<ValidMessage>,
    pub target_issues: Vec<TargetIssue>,
    pub message_issues: Vec<MessageIssue>,
}

impl ValidTempoConfig {
    /// Whether anything should be sent at all.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.messages.is_empty()
    }
}

impl TempoOutputConfig {
    /// Checks every enabled target and message, whether or not the output
    /// is enabled, so a saved config always shows its problems. Disabled
    /// targets and messages are skipped without comment.
    pub fn validate(&self) -> ValidTempoConfig {
        let mut target_issues = Vec::new();
        let mut usable_targets: HashMap<&str, ValidTarget> = HashMap::new();
        for target in self.targets.iter().filter(|target| target.enabled) {
            match parse_target(target) {
                Ok(valid) => {
                    usable_targets.insert(target.id.as_str(), valid);
                }
                Err(problem) => target_issues.push(TargetIssue {
                    target_id: target.id.clone(),
                    message: problem.to_string(),
                }),
            }
        }

        let mut messages = Vec::new();
        let mut message_issues = Vec::new();
        for message in self.messages.iter().filter(|message| message.enabled) {
            match validate_message(message, &usable_targets) {
                Ok(Some(valid)) => messages.push(valid),
                Ok(None) => {}
                Err(problems) => {
                    message_issues.extend(problems.into_iter().map(|problem| MessageIssue {
                        message_id: message.id.clone(),
                        message: problem.to_string(),
                    }));
                }
            }
        }

        ValidTempoConfig {
            enabled: self.enabled,
            messages,
            target_issues,
            message_issues,
        }
    }
}

fn parse_target(target: &OscTarget) -> Result<ValidTarget, TargetProblem> {
    let host: Ipv4Addr = target
        .host
        .trim()
        .parse()
        .map_err(|_| TargetProblem::InvalidHost(target.host.clone()))?;
    if target.port == 0 {
        return Err(TargetProblem::InvalidPort);
    }
    Ok(ValidTarget {
        id: target.id.clone(),
        address: SocketAddrV4::new(host, target.port),
    })
}

/// `Ok(None)`: nothing wrong, but no usable target to send to.
fn validate_message(
    message: &OscMessageConfig,
    usable_targets: &HashMap<&str, ValidTarget>,
) -> Result<Option<ValidMessage>, Vec<MessageProblem>> {
    let mut problems = Vec::new();

    if let TempoSource::Deck { deck_id } = message.source {
        if !(1..=MAX_DECK_ID).contains(&deck_id) {
            problems.push(MessageProblem::DeckOutOfRange(deck_id));
        }
    }
    if !is_valid_address(&message.address) {
        problems.push(MessageProblem::InvalidAddress(message.address.clone()));
    }
    // No checked target is not a problem to report: the settings panel
    // already warns about it on the message's row, and a message with no
    // usable target is simply never sent.
    let compiled = compile_kind(&message.kind, &mut problems);

    let Some(compiled) = compiled.filter(|_| problems.is_empty()) else {
        return Err(problems);
    };

    let mut targets: Vec<ValidTarget> = Vec::new();
    for id in &message.target_ids {
        if let Some(target) = usable_targets.get(id.as_str()) {
            if !targets.contains(target) {
                targets.push(target.clone());
            }
        }
    }
    if targets.is_empty() {
        return Ok(None);
    }

    Ok(Some(ValidMessage {
        id: message.id.clone(),
        source: message.source,
        address: message.address.clone(),
        trigger: compiled.trigger,
        refresh: compiled.refresh,
        value: compiled.value,
        targets,
    }))
}

struct CompiledKind {
    trigger: Trigger,
    refresh: BpmRefresh,
    value: ValueSpec,
}

/// Flattens `kind`; pushes what is wrong with it to `problems`, and returns
/// `None` when something is.
fn compile_kind(kind: &MessageKind, problems: &mut Vec<MessageProblem>) -> Option<CompiledKind> {
    match kind {
        MessageKind::Bpm { refresh } => Some(CompiledKind {
            trigger: Trigger::Bpm,
            refresh: *refresh,
            value: ValueSpec::FloatRaw,
        }),
        MessageKind::BarStart => Some(CompiledKind {
            trigger: Trigger::BarStart,
            refresh: BpmRefresh::OnChange,
            value: ValueSpec::Int,
        }),
        MessageKind::EachBeat => Some(CompiledKind {
            trigger: Trigger::EachBeat,
            refresh: BpmRefresh::OnChange,
            value: ValueSpec::Int,
        }),
        MessageKind::Custom {
            trigger,
            value,
            bpm_decimals,
            refresh,
        } => {
            if *bpm_decimals > MAX_BPM_DECIMALS {
                problems.push(MessageProblem::TooManyDecimals(*bpm_decimals));
            }
            let value = compile_value(value, *bpm_decimals, problems)?;
            Some(CompiledKind {
                trigger: *trigger,
                refresh: *refresh,
                value,
            })
        }
    }
}

fn compile_value(
    value: &CustomValue,
    bpm_decimals: u8,
    problems: &mut Vec<MessageProblem>,
) -> Option<ValueSpec> {
    match value {
        CustomValue::FloatRaw => Some(ValueSpec::FloatRaw),
        CustomValue::Int => Some(ValueSpec::Int),
        CustomValue::NoArgument => Some(ValueSpec::NoArgument),
        CustomValue::FloatNormalized { min, max } => {
            // Written so that NaN fails too.
            if min.is_finite() && max.is_finite() && min < max {
                Some(ValueSpec::FloatNormalized {
                    min: *min,
                    max: *max,
                })
            } else {
                problems.push(MessageProblem::InvalidRange);
                None
            }
        }
        CustomValue::Template { template } => match Template::parse(template) {
            Ok(template) => Some(ValueSpec::Template {
                template,
                bpm_decimals,
            }),
            Err(unknown) => {
                problems.push(MessageProblem::UnknownTemplateVariable(unknown.0));
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str, host: &str, port: u16) -> OscTarget {
        OscTarget {
            id: id.to_string(),
            name: id.to_string(),
            host: host.to_string(),
            port,
            enabled: true,
        }
    }

    fn message(id: &str, target_ids: &[&str]) -> OscMessageConfig {
        OscMessageConfig {
            id: id.to_string(),
            enabled: true,
            source: TempoSource::Master,
            kind: MessageKind::Bpm {
                refresh: BpmRefresh::OnChange,
            },
            address: "/bpm".to_string(),
            target_ids: target_ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    fn custom(value: CustomValue, bpm_decimals: u8) -> MessageKind {
        MessageKind::Custom {
            trigger: Trigger::Bpm,
            value,
            bpm_decimals,
            refresh: BpmRefresh::OnChange,
        }
    }

    fn config(targets: Vec<OscTarget>, messages: Vec<OscMessageConfig>) -> TempoOutputConfig {
        TempoOutputConfig {
            enabled: true,
            targets,
            messages,
        }
    }

    fn one_target() -> Vec<OscTarget> {
        vec![target("t1", "127.0.0.1", 9000)]
    }

    /// The only issue message of a single-message config.
    fn only_message_issue(message: OscMessageConfig) -> String {
        let valid = config(one_target(), vec![message]).validate();
        assert!(valid.messages.is_empty());
        assert_eq!(valid.message_issues.len(), 1, "{:?}", valid.message_issues);
        valid.message_issues[0].message.clone()
    }

    #[test]
    fn default_config_is_valid_and_inactive() {
        let valid = TempoOutputConfig::default().validate();
        assert!(!valid.is_active());
        assert!(valid.target_issues.is_empty());
        assert!(valid.message_issues.is_empty());
    }

    #[test]
    fn a_usable_message_with_a_usable_target_is_active() {
        let valid = config(one_target(), vec![message("m1", &["t1"])]).validate();
        assert!(valid.is_active());
        assert_eq!(valid.messages.len(), 1);
        assert_eq!(
            valid.messages[0].targets,
            [ValidTarget {
                id: "t1".to_string(),
                address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000),
            }]
        );
    }

    #[test]
    fn a_disabled_output_still_validates_but_is_inactive() {
        let mut config = config(one_target(), vec![message("m1", &["t1"])]);
        config.enabled = false;
        let valid = config.validate();
        assert_eq!(valid.messages.len(), 1);
        assert!(!valid.is_active());
    }

    #[test]
    fn host_is_trimmed() {
        let valid = config(
            vec![target("t1", " 10.0.0.255 ", 9000)],
            vec![message("m1", &["t1"])],
        )
        .validate();
        assert_eq!(
            valid.messages[0].targets[0].address,
            SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 255), 9000)
        );
    }

    #[test]
    fn hostnames_and_ipv6_are_target_issues() {
        for host in ["localhost", "::1", "", "256.0.0.1", "1.2.3"] {
            let valid = config(vec![target("t1", host, 9000)], vec![]).validate();
            assert_eq!(
                valid.target_issues,
                [TargetIssue {
                    target_id: "t1".to_string(),
                    message: format!("\"{host}\" is not an IPv4 address"),
                }],
                "{host:?}"
            );
        }
    }

    #[test]
    fn port_zero_is_a_target_issue() {
        let valid = config(vec![target("t1", "127.0.0.1", 0)], vec![]).validate();
        assert_eq!(valid.target_issues.len(), 1);
        assert_eq!(valid.target_issues[0].target_id, "t1");
        assert!(valid.target_issues[0].message.contains("port 0"));
    }

    #[test]
    fn one_bad_target_does_not_spoil_the_others() {
        let valid = config(
            vec![
                target("bad", "nope", 9000),
                target("good", "127.0.0.1", 9000),
            ],
            vec![message("m1", &["bad", "good"])],
        )
        .validate();
        assert_eq!(valid.target_issues.len(), 1);
        assert!(valid.message_issues.is_empty());
        assert_eq!(valid.messages[0].targets.len(), 1);
        assert_eq!(valid.messages[0].targets[0].id, "good");
    }

    #[test]
    fn disabled_targets_are_skipped_silently() {
        let mut disabled_bad = target("t1", "nope", 0);
        disabled_bad.enabled = false;
        let valid = config(vec![disabled_bad], vec![message("m1", &["t1"])]).validate();
        assert!(valid.target_issues.is_empty());
        assert!(valid.message_issues.is_empty());
        assert!(valid.messages.is_empty());
    }

    #[test]
    fn a_disabled_target_receives_nothing_even_when_checked() {
        let mut off = target("off", "127.0.0.1", 9001);
        off.enabled = false;
        let valid = config(
            vec![target("on", "127.0.0.1", 9000), off],
            vec![message("m1", &["on", "off"])],
        )
        .validate();
        assert_eq!(valid.messages[0].targets.len(), 1);
        assert_eq!(valid.messages[0].targets[0].id, "on");
    }

    #[test]
    fn disabled_messages_are_skipped_silently() {
        let mut off = message("m1", &[]);
        off.address = "bad address".to_string();
        off.enabled = false;
        let valid = config(one_target(), vec![off]).validate();
        assert!(valid.messages.is_empty());
        assert!(valid.message_issues.is_empty());
    }

    #[test]
    fn invalid_address_is_an_issue() {
        for address in ["", "bpm", "/a b", "/a*", "/a#"] {
            let mut bad = message("m1", &["t1"]);
            bad.address = address.to_string();
            assert_eq!(
                only_message_issue(bad),
                format!("\"{address}\" is not a valid OSC address"),
                "{address:?}"
            );
        }
    }

    #[test]
    fn deck_outside_one_to_six_is_an_issue() {
        for deck_id in [0, 7, 255] {
            let mut bad = message("m1", &["t1"]);
            bad.source = TempoSource::Deck { deck_id };
            assert!(
                only_message_issue(bad).contains(&format!("deck {deck_id} does not exist")),
                "{deck_id}"
            );
        }
    }

    #[test]
    fn decks_one_to_six_are_fine() {
        for deck_id in 1..=MAX_DECK_ID {
            let mut good = message("m1", &["t1"]);
            good.source = TempoSource::Deck { deck_id };
            let valid = config(one_target(), vec![good]).validate();
            assert_eq!(valid.messages.len(), 1, "{deck_id}");
        }
    }

    #[test]
    fn normalized_min_not_below_max_is_an_issue() {
        for (min, max) in [
            (1.0, 1.0),
            (2.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
        ] {
            let mut bad = message("m1", &["t1"]);
            bad.kind = custom(CustomValue::FloatNormalized { min, max }, 0);
            assert_eq!(
                only_message_issue(bad),
                "the minimum must be below the maximum",
                "{min} {max}"
            );
        }
    }

    #[test]
    fn more_than_two_decimals_is_an_issue() {
        let mut bad = message("m1", &["t1"]);
        bad.kind = custom(CustomValue::FloatRaw, 3);
        assert_eq!(
            only_message_issue(bad),
            "3 decimals is too many (at most 2)"
        );
    }

    #[test]
    fn zero_to_two_decimals_are_fine() {
        for decimals in 0..=MAX_BPM_DECIMALS {
            let mut good = message("m1", &["t1"]);
            good.kind = custom(CustomValue::FloatRaw, decimals);
            assert_eq!(
                config(one_target(), vec![good]).validate().messages.len(),
                1
            );
        }
    }

    #[test]
    fn unknown_template_variable_is_an_issue() {
        let mut bad = message("m1", &["t1"]);
        bad.kind = custom(
            CustomValue::Template {
                template: "{bpm} {tempo}".to_string(),
            },
            0,
        );
        assert_eq!(
            only_message_issue(bad),
            "unknown template variable \"tempo\""
        );
    }

    #[test]
    fn known_template_variables_are_fine() {
        let mut good = message("m1", &["t1"]);
        good.kind = custom(
            CustomValue::Template {
                template: "{deck}/{bpm}/{beat}".to_string(),
            },
            1,
        );
        assert_eq!(
            config(one_target(), vec![good]).validate().messages.len(),
            1
        );
    }

    #[test]
    fn no_checked_target_is_silent_not_an_issue() {
        let valid = config(one_target(), vec![message("m1", &[])]).validate();
        assert!(valid.messages.is_empty());
        assert!(valid.message_issues.is_empty());
    }

    #[test]
    fn only_unusable_targets_checked_is_silent_not_an_issue() {
        let mut off = target("off", "127.0.0.1", 9000);
        off.enabled = false;
        let valid = config(
            vec![off, target("bad", "nope", 9000)],
            vec![message("m1", &["off", "bad", "unknown-id"])],
        )
        .validate();
        assert!(valid.messages.is_empty());
        assert!(valid.message_issues.is_empty());
        assert!(!valid.is_active());
    }

    #[test]
    fn every_problem_of_a_message_is_listed() {
        let mut bad = message("m1", &["t1"]);
        bad.address = "nope".to_string();
        bad.source = TempoSource::Deck { deck_id: 9 };
        let valid = config(one_target(), vec![bad]).validate();
        assert_eq!(valid.message_issues.len(), 2);
        assert!(valid.message_issues.iter().all(|i| i.message_id == "m1"));
    }

    #[test]
    fn a_bad_message_does_not_spoil_the_others() {
        let mut bad = message("bad", &["t1"]);
        bad.address = "nope".to_string();
        let valid = config(one_target(), vec![bad, message("good", &["t1"])]).validate();
        assert_eq!(valid.messages.len(), 1);
        assert_eq!(valid.messages[0].id, "good");
        assert_eq!(valid.message_issues.len(), 1);
    }

    #[test]
    fn duplicate_target_ids_on_a_message_are_sent_once() {
        let valid = config(one_target(), vec![message("m1", &["t1", "t1"])]).validate();
        assert_eq!(valid.messages[0].targets.len(), 1);
    }

    #[test]
    fn prefilled_kinds_compile_to_trigger_and_value() {
        let compile = |kind: MessageKind| {
            let mut message = message("m1", &["t1"]);
            message.kind = kind;
            config(one_target(), vec![message])
                .validate()
                .messages
                .remove(0)
        };

        let bpm = compile(MessageKind::Bpm {
            refresh: BpmRefresh::OnChangeAndEverySecond,
        });
        assert_eq!(bpm.trigger, Trigger::Bpm);
        assert_eq!(bpm.refresh, BpmRefresh::OnChangeAndEverySecond);
        assert_eq!(bpm.value, ValueSpec::FloatRaw);

        let bar = compile(MessageKind::BarStart);
        assert_eq!(
            (bar.trigger, bar.value),
            (Trigger::BarStart, ValueSpec::Int)
        );

        let beat = compile(MessageKind::EachBeat);
        assert_eq!(
            (beat.trigger, beat.value),
            (Trigger::EachBeat, ValueSpec::Int)
        );
    }

    #[test]
    fn custom_kind_compiles_with_its_template() {
        let mut custom_message = message("m1", &["t1"]);
        custom_message.kind = MessageKind::Custom {
            trigger: Trigger::EachBeat,
            value: CustomValue::Template {
                template: "b{beat}".to_string(),
            },
            bpm_decimals: 2,
            refresh: BpmRefresh::OnChange,
        };
        let valid = config(one_target(), vec![custom_message]).validate();
        let compiled = &valid.messages[0];
        assert_eq!(compiled.trigger, Trigger::EachBeat);
        assert_eq!(
            compiled.value,
            ValueSpec::Template {
                template: Template::parse("b{beat}").unwrap(),
                bpm_decimals: 2,
            }
        );
    }
}
