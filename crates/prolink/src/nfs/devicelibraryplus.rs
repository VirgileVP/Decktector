//! Parser for Rekordbox 7 / DeviceLibraryPlus (`exportLibrary.db`).
//!
//! On newer USB drives exported with Rekordbox 7 or without "legacy CDJ
//! compatibility" checked, `export.pdb` does not exist. Instead, Rekordbox
//! writes an encrypted SQLite database using SQLCipher 4 to
//! `PIONEER/rekordbox/exportLibrary.db` (or `PIONEER/DeviceLibraryPlus/exportLibrary.db`).
//!
//! This module opens and decrypts that database using `rusqlite` (with bundled SQLCipher),
//! maps the contents into [`prolink::nfs::pdb::Pdb`], and extracts cues into
//! [`pdb.cues`].

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tempfile::NamedTempFile;

use crate::nfs::cues::AnlzCue;
use crate::nfs::pdb::{Pdb, Track};

/// Primary path to the Rekordbox 7 DeviceLibraryPlus database on USB/SD media.
pub const EXPORT_LIBRARY_PATH: &str = "PIONEER/rekordbox/exportLibrary.db";
/// Alternative directory path used by some Rekordbox 7 exports.
pub const EXPORT_LIBRARY_ALT_PATH: &str = "PIONEER/DeviceLibraryPlus/exportLibrary.db";
/// Hidden alternative path observed in some exports.
pub const EXPORT_LIBRARY_HIDDEN_PATH: &str = ".PIONEER/rekordbox/exportLibrary.db";

/// The static SQLCipher encryption key used by Rekordbox for `exportLibrary.db`.
pub const REKORDBOX_CIPHER_KEY: &str =
    "r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls";

/// Normalizes file paths extracted from the database so they match the NFS file structure.
/// Ensures leading slashes are stripped and paths under USBANLZ or Artwork have the `PIONEER/` prefix.
pub fn normalize_device_path(raw: &str) -> String {
    let trimmed = raw.trim_start_matches('/');
    if trimmed.starts_with("PIONEER/") || trimmed.starts_with(".PIONEER/") {
        trimmed.to_string()
    } else {
        format!("PIONEER/{trimmed}")
    }
}

/// Parses an `exportLibrary.db` file from in-memory bytes by writing them to a temporary
/// file and decrypting with SQLCipher.
pub fn parse_export_library(bytes: &[u8]) -> Result<Pdb> {
    let mut temp = NamedTempFile::new().context("failed to create temporary file for SQLite db")?;
    temp.write_all(bytes)
        .context("failed to write SQLite db bytes to temporary file")?;
    temp.flush()
        .context("failed to flush temporary SQLite db file")?;

    parse_export_library_file(temp.path())
}

/// Opens and decrypts an `exportLibrary.db` file from the filesystem.
pub fn parse_export_library_file(path: &Path) -> Result<Pdb> {
    // First try with the standard Rekordbox SQLCipher key.
    // If that fails (e.g. database was already decrypted or unencrypted), try without key.
    let conn = match open_connection(path, Some(REKORDBOX_CIPHER_KEY)) {
        Ok(c) => c,
        Err(cipher_err) => open_connection(path, None).with_context(|| {
            format!(
                "failed to open exportLibrary.db at {} with SQLCipher key: {cipher_err:#}",
                path.display()
            )
        })?,
    };

    parse_export_library_conn(&conn)
}

fn open_connection(path: &Path, key: Option<&str>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    if let Some(key) = key {
        conn.execute_batch(&format!("PRAGMA key = '{key}';"))?;
    }
    // Verify database readability and decryption
    let _: u32 = conn
        .query_row("SELECT count(*) FROM sqlite_master;", [], |row| {
            row.get(0)
        })
        .context("database read test failed (incorrect key or corrupt database)")?;
    Ok(conn)
}

/// Reads tables and constructs a [`Pdb`] instance from an open connection.
pub fn parse_export_library_conn(conn: &Connection) -> Result<Pdb> {
    let mut pdb = Pdb::default();

    if table_exists(conn, "artist") {
        read_id_name_table(conn, "artist", "artist_id", &mut pdb.artists)?;
    }
    if table_exists(conn, "album") {
        read_id_name_table(conn, "album", "album_id", &mut pdb.albums)?;
    }
    if table_exists(conn, "genre") {
        read_id_name_table(conn, "genre", "genre_id", &mut pdb.genres)?;
    }
    if table_exists(conn, "key") {
        read_id_name_table(conn, "key", "key_id", &mut pdb.keys)?;
    }
    if table_exists(conn, "label") {
        read_id_name_table(conn, "label", "label_id", &mut pdb.labels)?;
    }
    if table_exists(conn, "image") {
        read_image_table(conn, &mut pdb.artwork_paths)?;
    }

    if table_exists(conn, "content") {
        read_content_table(conn, &mut pdb.tracks)?;
    }

    if table_exists(conn, "cue") {
        read_cue_table(conn, &mut pdb.cues)?;
    }

    Ok(pdb)
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

fn get_columns(conn: &Connection, table: &str) -> HashSet<String> {
    let mut stmt = match conn.prepare(&format!("PRAGMA table_info({table});")) {
        Ok(s) => s,
        Err(_) => return HashSet::new(),
    };
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map(|iter| {
            iter.filter_map(Result::ok)
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    names
}

fn read_id_name_table(
    conn: &Connection,
    table: &str,
    preferred_id_col: &str,
    target: &mut std::collections::HashMap<u32, String>,
) -> Result<()> {
    let cols = get_columns(conn, table);
    let id_col = if cols.contains(&preferred_id_col.to_lowercase()) {
        preferred_id_col
    } else if cols.contains("id") {
        "id"
    } else {
        return Ok(());
    };
    let name_col = if cols.contains("name") {
        "name"
    } else if cols.contains("title") {
        "title"
    } else {
        return Ok(());
    };

    let sql = format!("SELECT {id_col}, {name_col} FROM {table};");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let id: u32 = row.get(0)?;
        let name: String = row.get(1)?;
        Ok((id, name))
    })?;

    for row in rows.filter_map(Result::ok) {
        target.insert(row.0, row.1);
    }
    Ok(())
}

fn read_image_table(
    conn: &Connection,
    target: &mut std::collections::HashMap<u32, String>,
) -> Result<()> {
    let cols = get_columns(conn, "image");
    let id_col = if cols.contains("image_id") {
        "image_id"
    } else if cols.contains("id") {
        "id"
    } else {
        return Ok(());
    };
    let path_col = if cols.contains("path") {
        "path"
    } else if cols.contains("file_path") {
        "file_path"
    } else {
        return Ok(());
    };

    let sql = format!("SELECT {id_col}, {path_col} FROM image;");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let id: u32 = row.get(0)?;
        let path: String = row.get(1)?;
        Ok((id, normalize_device_path(&path)))
    })?;

    for row in rows.filter_map(Result::ok) {
        target.insert(row.0, row.1);
    }
    Ok(())
}

fn read_content_table(
    conn: &Connection,
    target: &mut std::collections::HashMap<u32, Track>,
) -> Result<()> {
    let cols = get_columns(conn, "content");

    let id_col = if cols.contains("content_id") {
        "content_id"
    } else if cols.contains("id") {
        "id"
    } else {
        return Ok(());
    };
    let title_col = if cols.contains("title") { "title" } else { "''" };
    let artist_col = if cols.contains("artist_id_artist") {
        "artist_id_artist"
    } else if cols.contains("artist_id") {
        "artist_id"
    } else {
        "0"
    };
    let album_col = if cols.contains("album_id") {
        "album_id"
    } else {
        "0"
    };
    let genre_col = if cols.contains("genre_id") {
        "genre_id"
    } else {
        "0"
    };
    let key_col = if cols.contains("key_id") {
        "key_id"
    } else {
        "0"
    };
    let label_col = if cols.contains("label_id") {
        "label_id"
    } else {
        "0"
    };
    let image_col = if cols.contains("image_id") {
        "image_id"
    } else {
        "0"
    };
    let bpm_col = if cols.contains("bpmx100") {
        "bpmx100"
    } else if cols.contains("bpm") {
        "bpm"
    } else {
        "0"
    };
    let len_col = if cols.contains("length") {
        "length"
    } else if cols.contains("duration") {
        "duration"
    } else {
        "0"
    };
    let anlz_col = if cols.contains("analysisdatafilepath") {
        "analysisDataFilePath"
    } else if cols.contains("analysis_data_file_path") {
        "analysis_data_file_path"
    } else {
        "''"
    };

    let sql = format!(
        "SELECT {id_col}, {title_col}, {artist_col}, {album_col}, {genre_col}, {key_col}, {label_col}, {image_col}, {bpm_col}, {len_col}, {anlz_col} FROM content;"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let id: u32 = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let artist_id: Option<u32> = row.get(2)?;
        let album_id: Option<u32> = row.get(3)?;
        let genre_id: Option<u32> = row.get(4)?;
        let key_id: Option<u32> = row.get(5)?;
        let label_id: Option<u32> = row.get(6)?;
        let artwork_id: Option<u32> = row.get(7)?;
        let bpm: Option<u32> = row.get(8)?;
        let length: Option<u32> = row.get(9)?;
        let anlz_path: Option<String> = row.get(10)?;

        let duration_sec = match length {
            Some(raw) if raw > 1000 => raw / 1000,
            Some(raw) => raw,
            None => 0,
        };

        let analyze_path = anlz_path
            .filter(|p| !p.trim().is_empty())
            .map(|p| normalize_device_path(&p))
            .unwrap_or_default();

        Ok(Track {
            id,
            title: title.unwrap_or_default(),
            artist_id: artist_id.unwrap_or(0),
            album_id: album_id.unwrap_or(0),
            genre_id: genre_id.unwrap_or(0),
            key_id: key_id.unwrap_or(0),
            label_id: label_id.unwrap_or(0),
            artwork_id: artwork_id.unwrap_or(0),
            duration_sec,
            tempo_centibpm: bpm.unwrap_or(0),
            analyze_path,
        })
    })?;

    for row in rows.filter_map(Result::ok) {
        target.insert(row.id, row);
    }
    Ok(())
}

fn read_cue_table(
    conn: &Connection,
    target: &mut std::collections::HashMap<u32, Vec<AnlzCue>>,
) -> Result<()> {
    let cols = get_columns(conn, "cue");

    let content_col = if cols.contains("content_id") {
        "content_id"
    } else if cols.contains("track_id") {
        "track_id"
    } else {
        return Ok(());
    };
    let kind_col = if cols.contains("kind") { "kind" } else { "0" };
    let in_col = if cols.contains("inusec") {
        "inUsec"
    } else if cols.contains("in_usec") {
        "in_usec"
    } else if cols.contains("time") {
        "time"
    } else {
        return Ok(());
    };
    let out_col = if cols.contains("outusec") {
        "outUsec"
    } else if cols.contains("out_usec") {
        "out_usec"
    } else if cols.contains("loop_time") {
        "loop_time"
    } else {
        "NULL"
    };
    let comment_col = if cols.contains("cuecomment") {
        "cueComment"
    } else if cols.contains("comment") {
        "comment"
    } else {
        "NULL"
    };
    let color_col = if cols.contains("colortableindex") {
        "colorTableIndex"
    } else if cols.contains("color") {
        "color"
    } else {
        "NULL"
    };

    let sql = format!(
        "SELECT {content_col}, {kind_col}, {in_col}, {out_col}, {comment_col}, {color_col} FROM cue ORDER BY {in_col} ASC;"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let content_id: u32 = row.get(0)?;
        let kind: Option<u32> = row.get(1)?;
        let in_val: Option<u64> = row.get(2)?;
        let out_val: Option<u64> = row.get(3)?;
        let comment: Option<String> = row.get(4)?;
        let color_code: Option<u8> = row.get(5)?;

        let raw_in = in_val.unwrap_or(0);
        let time_ms = if cols.contains("inusec") || cols.contains("in_usec") {
            (raw_in / 1000) as u32
        } else {
            raw_in as u32
        };

        let loop_time_ms = out_val.filter(|u| *u > 0).map(|raw_out| {
            if cols.contains("outusec") || cols.contains("out_usec") {
                (raw_out / 1000) as u32
            } else {
                raw_out as u32
            }
        });

        let is_loop = loop_time_ms.is_some_and(|out_ms| out_ms > time_ms);

        let hot_cue = match kind {
            Some(k @ 1..=8) => Some(k as u8),
            _ => None,
        };

        let cue = AnlzCue {
            hot_cue,
            is_loop,
            time_ms,
            loop_time_ms: if is_loop { loop_time_ms } else { None },
            comment: comment.filter(|c| !c.trim().is_empty()),
            color_code,
            color_rgb: None,
        };

        Ok((content_id, cue))
    })?;

    for row in rows.filter_map(Result::ok) {
        target.entry(row.0).or_default().push(row.1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_device_paths() {
        assert_eq!(
            normalize_device_path("/USBANLZ/P01C/000030FD/ANLZ0000.DAT"),
            "PIONEER/USBANLZ/P01C/000030FD/ANLZ0000.DAT"
        );
        assert_eq!(
            normalize_device_path("USBANLZ/P01C/000030FD/ANLZ0000.DAT"),
            "PIONEER/USBANLZ/P01C/000030FD/ANLZ0000.DAT"
        );
        assert_eq!(
            normalize_device_path("PIONEER/USBANLZ/P01C/000030FD/ANLZ0000.DAT"),
            "PIONEER/USBANLZ/P01C/000030FD/ANLZ0000.DAT"
        );
        assert_eq!(
            normalize_device_path("/Artwork/00001/a1.jpg"),
            "PIONEER/Artwork/00001/a1.jpg"
        );
        assert_eq!(
            normalize_device_path(".PIONEER/Artwork/00001/a1.jpg"),
            ".PIONEER/Artwork/00001/a1.jpg"
        );
    }

    #[test]
    fn parses_unencrypted_sqlite_database() {
        let temp = NamedTempFile::new().unwrap();
        let conn = Connection::open(temp.path()).unwrap();

        conn.execute_batch(
            "
            CREATE TABLE artist (artist_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO artist VALUES (1, 'Bicep');

            CREATE TABLE album (album_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO album VALUES (10, 'Isles');

            CREATE TABLE genre (genre_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO genre VALUES (20, 'Electronic');

            CREATE TABLE key (key_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO key VALUES (30, '8A');

            CREATE TABLE label (label_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO label VALUES (40, 'Ninja Tune');

            CREATE TABLE image (image_id INTEGER PRIMARY KEY, path TEXT);
            INSERT INTO image VALUES (50, '/Artwork/00001/a1.jpg');

            CREATE TABLE content (
                content_id INTEGER PRIMARY KEY,
                title TEXT,
                artist_id_artist INTEGER,
                album_id INTEGER,
                genre_id INTEGER,
                key_id INTEGER,
                label_id INTEGER,
                image_id INTEGER,
                bpmx100 INTEGER,
                length INTEGER,
                analysisDataFilePath TEXT
            );
            INSERT INTO content VALUES (
                100,
                'Glue',
                1,
                10,
                20,
                30,
                40,
                50,
                13000,
                269000,
                '/USBANLZ/P01C/000030FD/ANLZ0000.DAT'
            );

            CREATE TABLE cue (
                cue_id INTEGER PRIMARY KEY,
                content_id INTEGER,
                kind INTEGER,
                inUsec INTEGER,
                outUsec INTEGER,
                cueComment TEXT,
                colorTableIndex INTEGER
            );
            INSERT INTO cue VALUES (1, 100, 1, 15000000, 0, 'Intro', 2);
            INSERT INTO cue VALUES (2, 100, 0, 45000000, 60000000, 'Drop Loop', 5);
        ",
        )
        .unwrap();
        drop(conn);

        let pdb = parse_export_library_file(temp.path()).unwrap();
        assert_eq!(pdb.artists.get(&1), Some(&"Bicep".to_string()));
        assert_eq!(pdb.albums.get(&10), Some(&"Isles".to_string()));
        assert_eq!(pdb.genres.get(&20), Some(&"Electronic".to_string()));
        assert_eq!(pdb.keys.get(&30), Some(&"8A".to_string()));
        assert_eq!(pdb.labels.get(&40), Some(&"Ninja Tune".to_string()));
        assert_eq!(
            pdb.artwork_paths.get(&50),
            Some(&"PIONEER/Artwork/00001/a1.jpg".to_string())
        );

        let track = pdb.tracks.get(&100).expect("track 100 present");
        assert_eq!(track.id, 100);
        assert_eq!(track.title, "Glue");
        assert_eq!(track.artist_id, 1);
        assert_eq!(track.album_id, 10);
        assert_eq!(track.genre_id, 20);
        assert_eq!(track.key_id, 30);
        assert_eq!(track.label_id, 40);
        assert_eq!(track.artwork_id, 50);
        assert_eq!(track.duration_sec, 269);
        assert_eq!(track.tempo_centibpm, 13000);
        assert_eq!(
            track.analyze_path,
            "PIONEER/USBANLZ/P01C/000030FD/ANLZ0000.DAT"
        );

        let cues = pdb.cues.get(&100).expect("cues present");
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].hot_cue, Some(1));
        assert_eq!(cues[0].time_ms, 15000);
        assert!(!cues[0].is_loop);
        assert_eq!(cues[0].comment.as_deref(), Some("Intro"));

        assert_eq!(cues[1].hot_cue, None);
        assert_eq!(cues[1].time_ms, 45000);
        assert_eq!(cues[1].loop_time_ms, Some(60000));
        assert!(cues[1].is_loop);
        assert_eq!(cues[1].comment.as_deref(), Some("Drop Loop"));
    }

    #[test]
    fn parses_sqlcipher_encrypted_database_with_rekordbox_key() {
        let temp = NamedTempFile::new().unwrap();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute_batch(&format!("PRAGMA key = '{REKORDBOX_CIPHER_KEY}';"))
            .unwrap();

        conn.execute_batch(
            "
            CREATE TABLE artist (artist_id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO artist VALUES (2, 'Four Tet');

            CREATE TABLE content (
                content_id INTEGER PRIMARY KEY,
                title TEXT,
                artist_id_artist INTEGER,
                album_id INTEGER,
                genre_id INTEGER,
                key_id INTEGER,
                label_id INTEGER,
                image_id INTEGER,
                bpmx100 INTEGER,
                length INTEGER,
                analysisDataFilePath TEXT
            );
            INSERT INTO content VALUES (
                200,
                'Baby',
                2,
                0,
                0,
                0,
                0,
                0,
                12400,
                240000,
                'PIONEER/USBANLZ/P002/00000002/ANLZ0000.DAT'
            );
        ",
        )
        .unwrap();
        drop(conn);

        let bytes = std::fs::read(temp.path()).unwrap();
        let pdb = parse_export_library(&bytes).expect("decryption and parsing succeeds");
        assert_eq!(pdb.artists.get(&2), Some(&"Four Tet".to_string()));
        let track = pdb.tracks.get(&200).expect("track 200 present");
        assert_eq!(track.title, "Baby");
        assert_eq!(track.tempo_centibpm, 12400);
        assert_eq!(track.duration_sec, 240);
    }
}
