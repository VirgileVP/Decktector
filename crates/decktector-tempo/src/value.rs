//! What a message carries: the trigger's value turned into OSC arguments.
//!
//! Pure. [`ValueSpec`] is the validated form of [`crate::config::CustomValue`]
//! (templates already parsed); the prefilled message kinds map onto it too.

use crate::config::Trigger;
use crate::osc::OscArg;
use crate::reading::DeckTempo;
use crate::template::Template;

/// How a trigger's value becomes OSC arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSpec {
    FloatRaw,
    /// Invariant (checked at validation): `min < max`.
    FloatNormalized {
        min: f64,
        max: f64,
    },
    Int,
    Template {
        template: Template,
        bpm_decimals: u8,
    },
    NoArgument,
}

/// The number `trigger` watches: the source's BPM, or its beat. `None`
/// while the source has no such value — which means nothing is sent.
pub fn trigger_value(trigger: Trigger, source: &DeckTempo) -> Option<f64> {
    match trigger {
        Trigger::Bpm => source.bpm,
        Trigger::BarStart | Trigger::EachBeat => source.beat_in_measure.map(f64::from),
    }
}

impl ValueSpec {
    /// The arguments for `source`, `None` when there is nothing to say: the
    /// trigger's value is unknown, or a template variable has no value.
    pub fn args(&self, trigger: Trigger, source: &DeckTempo) -> Option<Vec<OscArg>> {
        let value = trigger_value(trigger, source)?;
        match self {
            Self::FloatRaw => Some(vec![OscArg::Float(value as f32)]),
            Self::FloatNormalized { min, max } => {
                let normalized = ((value - min) / (max - min)).clamp(0.0, 1.0);
                Some(vec![OscArg::Float(normalized as f32)])
            }
            Self::Int => Some(vec![OscArg::Int(value.round() as i32)]),
            Self::Template {
                template,
                bpm_decimals,
            } => template
                .render(source, *bpm_decimals)
                .map(|text| vec![OscArg::String(text)]),
            Self::NoArgument => Some(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(bpm: Option<f64>, beat: Option<u8>) -> DeckTempo {
        DeckTempo {
            deck_id: 4,
            bpm,
            beat_in_measure: beat,
        }
    }

    fn template(text: &str, bpm_decimals: u8) -> ValueSpec {
        ValueSpec::Template {
            template: Template::parse(text).unwrap(),
            bpm_decimals,
        }
    }

    #[test]
    fn trigger_value_is_the_bpm_or_the_beat() {
        let source = source(Some(126.0), Some(3));
        assert_eq!(trigger_value(Trigger::Bpm, &source), Some(126.0));
        assert_eq!(trigger_value(Trigger::BarStart, &source), Some(3.0));
        assert_eq!(trigger_value(Trigger::EachBeat, &source), Some(3.0));
    }

    #[test]
    fn trigger_value_is_none_when_unknown() {
        let source = source(None, None);
        for trigger in [Trigger::Bpm, Trigger::BarStart, Trigger::EachBeat] {
            assert_eq!(trigger_value(trigger, &source), None);
        }
    }

    #[test]
    fn float_raw_keeps_the_bpm_unrounded() {
        let args = ValueSpec::FloatRaw.args(Trigger::Bpm, &source(Some(127.456), None));
        assert_eq!(args, Some(vec![OscArg::Float(127.456_f64 as f32)]));
    }

    #[test]
    fn float_raw_on_a_beat_trigger_is_the_beat_number() {
        let args = ValueSpec::FloatRaw.args(Trigger::EachBeat, &source(None, Some(3)));
        assert_eq!(args, Some(vec![OscArg::Float(3.0)]));
    }

    #[test]
    fn normalized_scales_between_min_and_max() {
        let spec = ValueSpec::FloatNormalized {
            min: 60.0,
            max: 180.0,
        };
        let args = |bpm| spec.args(Trigger::Bpm, &source(Some(bpm), None));
        assert_eq!(args(60.0), Some(vec![OscArg::Float(0.0)]));
        assert_eq!(args(120.0), Some(vec![OscArg::Float(0.5)]));
        assert_eq!(args(180.0), Some(vec![OscArg::Float(1.0)]));
    }

    #[test]
    fn normalized_is_clamped_to_the_unit_range() {
        let spec = ValueSpec::FloatNormalized {
            min: 60.0,
            max: 180.0,
        };
        let args = |bpm| spec.args(Trigger::Bpm, &source(Some(bpm), None));
        assert_eq!(args(20.0), Some(vec![OscArg::Float(0.0)]));
        assert_eq!(args(300.0), Some(vec![OscArg::Float(1.0)]));
    }

    #[test]
    fn normalized_works_on_beats() {
        let spec = ValueSpec::FloatNormalized { min: 1.0, max: 4.0 };
        let args = spec.args(Trigger::EachBeat, &source(None, Some(4)));
        assert_eq!(args, Some(vec![OscArg::Float(1.0)]));
    }

    #[test]
    fn int_rounds_the_bpm_to_the_nearest() {
        let args = |bpm| ValueSpec::Int.args(Trigger::Bpm, &source(Some(bpm), None));
        assert_eq!(args(127.4), Some(vec![OscArg::Int(127)]));
        assert_eq!(args(127.5), Some(vec![OscArg::Int(128)]));
    }

    #[test]
    fn int_on_a_beat_trigger_is_the_beat_number() {
        let args = ValueSpec::Int.args(Trigger::BarStart, &source(None, Some(1)));
        assert_eq!(args, Some(vec![OscArg::Int(1)]));
    }

    #[test]
    fn template_becomes_a_string_argument() {
        let args = template("{deck}:{bpm}", 1).args(Trigger::Bpm, &source(Some(128.04), None));
        assert_eq!(args, Some(vec![OscArg::String("4:128.0".to_string())]));
    }

    #[test]
    fn template_without_a_value_for_a_used_variable_is_silent() {
        let spec = template("{beat}", 0);
        assert_eq!(spec.args(Trigger::Bpm, &source(Some(120.0), None)), None);
    }

    #[test]
    fn no_argument_gives_an_empty_argument_list() {
        let args = ValueSpec::NoArgument.args(Trigger::EachBeat, &source(None, Some(2)));
        assert_eq!(args, Some(Vec::new()));
    }

    #[test]
    fn nothing_is_said_without_a_trigger_value() {
        let silent = source(None, None);
        let specs = [
            ValueSpec::FloatRaw,
            ValueSpec::FloatNormalized { min: 0.0, max: 1.0 },
            ValueSpec::Int,
            template("{deck}", 0),
            ValueSpec::NoArgument,
        ];
        for spec in specs {
            assert_eq!(spec.args(Trigger::Bpm, &silent), None, "{spec:?}");
            assert_eq!(spec.args(Trigger::EachBeat, &silent), None, "{spec:?}");
        }
    }
}
