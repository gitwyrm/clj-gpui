//! Exercise the real bridge worker/RPC/batch path against a peer that replaces
//! its callback registry on every render, just like runtime/export-tree.
use super::*;
use crate::overlay::{CallbackQueue, DialogClose, QueuedAction};
use crate::protocol::{CallbackCall, send_callbacks_seq};
use std::net::{Shutdown, TcpListener};

#[derive(Default)]
struct RegistryPeer {
    next_id: u64,
    generation: u64,
    registry: HashMap<String, String>,
    fired: Vec<(String, Value, bool)>,
    unknown: Vec<String>,
    dialog_open: bool,
    popover_open: bool,
}

impl RegistryPeer {
    fn id(&mut self, role: &str) -> String {
        self.next_id += 1;
        let id = format!("cb-{}", self.next_id);
        self.registry.insert(id.clone(), role.into());
        id
    }

    fn export_tree(&mut self) -> Value {
        self.generation += 1;
        self.registry.clear();
        json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": self.dialog_open,
             "on-cancel": self.id("cancel"), "on-ok": self.id("ok"),
             "on-close": self.id("close"), "on-open-change": self.id("dialog-open"),
             "children": [
                {"type": "label", "text": "Really?"},
                {"type": "button", "text": "Save", "on-click": self.id("dialog-save")}
             ]},
            {"type": "popover", "id": "hint", "open": self.popover_open,
             "on-open-change": self.id("popover-open"),
             "children": [
                {"type": "label", "text": "Anchored"},
                {"type": "button", "text": "Close", "on-click": self.id("popover-close")}
             ]},
            {"type": "dropdown-menu", "id": "edit", "on-change": self.id("menu"),
             "items": [{"id": "copy", "label": "Copy", "on-click": self.id("copy")},
                       {"id": "share", "items": [{"id": "link", "on-click": self.id("link")}]}]},
            {"type": "context-menu", "id": "context", "on-change": self.id("context"),
             "items": [{"id": "inspect", "label": "Inspect"}]},
            {"type": "button", "id": "rerender", "text": "Rerender",
             "on-click": self.id("rerender")},
            {"type": "sheet", "id": "inspect", "open": true,
             "children": [{"type": "button", "text": "Ping", "on-click": self.id("sheet-body")}],
             "footer": {"type": "button", "text": "Done", "on-click": self.id("sheet-footer")}}
        ]})
    }

    fn request(&mut self, request: &Value) -> Value {
        match request["op"].as_str().unwrap() {
            "render" => json!({"ok": true, "tree": self.export_tree()}),
            "callback" => {
                let id = request["callback-id"].as_str().unwrap();
                let Some(role) = self.registry.get(id).cloned() else {
                    self.unknown.push(id.into());
                    return json!({"ok": false, "error": format!("unknown callback {id}")});
                };
                let value = request.get("value").cloned().unwrap_or(Value::Null);
                match role.as_str() {
                    "cancel" | "ok" => self.dialog_open = false,
                    "popover-open" => self.popover_open = value.as_bool().unwrap(),
                    _ => {}
                }
                self.fired
                    .push((role, value, request["defer-render"] == true));
                json!({"ok": true})
            }
            other => panic!("unexpected request {other}"),
        }
    }
}

struct Fixture {
    host: ClojureHost,
    peer: Arc<Mutex<RegistryPeer>>,
    socket: TcpStream,
    server: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.set_nodelay(true).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.set_nodelay(true).unwrap();
        let socket = server.try_clone().unwrap();
        let peer = Arc::new(Mutex::new(RegistryPeer {
            dialog_open: true,
            ..RegistryPeer::default()
        }));
        let state = peer.clone();
        let task = thread::spawn(move || {
            writeln!(
                server,
                "{}",
                json!({"op": "ready", "protocol-version": 9,
                "nrepl": 0, "app": "overlay-regression"})
            )
            .unwrap();
            let reader = BufReader::new(server.try_clone().unwrap());
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                let mut response = state.lock().unwrap().request(&request);
                response["id"] = request["id"].clone();
                response["op"] = json!("response");
                if writeln!(server, "{response}").is_err() {
                    break;
                }
            }
        });
        Self {
            host: attach(client).unwrap(),
            peer,
            socket,
            server: Some(task),
        }
    }

    fn tree(&self) -> (Node, Option<u64>) {
        loop {
            match self.host.event_rx.recv_blocking().unwrap() {
                HostEvent::Tree(tree, seq, _) => return (tree, seq),
                HostEvent::Error(error) => panic!("bridge error: {error}"),
                HostEvent::Ready { .. } => {}
                other => panic!("unexpected event: {other:?}"),
            }
        }
    }

    fn initial_tree(&self) -> Node {
        self.host.cmd_tx.send(Cmd::Render).unwrap();
        self.tree().0
    }

    fn send(&self, queue: &mut CallbackQueue, tree: &Node, seq: u64) -> Vec<CallbackCall> {
        let calls = queue
            .next(tree)
            .expect("expected a semantic button/overlay action");
        queue.sent(seq);
        send_callbacks_seq(&self.host.cmd_tx, calls.clone(), Some(seq));
        calls
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both);
        self.server.take().unwrap().join().unwrap();
    }
}

#[test]
fn dialog_then_popover_waits_for_generation_and_suppresses_native_echoes() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let old_popover_id = tree_a.children[1].on_open_change.clone().unwrap();
    let mut queue = CallbackQueue::default();
    let mut dialog = DialogClose::default();
    assert!(dialog.action(false));
    queue.push(dialog.take("ask").unwrap());
    let dismiss = fixture.send(&mut queue, &tree_a, 1);
    assert_eq!(dismiss.len(), 3);

    // The second native mouse-down reaches an already dismissed dialog.
    assert!(!dialog.action(false));
    assert!(dialog.take("ask").is_none());
    // The next interaction arrives before the dialog Tree is installed.
    let open = QueuedAction::PopoverOpen {
        key: "hint".into(),
        open: true,
    };
    queue.push(open.clone());
    queue.push(open);
    assert!(queue.next(&tree_a).is_none());
    queue.tree_installed(None);
    queue.tree_installed(Some(999));
    assert!(
        queue.next(&tree_a).is_none(),
        "unrelated trees cannot release the barrier"
    );

    let (tree_b, seq) = fixture.tree();
    assert_eq!(seq, Some(1));
    assert!(
        !fixture
            .peer
            .lock()
            .unwrap()
            .registry
            .contains_key(&old_popover_id)
    );
    queue.tree_installed(seq);
    let open = fixture.send(&mut queue, &tree_b, 2);
    assert_ne!(open[0].id, old_popover_id);
    assert_eq!(
        open[0].id,
        tree_b.children[1].on_open_change.clone().unwrap()
    );

    let (tree_c, seq) = fixture.tree();
    queue.tree_installed(seq);
    assert!(
        queue.next(&tree_c).is_none(),
        "duplicate true is now represented by controlled state"
    );
    assert!(
        dialog.take("ask").is_none(),
        "late close cannot survive into generation C"
    );
    let peer = fixture.peer.lock().unwrap();
    assert_eq!(peer.generation, 3);
    assert!(peer.unknown.is_empty());
    assert_eq!(
        peer.fired,
        vec![
            ("cancel".into(), Value::Null, true),
            ("close".into(), Value::Null, true),
            ("dialog-open".into(), json!(false), true),
            ("popover-open".into(), json!(true), false),
        ]
    );
}

#[test]
fn retained_menu_selection_uses_replacement_registry_and_keeps_batch_order() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let old_copy_id = tree_a.children[2].items[0].on_click.clone().unwrap();
    let retained_actions = [
        QueuedAction::MenuSelect {
            key: "edit".into(),
            item_path: vec!["copy".into()],
        },
        QueuedAction::MenuSelect {
            key: "edit".into(),
            item_path: vec!["share".into(), "link".into()],
        },
        QueuedAction::MenuSelect {
            key: "context".into(),
            item_path: vec!["inspect".into()],
        },
    ];
    let mut queue = CallbackQueue::default();
    for (index, action) in retained_actions.into_iter().enumerate() {
        // The native menu remains open while an unrelated render replaces ids.
        fixture.host.cmd_tx.send(Cmd::Render).unwrap();
        let (tree, seq) = fixture.tree();
        queue.tree_installed(seq);
        queue.push(action);
        let calls = fixture.send(&mut queue, &tree, index as u64 + 1);
        assert!(calls.iter().all(|call| call.id != old_copy_id));
        let (_, seq) = fixture.tree();
        queue.tree_installed(seq);
    }
    let peer = fixture.peer.lock().unwrap();
    assert!(peer.unknown.is_empty());
    assert_eq!(
        peer.fired,
        vec![
            ("copy".into(), Value::Null, true),
            ("menu".into(), json!("copy"), true),
            ("link".into(), Value::Null, true),
            ("menu".into(), json!("link"), true),
            ("context".into(), json!("inspect"), false),
        ]
    );
}

#[test]
fn raw_button_replay_still_fails_closed_after_registry_replacement() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let old_id = tree_a.children[4].on_click.clone().unwrap();
    // The pre-fix button closure enqueued this raw id for both clicks.
    for seq in [1, 2] {
        send_callbacks_seq(
            &fixture.host.cmd_tx,
            vec![CallbackCall::fire(old_id.clone())],
            Some(seq),
        );
        assert_eq!(fixture.tree().1, Some(seq));
    }
    let error = fixture.host.event_rx.recv_blocking().unwrap();
    assert!(matches!(error, HostEvent::Error(message)
        if message == format!("unknown callback {old_id}")));
    let peer = fixture.peer.lock().unwrap();
    assert_eq!(peer.unknown, vec![old_id]);
    assert_eq!(peer.fired.len(), 1);
}

#[test]
fn button_clicks_wait_for_generation_without_losing_distinct_activations() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let old_id = tree_a.children[4].on_click.clone().unwrap();
    let click = QueuedAction::ButtonClick {
        key: "rerender".into(),
    };
    let mut queue = CallbackQueue::default();
    queue.push(click.clone());
    assert_eq!(fixture.send(&mut queue, &tree_a, 1)[0].id, old_id);

    // A second genuine click arrives before the first response is installed.
    queue.push(click.clone());
    assert!(queue.next(&tree_a).is_none());
    let (tree_b, seq) = fixture.tree();
    queue.tree_installed(seq);
    let second = fixture.send(&mut queue, &tree_b, 2);
    assert_ne!(second[0].id, old_id);
    assert_eq!(second[0].id, tree_b.children[4].on_click.clone().unwrap());
    let (_, seq) = fixture.tree();
    queue.tree_installed(seq);

    // A retained painted handler also resolves against an unrelated new tree.
    fixture.host.cmd_tx.send(Cmd::Render).unwrap();
    let (tree_d, seq) = fixture.tree();
    queue.tree_installed(seq);
    queue.push(click);
    let third = fixture.send(&mut queue, &tree_d, 3);
    assert_eq!(third[0].id, tree_d.children[4].on_click.clone().unwrap());
    let (tree_e, seq) = fixture.tree();
    queue.tree_installed(seq);
    assert!(queue.next(&tree_e).is_none());
    let peer = fixture.peer.lock().unwrap();
    assert!(peer.unknown.is_empty());
    assert_eq!(peer.fired, vec![("rerender".into(), Value::Null, false); 3]);
}

#[test]
fn dialog_then_button_shares_the_same_generation_barrier() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let mut queue = CallbackQueue::default();
    queue.push(QueuedAction::DialogClose {
        key: "ask".into(),
        ok: Some(false),
    });
    fixture.send(&mut queue, &tree_a, 1);
    queue.push(QueuedAction::ButtonClick {
        key: "rerender".into(),
    });
    assert!(queue.next(&tree_a).is_none());
    let (tree_b, seq) = fixture.tree();
    queue.tree_installed(seq);
    fixture.send(&mut queue, &tree_b, 2);
    fixture.tree();
    let peer = fixture.peer.lock().unwrap();
    assert!(peer.unknown.is_empty());
    assert_eq!(
        peer.fired,
        vec![
            ("cancel".into(), Value::Null, true),
            ("close".into(), Value::Null, true),
            ("dialog-open".into(), json!(false), true),
            ("rerender".into(), Value::Null, false),
        ]
    );
}

#[test]
fn queued_button_skips_removed_disabled_or_replaced_controls() {
    for tree in [
        json!({"type": "window", "children": []}),
        json!({"type": "button", "id": "rerender", "disabled": true, "on-click": "cb-new"}),
        json!({"type": "label", "id": "rerender", "on-click": "cb-other"}),
        json!({"type": "button", "id": "rerender"}),
    ] {
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::ButtonClick {
            key: "rerender".into(),
        });
        assert!(queue.next(&serde_json::from_value(tree).unwrap()).is_none());
    }
}

#[test]
fn queued_button_paths_match_normal_and_accordion_rendering() {
    let tree: Node = serde_json::from_value(json!({"type": "window", "children": [
        {"type": "button", "on-click": "cb-normal"},
        {"type": "accordion", "items": [{"id": "item", "content": {
            "type": "vstack", "children": [{"type": "button", "on-click": "cb-nested"}]
        }}]}
    ]}))
    .unwrap();
    for (key, expected) in [("root-0", "cb-normal"), ("root-1-acc-0-0", "cb-nested")] {
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::ButtonClick { key: key.into() });
        assert_eq!(queue.next(&tree).unwrap()[0].id, expected);
    }
}

#[test]
fn dialog_close_acknowledgement_survives_a_skipped_paint_before_reopen() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let mut mounted = crate::overlay::dialog_keys(&crate::overlay::collect_open_dialogs(&tree_a));
    crate::overlay::acknowledge_dialog_tree(&mut mounted, &tree_a);
    assert!(crate::overlay::crate_dismiss_waiting_for_clojure(
        &mounted, &mounted, false
    ));

    let mut queue = CallbackQueue::default();
    queue.push(QueuedAction::DialogClose {
        key: "ask".into(),
        ok: Some(false),
    });
    fixture.send(&mut queue, &tree_a, 1);
    let (tree_b, _) = fixture.tree();
    crate::overlay::acknowledge_dialog_tree(&mut mounted, &tree_b);
    assert!(mounted.is_empty());

    // Native paint was skipped for B; the next received tree reopens the key.
    fixture.peer.lock().unwrap().dialog_open = true;
    fixture.host.cmd_tx.send(Cmd::Render).unwrap();
    let (tree_c, _) = fixture.tree();
    crate::overlay::acknowledge_dialog_tree(&mut mounted, &tree_c);
    let wanted = crate::overlay::dialog_keys(&crate::overlay::collect_open_dialogs(&tree_c));
    assert_eq!(wanted, vec!["ask"]);
    assert!(!crate::overlay::crate_dismiss_waiting_for_clojure(
        &wanted, &mounted, false
    ));
    assert!(fixture.peer.lock().unwrap().unknown.is_empty());
}

#[test]
fn retained_static_overlay_buttons_use_replacement_registry() {
    let fixture = Fixture::new();
    let tree_a = fixture.initial_tree();
    let old_dialog = tree_a.children[0].children[1].on_click.clone().unwrap();
    let old_popover = tree_a.children[1].children[1].on_click.clone().unwrap();
    let old_sheet = tree_a.children[5].children[0].on_click.clone().unwrap();
    let old_footer = tree_a.children[5]
        .footer
        .as_ref()
        .unwrap()
        .on_click
        .clone()
        .unwrap();
    let old_ids = [
        old_dialog.clone(),
        old_popover.clone(),
        old_sheet.clone(),
        old_footer.clone(),
    ];
    let retained = [
        ("ask/content/1", "dialog-save"),
        ("hint/content/1", "popover-close"),
        ("inspect/content/0", "sheet-body"),
        ("inspect/footer/0", "sheet-footer"),
    ];

    let mut queue = CallbackQueue::default();
    fixture.host.cmd_tx.send(Cmd::Render).unwrap();
    let (mut tree, seq) = fixture.tree();
    queue.tree_installed(seq);
    assert!(
        !fixture
            .peer
            .lock()
            .unwrap()
            .registry
            .contains_key(&old_dialog)
    );

    for (index, (key, role)) in retained.into_iter().enumerate() {
        queue.push(QueuedAction::ButtonClick { key: key.into() });
        let calls = fixture.send(&mut queue, &tree, index as u64 + 1);
        assert_eq!(calls.len(), 1, "{key}");
        assert!(!old_ids.contains(&calls[0].id), "{key} replayed a stale id");
        let current = match key {
            "ask/content/1" => tree.children[0].children[1].on_click.clone(),
            "hint/content/1" => tree.children[1].children[1].on_click.clone(),
            "inspect/content/0" => tree.children[5].children[0].on_click.clone(),
            "inspect/footer/0" => tree.children[5].footer.as_ref().unwrap().on_click.clone(),
            _ => None,
        };
        assert_eq!(calls[0].id, current.unwrap(), "{key} -> {role}");
        let next = fixture.tree();
        queue.tree_installed(next.1);
        tree = next.0;
    }

    let peer = fixture.peer.lock().unwrap();
    assert!(peer.unknown.is_empty());
    assert_eq!(
        peer.fired,
        vec![
            ("dialog-save".into(), Value::Null, false),
            ("popover-close".into(), Value::Null, false),
            ("sheet-body".into(), Value::Null, false),
            ("sheet-footer".into(), Value::Null, false),
        ]
    );
}

#[test]
fn queued_static_overlay_button_skips_removed_disabled_or_replaced() {
    for tree in [
        json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": true,
             "children": [{"type": "label", "text": "gone"}]}
        ]}),
        json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": true,
             "children": [{"type": "button", "disabled": true, "on-click": "cb-new"}]}
        ]}),
        json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": true,
             "children": [{"type": "button"}]}
        ]}),
        json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": true, "children": []}
        ]}),
    ] {
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::ButtonClick {
            key: "ask/content/0".into(),
        });
        assert!(queue.next(&serde_json::from_value(tree).unwrap()).is_none());
    }
}
