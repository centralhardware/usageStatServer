use std::cell::RefCell;
use usage_agent::queue::{Item, Queue, SendError};
use usage_model::UsageBlock;

fn block(start: i64) -> UsageBlock {
    UsageBlock {
        device_id: "mac".into(), device_name: "Mac".into(), kind: "app".into(), bundle_id: "a".into(), app_name: "A".into(),
        title: String::new(), domain: String::new(), app_category: String::new(), start_at: start, stop_at: start + 5_000, ..Default::default()
    }
}

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("usage-agent-test-{name}-{}.jsonl", std::process::id()))
}

fn queue(name: &str) -> Queue {
    let _ = std::fs::remove_file(path(name));
    Queue::new(path(name))
}

#[test]
fn flush_sends_queue_and_open_block_together() {
    let q = queue("together");
    q.push(&Item::Usage(block(1)));
    q.push(&Item::Usage(block(2)));
    // A line from before media existed: a plain block.
    std::fs::OpenOptions::new().append(true).open(path("together")).map(|mut f| { use std::io::Write; writeln!(f, "{}", serde_json::to_string(&block(4)).unwrap()) }).unwrap().unwrap();
    let sent = RefCell::new(Vec::new());
    q.flush(vec![Item::Usage(block(3))], |b| {
        sent.borrow_mut().push(b.blocks.iter().map(|x| x.start_at).collect::<Vec<_>>());
        Ok(())
    })
    .unwrap();
    assert_eq!(*sent.borrow(), vec![vec![1, 2, 4, 3]]);
    assert!(q.is_empty());
}

#[test]
fn retry_keeps_everything_and_permanent_drops() {
    let q = queue("retry");
    q.push(&Item::Usage(block(1)));
    assert!(q.flush(vec![], |_| Err(SendError::Retry("down".into()))).is_err());
    assert!(!q.is_empty());
    assert!(q.flush(vec![], |_| Err(SendError::Permanent("bad".into()))).is_ok());
    assert!(q.is_empty());
}

#[test]
fn status_codes() {
    assert_eq!(SendError::from_status(503, "x".into()), SendError::Retry("x".into()));
    assert_eq!(SendError::from_status(401, "x".into()), SendError::Retry("x".into()));
    assert_eq!(SendError::from_status(422, "x".into()), SendError::Permanent("x".into()));
}
