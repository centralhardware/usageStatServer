use std::cell::RefCell;
use usage_agent::queue::{Queue, SendError};
use usage_model::UsageBlock;

fn block(start: i64) -> UsageBlock {
    UsageBlock {
        device_id: "mac".into(), kind: "app".into(), bundle_id: "a".into(), app_name: "A".into(),
        title: String::new(), start_at: start, stop_at: start + 5_000,
    }
}

fn queue(name: &str) -> Queue {
    let path = std::env::temp_dir().join(format!("usage-agent-test-{name}-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    Queue::new(path)
}

#[test]
fn flush_sends_queue_and_open_block_together() {
    let q = queue("together");
    q.push(&block(1));
    q.push(&block(2));
    let sent = RefCell::new(Vec::new());
    q.flush(Some(&block(3)), |b| {
        sent.borrow_mut().push(b.iter().map(|x| x.start_at).collect::<Vec<_>>());
        Ok(())
    })
    .unwrap();
    assert_eq!(*sent.borrow(), vec![vec![1, 2, 3]]);
    assert!(q.is_empty());
}

#[test]
fn retry_keeps_everything_and_permanent_drops() {
    let q = queue("retry");
    q.push(&block(1));
    assert!(q.flush(None, |_| Err(SendError::Retry("down".into()))).is_err());
    assert!(!q.is_empty());
    assert!(q.flush(None, |_| Err(SendError::Permanent("bad".into()))).is_ok());
    assert!(q.is_empty());
}

#[test]
fn status_codes() {
    assert_eq!(SendError::from_status(503, "x".into()), SendError::Retry("x".into()));
    assert_eq!(SendError::from_status(401, "x".into()), SendError::Retry("x".into()));
    assert_eq!(SendError::from_status(422, "x".into()), SendError::Permanent("x".into()));
}
