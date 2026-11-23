//! Reader for Biome `App.InFocus` SEGB v2 files (the store behind Screen Time, incl. synced iPhone/iPad).
//!
//! Layout, learned from real files:
//! - header 32 bytes: `SEGB`, u32 entry count, f64 creation time (Apple epoch), padding;
//! - records from offset 32, each 4-aligned: u32 crc, u32, then a protobuf message;
//! - a trailer at the end of the file growing backwards, 16 bytes per record:
//!   u32 end offset (relative to 32), u32 state (1 = written), f64 time.
//! The protobuf: 3 = starting (1 focused, 0 left), 4 = time (f64, Apple epoch), 6 = bundle id.

const APPLE_EPOCH_MS: f64 = 978_307_200_000.0;
const DATA_START: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub bundle_id: String,
    pub starting: bool,
    /// Unix milliseconds.
    pub at_ms: i64,
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

/// Every written record of a SEGB v2 file: (trailer time in Unix ms, protobuf payload).
pub fn records(file: &[u8]) -> Vec<(i64, &[u8])> {
    if file.len() < DATA_START || &file[..4] != b"SEGB" {
        return vec![];
    }
    let count = u32_at(file, 4).unwrap_or(0) as usize;
    let mut out = Vec::new();
    let mut start = DATA_START;
    for i in 0..count {
        let Some(t) = file.len().checked_sub(16 * (i + 1)) else { break };
        if t < start {
            break;
        }
        let (Some(end), Some(state)) = (u32_at(file, t), u32_at(file, t + 4)) else { break };
        let end = DATA_START + end as usize;
        if end > t || end < start {
            break;
        }
        let at = f64::from_le_bytes(file[t + 8..t + 16].try_into().unwrap());
        if state == 1 && end >= start + 8 {
            out.push(((at * 1000.0 + APPLE_EPOCH_MS) as i64, &file[start + 8..end]));
        }
        start = (end + 3) & !3;
    }
    out
}

/// A protobuf message without its schema, flattened: ("3", "1"), ("6", "com.apple.Safari"),
/// nested messages as "2.1". Doubles that look like Apple-epoch times become ISO strings.
/// None when the bytes are not a well-formed message.
pub fn decode(b: &[u8]) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    decode_into(b, "", &mut out, 0)?;
    Some(out)
}

fn decode_into(b: &[u8], prefix: &str, out: &mut Vec<(String, String)>, depth: usize) -> Option<()> {
    let mut i = 0;
    if b.is_empty() || depth > 6 {
        return None;
    }
    while i < b.len() {
        let key = varint(b, &mut i)?;
        let (field, wire) = (key >> 3, key & 7);
        if field == 0 {
            return None;
        }
        let name = if prefix.is_empty() { field.to_string() } else { format!("{prefix}.{field}") };
        let value = match wire {
            0 => varint(b, &mut i)?.to_string(),
            1 => {
                let raw = b.get(i..i + 8)?;
                i += 8;
                let v = f64::from_le_bytes(raw.try_into().ok()?);
                // 2001-2100 in Apple-epoch seconds: a timestamp.
                if (0.0..3.2e9).contains(&v) && v.fract() != 0.0 && v > 1e8 {
                    let ms = (v * 1000.0 + APPLE_EPOCH_MS) as i64;
                    format!("t:{ms}")
                } else if v.is_finite() && v.abs() < 1e15 {
                    v.to_string()
                } else {
                    u64::from_le_bytes(raw.try_into().ok()?).to_string()
                }
            }
            2 => {
                let n = varint(b, &mut i)? as usize;
                let s = b.get(i..i + n)?;
                i += n;
                match std::str::from_utf8(s) {
                    Ok(t) if !t.is_empty() && t.chars().all(|c| !c.is_control() || c == '\n' || c == '\t') => t.chars().take(1024).collect(),
                    _ => {
                        let mut nested = Vec::new();
                        if decode_into(s, &name, &mut nested, depth + 1).is_some() {
                            out.extend(nested);
                            continue;
                        }
                        s.iter().take(64).map(|x| format!("{x:02x}")).collect()
                    }
                }
            }
            5 => {
                let raw = b.get(i..i + 4)?;
                i += 4;
                f32::from_le_bytes(raw.try_into().ok()?).to_string()
            }
            _ => return None,
        };
        out.push((name, value));
    }
    Some(())
}

pub fn parse_segb(file: &[u8]) -> Vec<Event> {
    if file.len() < DATA_START || &file[..4] != b"SEGB" {
        return vec![];
    }
    let count = u32_at(file, 4).unwrap_or(0) as usize;
    let mut out = Vec::new();
    let mut start = DATA_START;
    for i in 0..count {
        let Some(t) = file.len().checked_sub(16 * (i + 1)) else { break };
        if t < start {
            break;
        }
        let (Some(end), Some(state)) = (u32_at(file, t), u32_at(file, t + 4)) else { break };
        let end = DATA_START + end as usize;
        if end > t || end < start {
            break;
        }
        if state == 1 && end >= start + 8 {
            if let Some(e) = parse_record(&file[start + 8..end]) {
                out.push(e);
            }
        }
        start = (end + 3) & !3;
    }
    out
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let x = *b.get(*i)?;
        *i += 1;
        v |= u64::from(x & 0x7f) << shift;
        if x & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn parse_record(b: &[u8]) -> Option<Event> {
    let (mut i, mut bundle, mut starting, mut at) = (0, None, None, None);
    while i < b.len() {
        let key = varint(b, &mut i)?;
        let (field, wire) = (key >> 3, key & 7);
        match wire {
            0 => {
                let v = varint(b, &mut i)?;
                if field == 3 {
                    starting = Some(v == 1);
                }
            }
            1 => {
                let v = f64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?);
                i += 8;
                if field == 4 {
                    at = Some((v * 1000.0 + APPLE_EPOCH_MS) as i64);
                }
            }
            2 => {
                let n = varint(b, &mut i)? as usize;
                let s = b.get(i..i + n)?;
                i += n;
                if field == 6 {
                    bundle = Some(String::from_utf8_lossy(s).into_owned());
                }
            }
            5 => i += 4,
            _ => return None,
        }
    }
    Some(Event { bundle_id: bundle?, starting: starting?, at_ms: at? })
}

/// Longest believable stretch in one app; longer means a lost "left" event.
pub const MAX_BLOCK_MS: i64 = 4 * 3600 * 1000;

/// (bundle id, start, stop) stretches from focus events, in time order.
pub fn blocks(mut events: Vec<Event>) -> Vec<(String, i64, i64)> {
    events.sort_by_key(|e| (e.at_ms, e.starting));
    let mut out = Vec::new();
    let mut open: Option<(String, i64)> = None;
    let mut close = |open: &mut Option<(String, i64)>, at: i64| {
        if let Some((b, s)) = open.take()
            && at > s
            && at - s <= MAX_BLOCK_MS
        {
            out.push((b, s, at));
        }
    };
    for e in events {
        if e.starting {
            close(&mut open, e.at_ms);
            open = Some((e.bundle_id, e.at_ms));
        } else if open.as_ref().is_some_and(|(b, _)| *b == e.bundle_id) {
            close(&mut open, e.at_ms);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(bundle: &str, starting: bool, apple_secs: f64) -> Vec<u8> {
        let mut p = vec![0, 0, 0, 0, 0x0a, 0, 0, 0, 0x10, 1, 0x18, starting as u8, 0x21];
        p.extend(apple_secs.to_le_bytes());
        p.push(0x32);
        p.push(bundle.len() as u8);
        p.extend(bundle.as_bytes());
        p.extend([0x58, 1, 0x60, 1, 0x68, 0]);
        p
    }

    fn file(records: &[Vec<u8>]) -> Vec<u8> {
        let mut f = b"SEGB".to_vec();
        f.extend((records.len() as u32).to_le_bytes());
        f.resize(DATA_START, 0);
        let mut trailer = Vec::new();
        for r in records {
            f.extend(r);
            let end = (f.len() - DATA_START) as u32;
            while f.len() % 4 != 0 {
                f.push(0);
            }
            let mut t = end.to_le_bytes().to_vec();
            t.extend(1u32.to_le_bytes());
            t.extend(0f64.to_le_bytes());
            trailer.push(t);
        }
        f.resize(4096 - 16 * records.len(), 0);
        for t in trailer.into_iter().rev() {
            f.extend(t);
        }
        f
    }

    #[test]
    fn reads_records_and_builds_blocks() {
        let f = file(&[
            record("com.tdesktop.Telegram", false, 100.0),
            record("com.apple.Safari", true, 100.0),
            record("com.apple.Safari", false, 160.0),
            record("com.apple.Terminal", true, 160.0),
            record("com.apple.Terminal", false, 190.5),
        ]);
        let ev = parse_segb(&f);
        assert_eq!(ev.len(), 5);
        assert_eq!(ev[1], Event { bundle_id: "com.apple.Safari".into(), starting: true, at_ms: 978_307_300_000 });
        assert_eq!(
            blocks(ev),
            vec![
                ("com.apple.Safari".into(), 978_307_300_000, 978_307_360_000),
                ("com.apple.Terminal".into(), 978_307_360_000, 978_307_390_500),
            ]
        );
    }

    #[test]
    fn decodes_generically() {
        let f = file(&[record("com.apple.Safari", true, 100.5)]);
        let recs = records(&f);
        assert_eq!(recs.len(), 1);
        let d = decode(recs[0].1).unwrap();
        assert!(d.contains(&("6".into(), "com.apple.Safari".into())));
        assert!(d.contains(&("3".into(), "1".into())));
        assert!(d.iter().any(|(k, _)| k == "4"));
        // nested message
        let msg = [0x12, 0x03, 0x08, 0x96, 0x01];
        assert_eq!(decode(&msg).unwrap(), vec![("2.1".into(), "150".into())]);
    }

    #[test]
    fn ignores_garbage() {
        assert!(parse_segb(b"nope").is_empty());
        assert!(parse_segb(&[0u8; 64]).is_empty());
    }
}
