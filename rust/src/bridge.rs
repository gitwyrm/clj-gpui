use crate::protocol::{Cmd, HostEvent, Node};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct ClojureHost {
    pub nrepl_port: u16,
    #[allow(dead_code)]
    pub app: String,
    pub cmd_tx: mpsc::Sender<Cmd>,
    pub event_rx: async_channel::Receiver<HostEvent>,
    child: Option<Child>,
}

impl Drop for ClojureHost {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Shutdown);
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn find_clojure_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CLOJUREGPUI_CLOJURE_DIR") {
        let path = PathBuf::from(path);
        if path.join("deps.edn").exists() {
            return Ok(path.canonicalize().unwrap_or(path));
        }
        bail!("CLOJUREGPUI_CLOJURE_DIR does not contain deps.edn: {}", path.display());
    }

    let mut candidates = Vec::new();
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest).join("..").join("clojure"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("clojure"));
            candidates.push(dir.join("..").join("clojure"));
            candidates.push(dir.join("..").join("..").join("clojure"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("clojure"));
        candidates.push(cwd.join("..").join("clojure"));
    }

    for candidate in candidates {
        if candidate.join("deps.edn").exists() {
            return Ok(candidate.canonicalize().unwrap_or(candidate));
        }
    }

    bail!("Could not find clojure/deps.edn. Set CLOJUREGPUI_CLOJURE_DIR.")
}

fn detect_java_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let home = PathBuf::from(home);
        if home.exists() {
            return Some(home);
        }
    }
    let known = PathBuf::from("/usr/lib/jvm/java-21-openjdk-amd64");
    if known.exists() {
        return Some(known);
    }
    None
}

fn spawn_clojure(clojure_dir: &Path, port: u16) -> Result<Child> {
    let mut cmd = Command::new("clojure");
    cmd.current_dir(clojure_dir)
        .env("CLOJUREGPUI_PORT", port.to_string())
        .env("CLOJUREGPUI_HOST", "127.0.0.1")
        .env("CLOJUREGPUI_APP", std::env::var("CLOJUREGPUI_APP").unwrap_or_else(|_| "demo.app/app".into()))
        .args(["-M", "-m", "gpui.runtime"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(home) = detect_java_home() {
        cmd.env("JAVA_HOME", home);
    }
    cmd.spawn().context("failed to spawn `clojure`. Is the Clojure CLI on PATH?")
}

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>;

fn write_json(stream: &Mutex<TcpStream>, value: &Value) -> Result<()> {
    let mut stream = stream.lock().unwrap();
    writeln!(stream, "{value}")?;
    stream.flush()?;
    Ok(())
}

fn rpc(stream: &Mutex<TcpStream>, pending: &Pending, next_id: &AtomicU64, mut request: Value) -> Result<Value> {
    let id = next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    pending.lock().unwrap().insert(id, tx);
    request["id"] = json!(id);
    write_json(stream, &request)?;
    rx.recv_timeout(Duration::from_secs(30))
        .context("timed out waiting for Clojure to answer")
}

fn parse_tree(value: &Value) -> Result<Node> {
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        let err = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Clojure render failed");
        bail!("{err}");
    }
    let tree = value.get("tree").context("Clojure response missing :tree")?;
    serde_json::from_value(tree.clone()).context("invalid UI tree from Clojure")
}

pub fn start() -> Result<ClojureHost> {
    let clojure_dir = find_clojure_dir()?;
    println!("[host] clojure dir {}", clojure_dir.display());

    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind bridge socket")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(false)?;
    println!("[host] waiting for Clojure on 127.0.0.1:{port}");

    let child = spawn_clojure(&clojure_dir, port)?;

    let (stream, _) = listener
        .accept()
        .context("Clojure runtime never connected. Check that `clojure` starts.")?;
    stream.set_nodelay(true)?;
    println!("[host] Clojure connected");

    let reader_stream = stream.try_clone()?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let (event_tx, event_rx) = async_channel::unbounded::<HostEvent>();
    let (ready_tx, ready_rx) = mpsc::channel::<(u16, String)>();
    let worker_cmds = cmd_tx.clone();

    thread::Builder::new()
        .name("clojuregpui-reader".into())
        .spawn({
            let pending = pending.clone();
            let event_tx = event_tx.clone();
            move || {
                let mut lines = BufReader::new(reader_stream).lines();
                while let Some(Ok(line)) = lines.next() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let value: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(err) => {
                            eprintln!("[host] invalid JSON from Clojure: {err}: {line}");
                            continue;
                        }
                    };
                    let op = value.get("op").and_then(Value::as_str).unwrap_or("");
                    match op {
                        "ready" => {
                            let nrepl = value.get("nrepl").and_then(Value::as_u64).unwrap_or(0) as u16;
                            let app = value
                                .get("app")
                                .and_then(Value::as_str)
                                .unwrap_or("demo.app/app")
                                .to_string();
                            let _ = ready_tx.send((nrepl, app.clone()));
                            let _ = event_tx.send_blocking(HostEvent::Ready {
                                nrepl_port: nrepl,
                                app,
                            });
                        }
                        "request-render" => {
                            let _ = worker_cmds.send(Cmd::Render);
                        }
                        "response" | "" => {
                            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                                if let Some(tx) = pending.lock().unwrap().remove(&id) {
                                    let _ = tx.send(value);
                                }
                            }
                        }
                        other => {
                            eprintln!("[host] ignored Clojure message op={other}");
                        }
                    }
                }
                println!("[host] Clojure socket closed");
            }
        })?;

    let (nrepl_port, app) = ready_rx
        .recv_timeout(Duration::from_secs(180))
        .context("timed out waiting for Clojure :ready (first Maven download can be slow)")?;
    println!("[host] Clojure ready app={app} nREPL=127.0.0.1:{nrepl_port}");

    thread::Builder::new()
        .name("clojuregpui-worker".into())
        .spawn({
            let writer = writer.clone();
            let pending = pending.clone();
            let next_id = next_id.clone();
            let event_tx = event_tx.clone();
            move || {
                while let Ok(cmd) = cmd_rx.recv() {
                    let result = match cmd {
                        Cmd::Shutdown => break,
                        Cmd::Render => rpc(&writer, &pending, &next_id, json!({"op": "render"}))
                            .and_then(|value| parse_tree(&value))
                            .map(HostEvent::Tree),
                        Cmd::Callback(id) => rpc(
                            &writer,
                            &pending,
                            &next_id,
                            json!({"op": "callback", "callback-id": id}),
                        )
                        .and_then(|_| rpc(&writer, &pending, &next_id, json!({"op": "render"})))
                        .and_then(|value| parse_tree(&value))
                        .map(HostEvent::Tree),
                        Cmd::Reload => rpc(&writer, &pending, &next_id, json!({"op": "reload"}))
                            .and_then(|value| parse_tree(&value))
                            .map(HostEvent::Tree),
                    };
                    match result {
                        Ok(event) => {
                            let _ = event_tx.send_blocking(event);
                        }
                        Err(err) => {
                            let _ = event_tx.send_blocking(HostEvent::Error(err.to_string()));
                        }
                    }
                }
            }
        })?;

    Ok(ClojureHost {
        nrepl_port,
        app,
        cmd_tx,
        event_rx,
        child: Some(child),
    })
}

pub fn protocol_test() -> Result<()> {
    println!("[host] running protocol test (no GPUI window)");
    let host = start()?;
    host.cmd_tx.send(Cmd::Render)?;

    let started = Instant::now();
    let mut tree = None;
    while started.elapsed() < Duration::from_secs(30) {
        match host.event_rx.recv_blocking() {
            Ok(HostEvent::Tree(t)) => {
                tree = Some(t);
                break;
            }
            Ok(HostEvent::Ready { .. }) => continue,
            Ok(HostEvent::Error(err)) => bail!("Clojure error: {err}"),
            Err(err) => bail!("bridge closed: {err}"),
        }
    }
    let tree = tree.context("did not receive a UI tree")?;
    println!("[host] milestone 1/2: received Clojure UI tree");
    if !tree.contains_text("ClojureGPUI") {
        bail!("tree did not contain label 'ClojureGPUI': {tree:?}");
    }
    if !tree.contains_text("Count: 0") {
        bail!("tree did not contain initial count: {tree:?}");
    }

    let plus = tree
        .find_button("+")
        .and_then(|node| node.on_click.clone())
        .context("no '+' button with a callback id")?;
    println!("[host] milestone 3: invoking Clojure callback {plus}");
    host.cmd_tx.send(Cmd::Callback(plus))?;

    let started = Instant::now();
    let mut updated = None;
    while started.elapsed() < Duration::from_secs(30) {
        match host.event_rx.recv_blocking() {
            Ok(HostEvent::Tree(t)) => {
                updated = Some(t);
                break;
            }
            Ok(HostEvent::Error(err)) => bail!("Clojure error after click: {err}"),
            Ok(_) => continue,
            Err(err) => bail!("bridge closed: {err}"),
        }
    }
    let updated = updated.context("did not receive a tree after callback")?;
    if !updated.contains_text("Count: 1") {
        bail!("atom did not update after Clojure callback: {updated:?}");
    }
    println!("[host] milestone 4/5: atom updated and tree rerendered (Count: 1)");

    host.cmd_tx.send(Cmd::Reload)?;
    let started = Instant::now();
    let mut reloaded = false;
    while started.elapsed() < Duration::from_secs(30) {
        match host.event_rx.recv_blocking() {
            Ok(HostEvent::Tree(t)) => {
                if t.contains_text("Count: 1") {
                    reloaded = true;
                    break;
                }
            }
            Ok(HostEvent::Error(err)) => bail!("reload failed: {err}"),
            Ok(_) => continue,
            Err(err) => bail!("bridge closed: {err}"),
        }
    }
    if !reloaded {
        bail!("reload did not preserve defonce state");
    }
    println!("[host] milestone 6: reload preserved defonce atom state");
    println!("[host] protocol test passed");
    Ok(())
}
