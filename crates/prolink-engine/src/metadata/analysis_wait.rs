//! Waiting for a player to finish analysing a track itself.
//!
//! A file loaded without rekordbox analysis (or a streamed track) has no
//! beat grid or waveform when it is loaded: the player computes them over
//! the following seconds. Asked right away, the dbserver has nothing to
//! give, so the fetcher asks again on a fixed schedule — the one
//! `beat-link`'s `WaveformFinder` uses (`ANALYSIS_UPDATE_INTERVAL`,
//! `MAXIMUM_ANALYSIS_WAIT`).

use std::time::Duration;

use prolink::status::{TRACK_TYPE_STREAMING, TRACK_TYPE_UNANALYZED};
use tokio::time::{sleep, Instant};

/// Time between two requests for the analysis the player is computing.
pub const ANALYSIS_RETRY_INTERVAL: Duration = Duration::from_secs(10);
/// How long after the first fetch the player is still expected to finish.
pub const MAXIMUM_ANALYSIS_WAIT: Duration = Duration::from_secs(90);

/// Whether a track of `track_type` may still be under analysis by the
/// player that loaded it, so a missing beat grid or waveform is worth
/// asking for again.
pub fn player_may_still_be_analysing(track_type: u8) -> bool {
    matches!(track_type, TRACK_TYPE_UNANALYZED | TRACK_TYPE_STREAMING)
}

/// The retry schedule for one track: an attempt every `interval`, none
/// later than `max_wait` after the schedule started.
pub struct AnalysisWait {
    interval: Duration,
    deadline: Instant,
}

impl AnalysisWait {
    pub fn new(interval: Duration, max_wait: Duration) -> Self {
        Self {
            interval,
            deadline: Instant::now() + max_wait,
        }
    }

    /// Sleeps until the next attempt is due; `false`, without sleeping,
    /// once that attempt would fall past the deadline.
    pub async fn next_attempt(&self) -> bool {
        if Instant::now() + self.interval > self.deadline {
            return false;
        }
        sleep(self.interval).await;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prolink::status::TRACK_TYPE_REKORDBOX;

    #[test]
    fn an_unanalysed_file_and_a_streamed_track_are_waited_for() {
        assert!(player_may_still_be_analysing(TRACK_TYPE_UNANALYZED));
        assert!(player_may_still_be_analysing(TRACK_TYPE_STREAMING));
    }

    /// A rekordbox track's analysis is already on the media: if it is
    /// missing, asking again will not make it appear.
    #[test]
    fn a_rekordbox_track_and_an_audio_cd_are_not_waited_for() {
        const AUDIO_CD: u8 = 0x05;
        assert!(!player_may_still_be_analysing(TRACK_TYPE_REKORDBOX));
        assert!(!player_may_still_be_analysing(AUDIO_CD));
    }

    #[tokio::test(start_paused = true)]
    async fn attempts_come_every_ten_seconds_until_ninety_seconds() {
        let started = Instant::now();
        let wait = AnalysisWait::new(ANALYSIS_RETRY_INTERVAL, MAXIMUM_ANALYSIS_WAIT);

        let mut attempted_at = Vec::new();
        while wait.next_attempt().await {
            attempted_at.push(started.elapsed().as_secs());
        }

        assert_eq!(attempted_at, vec![10, 20, 30, 40, 50, 60, 70, 80, 90]);
    }

    #[tokio::test(start_paused = true)]
    async fn no_attempt_is_due_once_the_deadline_has_passed() {
        let wait = AnalysisWait::new(ANALYSIS_RETRY_INTERVAL, MAXIMUM_ANALYSIS_WAIT);
        tokio::time::advance(MAXIMUM_ANALYSIS_WAIT).await;
        assert!(!wait.next_attempt().await);
    }
}
