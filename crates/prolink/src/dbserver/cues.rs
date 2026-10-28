//! Decodes the basic cue list a player answers
//! [`super::client::Connection::fetch_cues_basic`] with.
//!
//! The reply (`0x4702`) splits each track's memory points, hot cues and loops
//! over two blobs, in the same order:
//!
//! ```text
//! records  36 bytes per cue, little-endian:
//!          00  u16  order — rekordbox's cue type in its high byte
//!          02  u16  hot cue: 0 for a memory point, 1-8 for A-H
//!          0c  u32  position, as a frame index at 150 frames a second
//!          rest     zero
//! times     8 bytes per cue, little-endian:
//!          00  u32  position in milliseconds
//!          04  u32  loop end in milliseconds, 0xffffffff for a plain cue
//! ```
//!
//! Pinned on three replies two CDJ-2000NXSs gave real decks, recorded in
//! `usr-ein/prolink`'s capture corpus (`S06-load-and-play`,
//! `S15a-sd-alone`), each with three hot cues. Only the fields those replies
//! exercise are trusted: the hot cue number, and both times. The cue type in
//! the order field is not read — a loop is told by its loop end instead,
//! which is what the file itself does — and the frame index only duplicates
//! the millisecond time, less precisely.
//!
//! Records and times are cross-checked before anything is decoded: a record
//! blob that is not a whole number of records, or a times blob that does not
//! match it one for one, decodes to no cues at all. A deck showing no cue
//! markers misleads nobody; markers off by one entry would.
//!
//! The extended list (`0x2b04`, with colors and comments) is not decoded:
//! no real reply to it has been captured, and its entries vary in length
//! with their comments, so one wrong offset would misplace every cue after
//! it. Colors and comments therefore come from the NFS route only.

use crate::nfs::AnlzCue;

/// Bytes per cue in the records blob.
pub const CUE_RECORD_LEN: usize = 0x24;
/// Bytes per cue in the times blob.
const CUE_TIMES_LEN: usize = 8;
/// Offset of the hot cue number within a record.
const HOT_CUE_OFFSET: usize = 2;
/// A plain cue's loop end.
const NO_LOOP_END: u32 = u32::MAX;

/// The cues a basic cue list reply describes, in the order it lists them
/// (by time), or none when its two blobs disagree. `blobs` is every blob
/// of the reply, in argument order.
pub fn parse_cue_points(blobs: &[Vec<u8>]) -> Vec<AnlzCue> {
    let [records, times] = blobs else {
        return Vec::new();
    };
    let (records, partial_record) = records.as_chunks::<CUE_RECORD_LEN>();
    let (times, partial_times) = times.as_chunks::<CUE_TIMES_LEN>();
    if !partial_record.is_empty() || !partial_times.is_empty() || records.len() != times.len() {
        return Vec::new();
    }

    records
        .iter()
        .zip(times)
        .map(|(record, times)| decode_cue(record, times))
        .collect()
}

fn decode_cue(record: &[u8; CUE_RECORD_LEN], times: &[u8; CUE_TIMES_LEN]) -> AnlzCue {
    let hot_cue = u16::from_le_bytes([record[HOT_CUE_OFFSET], record[HOT_CUE_OFFSET + 1]]);
    let time_ms = u32::from_le_bytes([times[0], times[1], times[2], times[3]]);
    let loop_end_ms = u32::from_le_bytes([times[4], times[5], times[6], times[7]]);
    let loop_time_ms = (loop_end_ms != NO_LOOP_END && loop_end_ms > time_ms).then_some(loop_end_ms);

    AnlzCue {
        hot_cue: u8::try_from(hot_cue).ok().filter(|number| *number != 0),
        is_loop: loop_time_ms.is_some(),
        time_ms,
        loop_time_ms,
        comment: None,
        color_code: None,
        color_rgb: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    fn record(order: u16, hot_cue: u16) -> Vec<u8> {
        let mut bytes = vec![0; CUE_RECORD_LEN];
        bytes[..2].copy_from_slice(&order.to_le_bytes());
        bytes[2..4].copy_from_slice(&hot_cue.to_le_bytes());
        bytes
    }

    fn times(time_ms: u32, loop_end_ms: u32) -> Vec<u8> {
        [time_ms.to_le_bytes(), loop_end_ms.to_le_bytes()].concat()
    }

    /// A CDJ-2000NXS's reply for a track with hot cues A, B and C
    /// (`usr-ein/prolink` corpus, `S06-load-and-play`), both blobs verbatim.
    #[test]
    fn decodes_a_cdj_2000nxss_three_hot_cues() {
        let records = decode_hex(concat!(
            "000101000000000000000000280000000000000000000000000000000000000000000000",
            "000102000000000000000000712500000000000000000000000000000000000000000000",
            "0001030000000000000000005b3400000000000000000000000000000000000000000000",
        ));
        let times = decode_hex("0f010000ffffffff9ef90000ffffffff0b5d0100ffffffff");

        let cues = parse_cue_points(&[records, times]);

        let summary: Vec<_> = cues
            .iter()
            .map(|cue| (cue.hot_cue, cue.time_ms, cue.loop_time_ms))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Some(1), 271, None),
                (Some(2), 63_902, None),
                (Some(3), 89_355, None),
            ]
        );
        assert!(cues.iter().all(|cue| !cue.is_loop));
    }

    /// The frame index in each record is the millisecond time at 150 frames
    /// a second, truncated — which is what ties the two blobs together
    /// entry for entry (`S15a-sd-alone`).
    #[test]
    fn each_records_frame_index_matches_its_time() {
        let records = decode_hex(concat!(
            "000101000000000000000000030000000000000000000000000000000000000000000000",
            "000102000000000000000000074a00000000000000000000000000000000000000000000",
            "000103000000000000000000086f00000000000000000000000000000000000000000000",
        ));
        let times = decode_hex("1a000000ffffffff85ed0100ffffffff3be40200ffffffff");

        let cues = parse_cue_points(&[records.clone(), times]);

        for (cue, record) in cues.iter().zip(records.as_chunks::<CUE_RECORD_LEN>().0) {
            let frame = u32::from_le_bytes([record[12], record[13], record[14], record[15]]);
            assert_eq!(frame, cue.time_ms * 150 / 1000);
        }
    }

    #[test]
    fn a_memory_point_has_no_hot_cue() {
        let cues = parse_cue_points(&[record(0x0100, 0), times(5_000, NO_LOOP_END)]);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].hot_cue, None);
    }

    #[test]
    fn a_loop_is_told_by_its_loop_end() {
        let cues = parse_cue_points(&[record(0x0200, 4), times(10_000, 18_000)]);

        assert!(cues[0].is_loop);
        assert_eq!(cues[0].loop_time_ms, Some(18_000));
    }

    #[test]
    fn a_track_without_cues_has_two_empty_blobs() {
        assert!(parse_cue_points(&[Vec::new(), Vec::new()]).is_empty());
    }

    #[test]
    fn blobs_that_disagree_decode_to_no_cues() {
        let two_records = [record(0x0100, 1), record(0x0100, 2)].concat();

        assert!(parse_cue_points(&[two_records, times(1_000, NO_LOOP_END)]).is_empty());
        assert!(parse_cue_points(&[vec![0; 35], times(1_000, NO_LOOP_END)]).is_empty());
        assert!(parse_cue_points(&[record(0x0100, 1)]).is_empty());
    }
}
