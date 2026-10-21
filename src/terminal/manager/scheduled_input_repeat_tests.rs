use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::{
    ScheduledInputUpdate, StoredTerminalScheduledInputRegistry, TerminalManager,
    TerminalScheduledInputTask,
};
use super::scheduled_input::{
    claim_and_persist_due_scheduled_input_tasks, claim_due_scheduled_input_tasks,
    load_terminal_scheduled_input_tasks, persist_terminal_scheduled_input_registry,
    prune_completed_scheduled_input_tasks, scheduled_input_task_info,
    validate_scheduled_input_repeat_interval,
};
use crate::terminal::tests::stored_test_terminal_session;

fn repeat_test_manager() -> (TerminalManager, PathBuf) {
    let unique = format!(
        "webclx-scheduled-repeat-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let base_dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&base_dir).expect("temp base dir");
    let manager = TerminalManager::new(base_dir.join("terminal-state.json"));
    (manager, base_dir)
}

fn blocked_registry_file(manager: &mut TerminalManager, base_dir: &PathBuf) {
    let blocker = base_dir.join("blocker");
    std::fs::write(&blocker, b"not-a-directory").unwrap();
    manager.scheduled_input_file = Arc::new(blocker.join("scheduled-inputs.json"));
}

fn repeat_test_task(
    id: &str,
    due_at_millis: u64,
    repeat_interval_millis: u64,
) -> TerminalScheduledInputTask {
    TerminalScheduledInputTask {
        status: String::new(),
        last_error: String::new(),
        id: id.to_string(),
        session_id: "s1".to_string(),
        terminal_name: String::new(),
        due_at_millis,
        created_at_millis: 1,
        label: String::new(),
        text: format!("body-{id}"),
        send_enter: false,
        task_type: String::new(),
        working_dir: String::new(),
        repeat_interval_millis,
        parent_task_id: String::new(),
    }
}

#[test]
fn repeat_interval_validation_bounds() {
    for ok in [0, 1_000, 60_000, 9_007_199_254_740_991] {
        validate_scheduled_input_repeat_interval(ok)
            .unwrap_or_else(|error| panic!("interval {ok} must be accepted: {error}"));
    }
    for bad in [1, 999, 9_007_199_254_740_992, u64::MAX] {
        assert!(
            validate_scheduled_input_repeat_interval(bad).is_err(),
            "interval {bad} must be rejected"
        );
    }
}

#[test]
fn create_scheduled_input_validates_repeat_before_session_lookup() {
    let (manager, base_dir) = repeat_test_manager();
    let now = terminal_core::current_timestamp_millis();
    let error = manager
        .create_scheduled_input(
            "missing-session",
            "body".to_string(),
            now + 60_000,
            String::new(),
            false,
            "paste".to_string(),
            String::new(),
            500,
        )
        .expect_err("sub-second repeat must be rejected before touching sessions");
    assert!(error.to_string().contains("repeat interval"), "unexpected error: {error}");
    let error = manager
        .create_scheduled_input(
            "missing-session",
            "body".to_string(),
            now + 60_000,
            String::new(),
            false,
            "paste".to_string(),
            String::new(),
            9_007_199_254_740_992,
        )
        .expect_err("above-max repeat must be rejected");
    assert!(error.to_string().contains("repeat interval"));
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn recurring_task_persists_and_survives_reload() {
    let (manager, base_dir) = repeat_test_manager();
    {
        let mut state = manager.state.write().expect("state lock");
        state
            .sessions_by_id
            .insert("s1".to_string(), stored_test_terminal_session("s1"));
    }
    let now = terminal_core::current_timestamp_millis();
    let created = manager
        .create_scheduled_input(
            "s1",
            "body".to_string(),
            now + 60_000,
            String::new(),
            false,
            "paste".to_string(),
            String::new(),
            300_000,
        )
        .expect("recurring scheduled input should be created");
    assert_eq!(created.repeat_interval_millis, 300_000);
    assert_eq!(created.parent_task_id, "");

    let reloaded = TerminalManager::new(base_dir.join("terminal-state.json"));
    let tasks = reloaded.list_scheduled_inputs();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, created.task_id);
    assert_eq!(tasks[0].repeat_interval_millis, 300_000);
    assert_eq!(tasks[0].parent_task_id, "");
    assert_eq!(tasks[0].status, "queued");
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn update_scheduled_input_sets_and_clears_repeat() {
    let (manager, base_dir) = repeat_test_manager();
    let parent = repeat_test_task("parent", terminal_core::current_timestamp_millis() + 60_000, 0);
    manager
        .scheduled_input_tasks
        .lock()
        .unwrap()
        .insert(parent.id.clone(), parent);

    let error = manager
        .update_scheduled_input(
            "parent",
            ScheduledInputUpdate {
                repeat_interval_millis: Some(500),
                ..ScheduledInputUpdate::default()
            },
        )
        .expect_err("sub-second repeat update must be rejected");
    assert!(error.to_string().contains("repeat interval"));

    let updated = manager
        .update_scheduled_input(
            "parent",
            ScheduledInputUpdate {
                repeat_interval_millis: Some(120_000),
                ..ScheduledInputUpdate::default()
            },
        )
        .expect("repeat update should succeed");
    assert_eq!(updated.repeat_interval_millis, 120_000);

    let cleared = manager
        .update_scheduled_input(
            "parent",
            ScheduledInputUpdate {
                repeat_interval_millis: Some(0),
                ..ScheduledInputUpdate::default()
            },
        )
        .expect("repeat=0 must disable recurrence");
    assert_eq!(cleared.repeat_interval_millis, 0);
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn update_and_cancel_fail_closed_when_registry_write_fails() {
    let (mut manager, base_dir) = repeat_test_manager();
    let parent =
        repeat_test_task("parent", terminal_core::current_timestamp_millis() + 60_000, 60_000);
    manager
        .scheduled_input_tasks
        .lock()
        .unwrap()
        .insert(parent.id.clone(), parent);
    blocked_registry_file(&mut manager, &base_dir);

    let error = manager
        .update_scheduled_input(
            "parent",
            ScheduledInputUpdate {
                repeat_interval_millis: Some(120_000),
                ..ScheduledInputUpdate::default()
            },
        )
        .expect_err("update must fail when the registry cannot be written");
    assert!(
        error.downcast_ref::<std::io::Error>().is_some(),
        "expected an io error from the blocked registry path, got {error:#}"
    );
    {
        let tasks = manager.scheduled_input_tasks.lock().unwrap();
        assert_eq!(tasks["parent"].repeat_interval_millis, 60_000);
        assert_eq!(tasks["parent"].status, "");
    }

    let error = manager
        .cancel_scheduled_input("parent")
        .expect_err("cancel must fail when the registry cannot be written");
    assert!(
        error.downcast_ref::<std::io::Error>().is_some(),
        "expected an io error from the blocked registry path, got {error:#}"
    );
    {
        let tasks = manager.scheduled_input_tasks.lock().unwrap();
        assert!(tasks.contains_key("parent"));
        assert_eq!(tasks["parent"].repeat_interval_millis, 60_000);
    }
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn cancelled_parent_does_not_resurrect_after_reload() {
    let (manager, base_dir) = repeat_test_manager();
    let parent =
        repeat_test_task("parent", terminal_core::current_timestamp_millis() + 60_000, 60_000);
    {
        let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
        tasks.insert(parent.id.clone(), parent);
        persist_terminal_scheduled_input_registry(
            manager.scheduled_input_file.as_path(),
            &StoredTerminalScheduledInputRegistry {
                tasks: tasks.values().cloned().collect(),
            },
        )
        .expect("seed registry write");
    }

    let removed = manager
        .cancel_scheduled_input("parent")
        .expect("cancel should succeed");
    assert!(removed);

    let tasks = load_terminal_scheduled_input_tasks(manager.scheduled_input_file.as_path());
    assert!(
        !tasks.contains_key("parent"),
        "cancelled recurring parent must not survive a reload"
    );
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn old_registry_without_repeat_fields_defaults_to_one_shot() {
    let (manager, base_dir) = repeat_test_manager();
    let legacy_json = serde_json::json!({
        "tasks": [{
            "status": "",
            "last_error": "",
            "id": "legacy-1",
            "session_id": "s1",
            "terminal_name": "",
            "due_at_millis": 1000,
            "created_at_millis": 1,
            "label": "",
            "text": "legacy",
            "send_enter": false,
            "task_type": "",
            "working_dir": ""
        }]
    });
    std::fs::write(
        manager.scheduled_input_file.as_ref(),
        serde_json::to_vec(&legacy_json).unwrap(),
    )
    .unwrap();
    let tasks = load_terminal_scheduled_input_tasks(manager.scheduled_input_file.as_path());
    let legacy = &tasks["legacy-1"];
    assert_eq!(legacy.repeat_interval_millis, 0);
    assert_eq!(legacy.parent_task_id, "");
    assert_eq!(legacy.status, "");
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn claim_dispatches_one_shot_unchanged() {
    let mut tasks = HashMap::new();
    let now = terminal_core::current_timestamp_millis();
    tasks.insert("once".to_string(), repeat_test_task("once", now - 1000, 0));
    let due = claim_due_scheduled_input_tasks(&mut tasks, now, None).expect("claim should succeed");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "once");
    assert_eq!(due[0].repeat_interval_millis, 0);
    assert_eq!(tasks["once"].status, "dispatching");
    assert_eq!(tasks.len(), 1, "one-shot claim must not create occurrences");
}

#[test]
fn claim_recurring_parent_spawns_child_and_advances() {
    let mut tasks = HashMap::new();
    let now = terminal_core::current_timestamp_millis();
    let due_at = now - 1000;
    tasks.insert("parent".to_string(), repeat_test_task("parent", due_at, 60_000));
    let due = claim_due_scheduled_input_tasks(&mut tasks, now, None).expect("claim should succeed");
    assert_eq!(due.len(), 1);
    let child = &due[0];
    assert_ne!(child.id, "parent");
    assert_eq!(child.parent_task_id, "parent");
    assert_eq!(child.repeat_interval_millis, 0);
    assert_eq!(child.status, "dispatching");
    assert_eq!(child.due_at_millis, due_at);
    assert_eq!(child.text, "body-parent");
    let parent = &tasks["parent"];
    assert_eq!(parent.status, "", "parent must stay queued, never dispatching");
    assert_eq!(parent.due_at_millis, due_at + 60_000);
    assert!(parent.due_at_millis > now);
    assert!(tasks.contains_key(&child.id));
    let info = scheduled_input_task_info(child);
    assert_eq!(info.parent_task_id, "parent");
    assert_eq!(info.repeat_interval_millis, 0);
}

#[test]
fn consecutive_firings_get_distinct_child_ids_and_markers() {
    let mut tasks = HashMap::new();
    let start = terminal_core::current_timestamp_millis();
    tasks.insert("parent".to_string(), repeat_test_task("parent", start, 60_000));

    let first = claim_due_scheduled_input_tasks(&mut tasks, start, None)
        .expect("first claim should succeed");
    assert_eq!(first.len(), 1);

    let second_now = tasks["parent"].due_at_millis;
    let second = claim_due_scheduled_input_tasks(&mut tasks, second_now, None)
        .expect("second claim should succeed");
    assert_eq!(second.len(), 1);
    assert_ne!(first[0].id, second[0].id);
    assert_eq!(second[0].parent_task_id, "parent");
    assert_ne!(
        format!("[scheduled:{}]", first[0].id),
        format!("[scheduled:{}]", second[0].id),
        "each occurrence must carry a distinct delivery marker"
    );
}

#[test]
fn reloaded_occurrence_id_is_never_reused() {
    let (manager, base_dir) = repeat_test_manager();
    let parent = repeat_test_task("parent", 1000, 60_000);
    let mut old_child = repeat_test_task("parent-fire-aaaabbbb", 1000, 0);
    old_child.status = "confirmed".into();
    old_child.parent_task_id = "parent".into();
    let registry = StoredTerminalScheduledInputRegistry {
        tasks: vec![parent, old_child],
    };
    std::fs::write(manager.scheduled_input_file.as_ref(), serde_json::to_vec(&registry).unwrap())
        .unwrap();

    let mut tasks = load_terminal_scheduled_input_tasks(manager.scheduled_input_file.as_path());
    assert!(tasks.contains_key("parent-fire-aaaabbbb"));
    let due = claim_due_scheduled_input_tasks(&mut tasks, 1000, None).expect("claim after reload");
    assert_eq!(due.len(), 1);
    assert_ne!(due[0].id, "parent-fire-aaaabbbb");
    assert_eq!(due[0].parent_task_id, "parent");
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn missed_periods_produce_single_occurrence_and_next_due_beyond_now() {
    let mut tasks = HashMap::new();
    let start = 1_000_000_000u64;
    let interval = 60_000u64;
    tasks.insert("parent".to_string(), repeat_test_task("parent", start, interval));
    let now = start + 5 * interval + 500;
    let due = claim_due_scheduled_input_tasks(&mut tasks, now, None).expect("claim should succeed");
    assert_eq!(due.len(), 1, "missed periods must not burst catchup deliveries");
    assert_eq!(due[0].due_at_millis, start);
    let parent = &tasks["parent"];
    assert_eq!(parent.due_at_millis, start + 6 * interval);
    assert!(parent.due_at_millis > now);
    assert_eq!(parent.status, "");
}

#[test]
fn malformed_stored_repeat_interval_fails_claim_closed() {
    let mut tasks = HashMap::new();
    let now = terminal_core::current_timestamp_millis();
    tasks.insert("corrupt".to_string(), repeat_test_task("corrupt", now - 1000, 500));
    tasks.insert("once".to_string(), repeat_test_task("once", now - 500, 0));
    let snapshot = tasks.clone();
    let error = claim_due_scheduled_input_tasks(&mut tasks, now, None)
        .expect_err("malformed stored repeat interval must fail the claim");
    assert!(error.to_string().contains("repeat interval"));
    assert_eq!(tasks.len(), snapshot.len());
    for (id, before) in &snapshot {
        let after = &tasks[id];
        assert_eq!(after.status, before.status, "{id} status must be unchanged");
        assert_eq!(after.due_at_millis, before.due_at_millis);
    }
    assert_eq!(tasks["once"].status, "", "sibling one-shot must not be dispatched");
}

#[test]
fn claim_overflow_leaves_every_task_unchanged() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "parent".to_string(),
        repeat_test_task("parent", u64::MAX - 10, 9_007_199_254_740_991),
    );
    tasks.insert("once".to_string(), repeat_test_task("once", u64::MAX - 20, 0));
    let snapshot = tasks.clone();
    let error = claim_due_scheduled_input_tasks(&mut tasks, u64::MAX, None)
        .expect_err("overflowing next due must fail the claim");
    assert!(error.to_string().contains("repeat"));
    assert_eq!(tasks.len(), snapshot.len());
    for (id, before) in &snapshot {
        let after = &tasks[id];
        assert_eq!(after.status, before.status, "{id} status must be restored");
        assert_eq!(after.due_at_millis, before.due_at_millis, "{id} due_at must be unchanged");
        assert_eq!(after.text, before.text);
    }
}

#[test]
fn persist_failure_rolls_back_claim_without_touching_terminals() {
    let mut tasks = HashMap::new();
    let now = terminal_core::current_timestamp_millis();
    tasks.insert("parent".to_string(), repeat_test_task("parent", now - 1000, 60_000));
    tasks.insert("once".to_string(), repeat_test_task("once", now - 500, 0));
    let snapshot = tasks.clone();

    let persisted = Arc::new(AtomicBool::new(false));
    let persisted_flag = persisted.clone();
    let error =
        claim_and_persist_due_scheduled_input_tasks(&mut tasks, now, None, move |_snapshot| {
            persisted_flag.store(true, Ordering::SeqCst);
            anyhow::bail!("injected registry write failure")
        })
        .expect_err("failed persist must surface as an error");
    assert!(persisted.load(Ordering::SeqCst));
    assert!(error.to_string().contains("injected"));
    assert_eq!(tasks.len(), snapshot.len());
    for (id, before) in &snapshot {
        let after = &tasks[id];
        assert_eq!(after.status, before.status, "{id} must keep its status");
        assert_eq!(after.due_at_millis, before.due_at_millis);
        assert_eq!(after.parent_task_id, before.parent_task_id);
    }
}

#[test]
fn claim_respects_avoid_window_agent_bypass() {
    let mut tasks = HashMap::new();
    let now = terminal_core::current_timestamp_millis();
    let mut agent_parent = repeat_test_task("agent-parent", now - 1000, 60_000);
    agent_parent.task_type = "agent_message".into();
    let mut normal = repeat_test_task("normal", now - 1000, 0);
    normal.task_type = "paste".into();
    tasks.insert(agent_parent.id.clone(), agent_parent);
    tasks.insert(normal.id.clone(), normal);
    let due = claim_due_scheduled_input_tasks(&mut tasks, now, Some(now + 3_600_000))
        .expect("claim should succeed");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].parent_task_id, "agent-parent");
    assert_eq!(due[0].task_type, "agent_message");
    assert_eq!(tasks["normal"].status, "", "deferred tasks stay queued");
    let due_unrestricted = claim_due_scheduled_input_tasks(&mut tasks, now, None).expect("claim");
    assert_eq!(due_unrestricted.len(), 1);
    assert_eq!(due_unrestricted[0].id, "normal");
}

#[test]
fn parent_edit_and_cancel_only_affect_future_occurrences() {
    let (manager, base_dir) = repeat_test_manager();
    let start = terminal_core::current_timestamp_millis();
    let parent = repeat_test_task("parent", start, 60_000);
    {
        let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
        tasks.insert(parent.id.clone(), parent);
    }
    let first_child = {
        let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
        let due = claim_due_scheduled_input_tasks(&mut tasks, start, None).expect("first claim");
        assert_eq!(due.len(), 1);
        due[0].id.clone()
    };

    manager
        .update_scheduled_input(
            "parent",
            ScheduledInputUpdate {
                text: Some("edited-body".to_string()),
                ..ScheduledInputUpdate::default()
            },
        )
        .expect("parent edit should succeed");

    let second_now = {
        let tasks = manager.scheduled_input_tasks.lock().unwrap();
        tasks["parent"].due_at_millis
    };
    {
        let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
        let due =
            claim_due_scheduled_input_tasks(&mut tasks, second_now, None).expect("second claim");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].text, "edited-body", "edit must shape future occurrences");
        assert_ne!(due[0].id, first_child);
        assert_eq!(tasks[&first_child].text, "body-parent");
        assert_eq!(tasks[&first_child].status, "dispatching");
    }

    let removed = manager
        .cancel_scheduled_input("parent")
        .expect("cancel should succeed");
    assert!(removed);
    {
        let tasks = manager.scheduled_input_tasks.lock().unwrap();
        assert!(!tasks.contains_key("parent"));
        assert!(tasks.contains_key(&first_child), "in-flight child is independent");
        let due = claim_due_scheduled_input_tasks(&mut tasks.clone(), second_now + 60_000, None)
            .expect("claim after cancel");
        assert!(due.is_empty(), "cancelled parent must not produce more occurrences");
    }
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn restart_marks_dispatching_child_unknown_without_replay_and_keeps_parent() {
    let (manager, base_dir) = repeat_test_manager();
    let mut parent = repeat_test_task("parent", 1000, 60_000);
    parent.status = String::new();
    let mut child = repeat_test_task("parent-fire-0123", 1000, 0);
    child.status = "dispatching".into();
    child.parent_task_id = "parent".into();
    let registry = StoredTerminalScheduledInputRegistry {
        tasks: vec![parent, child],
    };
    std::fs::write(manager.scheduled_input_file.as_ref(), serde_json::to_vec(&registry).unwrap())
        .unwrap();

    let tasks = load_terminal_scheduled_input_tasks(manager.scheduled_input_file.as_path());
    assert_eq!(tasks["parent"].status, "");
    assert_eq!(tasks["parent"].repeat_interval_millis, 60_000);
    let child = &tasks["parent-fire-0123"];
    assert_eq!(child.status, "unknown");
    assert!(child.last_error.contains("restarted"));
    assert_eq!(child.parent_task_id, "parent");

    let mut claimed = tasks.clone();
    let due =
        claim_due_scheduled_input_tasks(&mut claimed, 1000, None).expect("claim after restart");
    assert_eq!(
        due.len(),
        1,
        "unknown child must never replay; only the parent's next planned occurrence fires"
    );
    assert_ne!(due[0].id, "parent-fire-0123");
    assert_eq!(due[0].parent_task_id, "parent");
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn pruning_completed_occurrences_keeps_queued_parent() {
    let mut tasks = HashMap::new();
    tasks.insert("parent".to_string(), repeat_test_task("parent", 500, 60_000));
    for index in 0..35 {
        let mut child = repeat_test_task(&format!("child-{index}"), 1000 + index, 0);
        child.status = "confirmed".into();
        child.parent_task_id = "parent".into();
        tasks.insert(child.id.clone(), child);
    }
    prune_completed_scheduled_input_tasks(&mut tasks);
    assert!(tasks.contains_key("parent"), "queued recurring parent is never pruned");
    assert_eq!(tasks["parent"].status, "");
    assert!(tasks.len() <= 31);
}
