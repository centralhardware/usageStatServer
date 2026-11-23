//! Discovery for Screen Time data: what macOS stores locally, before we decide what to send.
//! Both stores need Full Disk Access for whatever process reads them.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Seconds between the Unix epoch and Apple's Core Data epoch (2001-01-01).
const APPLE_EPOCH: f64 = 978_307_200.0;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn fmt_ts(unix: f64) -> String {
    let secs = unix as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

pub fn dump() {
    dump_knowledge();
    println!();
    dump_biome();
}

fn dump_knowledge() {
    let path = home().join("Library/Application Support/Knowledge/knowledgeC.db");
    println!("== knowledgeC: {}", path.display());
    let db = match rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(db) => db,
        Err(e) => return println!("  cannot open: {e} (Full Disk Access?)"),
    };
    let streams = db.prepare(
        "SELECT ZSTREAMNAME, COUNT(*), MIN(ZSTARTDATE), MAX(ZENDDATE) FROM ZOBJECT GROUP BY ZSTREAMNAME ORDER BY 2 DESC",
    );
    match streams {
        Err(e) => println!("  query failed: {e} (Full Disk Access?)"),
        Ok(mut q) => {
            let rows = q.query_map([], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<f64>>(2)?, r.get::<_, Option<f64>>(3)?))
            });
            for (name, n, lo, hi) in rows.into_iter().flatten().flatten() {
                let span = match (lo, hi) {
                    (Some(lo), Some(hi)) => format!("{} .. {}", fmt_ts(lo + APPLE_EPOCH), fmt_ts(hi + APPLE_EPOCH)),
                    _ => String::new(),
                };
                println!("  {:<40} {n:>7}  {span}", name.unwrap_or_default());
            }
        }
    }
    let devices = db.prepare(
        "SELECT IFNULL(s.ZDEVICEID, '(this mac)'), COUNT(*), MAX(o.ZENDDATE) FROM ZOBJECT o \
         LEFT JOIN ZSOURCE s ON o.ZSOURCE = s.Z_PK WHERE o.ZSTREAMNAME = '/app/usage' GROUP BY 1",
    );
    if let Ok(mut q) = devices {
        println!("  -- /app/usage by device");
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<f64>>(2)?)));
        for (dev, n, hi) in rows.into_iter().flatten().flatten() {
            println!("  {dev:<40} {n:>7}  last {}", hi.map(|h| fmt_ts(h + APPLE_EPOCH)).unwrap_or_default());
        }
    }
    let latest = db.prepare(
        "SELECT ZSTREAMNAME, ZVALUESTRING, ZSTARTDATE, ZENDDATE FROM ZOBJECT \
         WHERE ZSTREAMNAME IN ('/app/usage', '/app/webUsage', '/device/isLocked') ORDER BY ZENDDATE DESC LIMIT 10",
    );
    if let Ok(mut q) = latest {
        println!("  -- latest events");
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?))
        });
        for (s, v, a, b) in rows.into_iter().flatten().flatten() {
            println!("  {} {s:<18} {:>6.0}s  {}", fmt_ts(a + APPLE_EPOCH), b - a, v.unwrap_or_default());
        }
    }
}

fn dump_biome() {
    let root = home().join("Library/Biome/streams");
    println!("== Biome: {}", root.display());
    for scope in ["public", "restricted"] {
        let Ok(dir) = std::fs::read_dir(root.join(scope)) else {
            println!("  {scope}: cannot read (Full Disk Access?)");
            continue;
        };
        let mut names: Vec<_> = dir.flatten().map(|e| e.path()).collect();
        names.sort();
        for stream in names {
            let name = stream.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let interesting = ["App", "Device", "Screen", "Safari", "Web", "Notification", "Lock", "Backlight"]
                .iter()
                .any(|k| name.contains(k));
            if !interesting {
                continue;
            }
            let (files, bytes, newest) = scan(&stream.join("local"));
            println!("  {scope}/{name:<36} local: {files} files {bytes} B newest {newest}");
            if let Ok(remote) = std::fs::read_dir(stream.join("remote")) {
                for dev in remote.flatten() {
                    let (f, b, n) = scan(&dev.path());
                    println!("      remote {:<38} {f} files {b} B newest {n}", dev.file_name().to_string_lossy());
                }
            }
        }
    }
}

/// File count, total size and newest mtime under a directory (recursive).
fn scan(dir: &Path) -> (usize, u64, String) {
    let (mut files, mut bytes, mut newest) = (0, 0, SystemTime::UNIX_EPOCH);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                stack.push(e.path());
            } else {
                files += 1;
                bytes += md.len();
                newest = newest.max(md.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            }
        }
    }
    let newest = newest.duration_since(SystemTime::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    (files, bytes, if newest > 0.0 { fmt_ts(newest) } else { "-".into() })
}
