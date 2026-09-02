//! Exercise the real bridge worker/RPC/batch path against a peer that replaces
//! its callback registry on every render, just like runtime/export-tree.
use super::*;
use crate::overlay::{DialogClose, OverlayAction, OverlayCallbacks};
use crate::protocol::{send_callbacks_seq, CallbackCall};
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
             "on-close": self.id("close"), "on-open-change": self.id("dialog-open")},
            {"type": "popover", "id": "hint", "open": self.popover_open,
             "on-open-change": self.id("popover-open")},
            {"type": "dropdown-menu", "id": "edit", "on-change": self.id("menu"),
             "items": [{"id": "copy", "label": "Copy", "on-click": self.id("copy")},
                       {"id": "share", "items": [{"id": "link", "on-click": self.id("link")}]}]},
            {"type": "context-menu", "id": "context", "on-change": self.id("context"),
             "items": [{"id": "inspect", "label": "Inspect"}]}
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
                json!({"op": "ready", "protocol-version": 6,
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

    fn send(&self, queue: &mut OverlayCallbacks, tree: &Node, seq: u64) -> Vec<CallbackCall> {
        let calls = queue
            .next(tree)
            .expect("expected a semantic overlay action");
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
    let mut queue = OverlayCallbacks::default();
    let mut dialog = DialogClose::default();
    assert!(dialog.action(false));
    queue.push(dialog.take("ask").unwrap());
    let dismiss = fixture.send(&mut queue, &tree_a, 1);
    assert_eq!(dismiss.len(), 3);

    // The second native mouse-down reaches an already dismissed dialog.
    assert!(!dialog.action(false));
    assert!(dialog.take("ask").is_none());
    // The next interaction arrives before the dialog Tree is installed.
    let open = OverlayAction::PopoverOpen {
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
    assert!(!fixture
        .peer
        .lock()
        .unwrap()
        .registry
        .contains_key(&old_popover_id));
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
        OverlayAction::MenuSelect {
            key: "edit".into(),
            item_path: vec!["copy".into()],
        },
        OverlayAction::MenuSelect {
            key: "edit".into(),
            item_path: vec!["share".into(), "link".into()],
        },
        OverlayAction::MenuSelect {
            key: "context".into(),
            item_path: vec!["inspect".into()],
        },
    ];
    let mut queue = OverlayCallbacks::default();
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
