//! The `{bpm}` / `{beat}` / `{deck}` mini-language of template arguments.
//!
//! Parsed once, at validation, so an unknown variable is reported to the
//! user instead of being sent as literal text; rendered on every send.

use crate::reading::DeckTempo;

/// Variable names a template may use, without their braces.
pub const VARIABLE_BPM: &str = "bpm";
pub const VARIABLE_BEAT: &str = "beat";
pub const VARIABLE_DECK: &str = "deck";

const OPEN_BRACE: char = '{';
const CLOSE_BRACE: char = '}';

/// A `{name}` the template does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown template variable \"{0}\"")]
pub struct UnknownVariable(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variable {
    Bpm,
    Beat,
    Deck,
}

impl Variable {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            VARIABLE_BPM => Some(Self::Bpm),
            VARIABLE_BEAT => Some(Self::Beat),
            VARIABLE_DECK => Some(Self::Deck),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Variable(Variable),
}

/// A parsed template string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    segments: Vec<Segment>,
}

impl Template {
    /// Splits `text` into literals and variables. An opening brace with no
    /// closing one is plain text.
    pub fn parse(text: &str) -> Result<Self, UnknownVariable> {
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut rest = text;

        while let Some(open) = rest.find(OPEN_BRACE) {
            let after_open = &rest[open + OPEN_BRACE.len_utf8()..];
            let Some(close) = after_open.find(CLOSE_BRACE) else {
                break;
            };
            let name = &after_open[..close];
            let variable =
                Variable::from_name(name).ok_or_else(|| UnknownVariable(name.to_string()))?;

            literal.push_str(&rest[..open]);
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(Segment::Variable(variable));
            rest = &after_open[close + CLOSE_BRACE.len_utf8()..];
        }

        literal.push_str(rest);
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Ok(Self { segments })
    }

    /// The template with `source`'s values substituted; `{bpm}` is written
    /// with `bpm_decimals` decimals. `None` when a variable the template
    /// uses has no value right now (BPM or beat unknown).
    pub fn render(&self, source: &DeckTempo, bpm_decimals: u8) -> Option<String> {
        let mut text = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(literal) => text.push_str(literal),
                Segment::Variable(Variable::Bpm) => {
                    text.push_str(&format!("{:.*}", usize::from(bpm_decimals), source.bpm?));
                }
                Segment::Variable(Variable::Beat) => {
                    text.push_str(&source.beat_in_measure?.to_string());
                }
                Segment::Variable(Variable::Deck) => text.push_str(&source.deck_id.to_string()),
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(bpm: Option<f64>, beat: Option<u8>) -> DeckTempo {
        DeckTempo {
            deck_id: 2,
            bpm,
            beat_in_measure: beat,
        }
    }

    fn render(text: &str, source: &DeckTempo, decimals: u8) -> Option<String> {
        Template::parse(text).unwrap().render(source, decimals)
    }

    #[test]
    fn plain_text_is_rendered_as_is() {
        assert_eq!(
            render("hello", &source(None, None), 0).as_deref(),
            Some("hello")
        );
        assert_eq!(render("", &source(None, None), 0).as_deref(), Some(""));
    }

    #[test]
    fn every_variable_is_substituted() {
        let text = render(
            "deck {deck} at {bpm} beat {beat}",
            &source(Some(128.0), Some(3)),
            1,
        );
        assert_eq!(text.as_deref(), Some("deck 2 at 128.0 beat 3"));
    }

    #[test]
    fn bpm_follows_the_requested_decimals() {
        let source = source(Some(127.456), None);
        assert_eq!(render("{bpm}", &source, 0).as_deref(), Some("127"));
        assert_eq!(render("{bpm}", &source, 1).as_deref(), Some("127.5"));
        assert_eq!(render("{bpm}", &source, 2).as_deref(), Some("127.46"));
    }

    #[test]
    fn a_variable_may_appear_several_times() {
        let text = render("{beat}-{beat}", &source(None, Some(4)), 0);
        assert_eq!(text.as_deref(), Some("4-4"));
    }

    #[test]
    fn a_used_variable_without_a_value_gives_no_text() {
        assert_eq!(render("{bpm}", &source(None, Some(1)), 0), None);
        assert_eq!(render("{beat}", &source(Some(120.0), None), 0), None);
    }

    #[test]
    fn an_unused_variable_without_a_value_does_not_matter() {
        let text = render("{deck}", &source(None, None), 0);
        assert_eq!(text.as_deref(), Some("2"));
    }

    #[test]
    fn unknown_variables_are_refused() {
        assert_eq!(
            Template::parse("x {tempo} y"),
            Err(UnknownVariable("tempo".to_string()))
        );
        assert_eq!(Template::parse("{}"), Err(UnknownVariable(String::new())));
        assert_eq!(
            Template::parse("{BPM}"),
            Err(UnknownVariable("BPM".to_string()))
        );
    }

    #[test]
    fn an_unclosed_brace_is_plain_text() {
        assert_eq!(
            render("a {bpm", &source(Some(1.0), None), 0).as_deref(),
            Some("a {bpm")
        );
        assert_eq!(
            render("{deck} {", &source(None, None), 0).as_deref(),
            Some("2 {")
        );
    }

    #[test]
    fn a_closing_brace_alone_is_plain_text() {
        assert_eq!(
            render("a } b", &source(None, None), 0).as_deref(),
            Some("a } b")
        );
    }
}
