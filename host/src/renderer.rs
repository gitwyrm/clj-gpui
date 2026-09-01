use crate::catalog;
use crate::protocol::{Cmd, HostEvent, Node};
use gpui::{
    div, prelude::*, px, rgb, size, AnyElement, App, Bounds, ClickEvent, Context, Element,
    ElementId, Entity, Focusable, GlobalElementId, InspectorElementId, LayoutId, PathPromptOptions,
    Pixels, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    theme::{Theme, ThemeConfig, ThemeMode},
    v_flex, ActiveTheme as _, Root,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;

struct InputSlot {
    state: Entity<InputState>,
    on_change: Option<String>,
    on_submit: Option<String>,
    on_blur: Option<String>,
    on_escape: Option<String>,
    /// When set, ignore `Change` and wait for the tree that follows this submit.
    wait_for_seq: Option<u64>,
    /// Submitted string; a late `Change` echoing it must not restore the draft.
    submitted: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum ZenityPick {
    Picked(String),
    Cancelled,
    Unavailable,
    Failed(String),
}

fn zenity_from_output(
    status: std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> ZenityPick {
    if status.success() {
        let path = String::from_utf8_lossy(stdout).trim().to_string();
        if path.is_empty() {
            ZenityPick::Cancelled
        } else {
            ZenityPick::Picked(path)
        }
    } else if status.code() == Some(1) {
        // zenity: 1 means the user clicked Cancel.
        ZenityPick::Cancelled
    } else {
        let err = String::from_utf8_lossy(stderr).trim().to_string();
        let detail = if err.is_empty() {
            format!("zenity exited {status}")
        } else {
            err
        };
        ZenityPick::Failed(detail)
    }
}

fn zenity_pick_directory(title: &str) -> ZenityPick {
    match Command::new("zenity")
        .args(["--file-selection", "--directory", "--title", title])
        .output()
    {
        Ok(output) => zenity_from_output(output.status, &output.stdout, &output.stderr),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ZenityPick::Unavailable,
        Err(err) => ZenityPick::Failed(err.to_string()),
    }
}

fn zenity_to_cmd(request_id: String, pick: ZenityPick, portal_err: &str) -> Cmd {
    match pick {
        ZenityPick::Picked(path) => Cmd::DirectoryPicked {
            request_id,
            path: Some(path),
            error: None,
            cancelled: false,
        },
        ZenityPick::Cancelled => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: None,
            cancelled: true,
        },
        ZenityPick::Unavailable => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: Some(format!(
                "native folder picker failed ({portal_err}). Install xdg-desktop-portal or zenity."
            )),
            cancelled: false,
        },
        ZenityPick::Failed(detail) => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: Some(format!("zenity failed: {detail}")),
            cancelled: false,
        },
    }
}

pub struct RootView {
    tree: Option<Node>,
    status: String,
    error: Option<String>,
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    inputs: HashMap<String, InputSlot>,
    used_inputs: HashSet<String>,
    _appearance: Subscription,
    _keystrokes: Subscription,
    next_submit_seq: u64,
    tree_seq: Option<u64>,
    applied_title: String,
    applied_window_size: Option<(i32, i32)>,
}

impl RootView {
    pub fn new(
        nrepl_port: u16,
        cmd_tx: mpsc::Sender<Cmd>,
        event_rx: async_channel::Receiver<HostEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            this.apply_theme(window, cx);
            // Always notify: nested nodes may use `:theme :system` even when
            // the root is pinned to light or dark.
            cx.notify();
        });
        let keystrokes = cx.observe_keystrokes(|this, event, window, cx| {
            if event.keystroke.key != "escape" {
                return;
            }
            this.handle_escape(window, cx);
        });
        let _ = cmd_tx.send(Cmd::Render);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_rx.recv().await {
                let _ = this.update(cx, |view, cx| {
                    match event {
                        HostEvent::Ready { nrepl_port, .. } => {
                            view.nrepl_port = nrepl_port;
                            view.status = format!("nREPL 127.0.0.1:{nrepl_port} · connected");
                        }
                        HostEvent::Tree(tree, seq, themes) => {
                            catalog::install_clojure_sets(themes);
                            view.tree = Some(tree);
                            view.tree_seq = seq;
                            view.error = None;
                            view.status = format!(
                                "nREPL 127.0.0.1:{} · live · hot reload on",
                                view.nrepl_port
                            );
                        }
                        HostEvent::Error(err) => {
                            for slot in view.inputs.values_mut() {
                                slot.wait_for_seq = None;
                                slot.submitted = None;
                            }
                            view.error = Some(err);
                        }
                        HostEvent::PickDirectory { request_id, title } => {
                            view.start_pick_directory(request_id, title, cx);
                        }
                        HostEvent::RevealPath { path } => {
                            cx.reveal_path(Path::new(&path));
                        }
                        HostEvent::OpenPath { path } => {
                            cx.open_with_system(Path::new(&path));
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();

        Self {
            tree: None,
            status: format!("nREPL 127.0.0.1:{nrepl_port} · loading Clojure UI"),
            error: None,
            nrepl_port,
            cmd_tx,
            inputs: HashMap::new(),
            used_inputs: HashSet::new(),
            _appearance: appearance,
            _keystrokes: keystrokes,
            next_submit_seq: 0,
            tree_seq: None,
            applied_title: String::new(),
            applied_window_size: None,
        }
    }

    fn requested_theme(&self) -> &str {
        self.tree
            .as_ref()
            .and_then(|node| node.theme.as_deref())
            .filter(|theme| !theme.is_empty())
            .unwrap_or("system")
    }

    fn apply_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        apply_theme_pref(self.requested_theme(), window, cx);
    }

    fn requested_chrome(&self) -> &str {
        self.tree
            .as_ref()
            .and_then(|node| node.chrome.as_deref())
            .filter(|chrome| !chrome.is_empty())
            .unwrap_or("dev")
    }

    fn show_dev_chrome(&self) -> bool {
        self.requested_chrome() != "app"
    }

    fn apply_chrome(&mut self, window: &mut Window) {
        let (title, width, height) = {
            let Some(tree) = self.tree.as_ref() else {
                return;
            };
            (
                tree.title
                    .as_deref()
                    .filter(|title| !title.is_empty())
                    .unwrap_or("clj-gpui")
                    .to_string(),
                tree.window_width.or(tree.width),
                tree.window_height.or(tree.height),
            )
        };
        if self.applied_title != title {
            window.set_window_title(&title);
            self.applied_title = title;
        }
        if let (Some(width), Some(height)) = (width, height) {
            let requested = (width.round() as i32, height.round() as i32);
            if self.applied_window_size != Some(requested) {
                window.resize(size(px(width), px(height)));
                self.applied_window_size = Some(requested);
            }
        }
    }

    fn handle_escape(&self, window: &mut Window, cx: &mut Context<Self>) {
        for slot in self.inputs.values() {
            let Some(id) = slot.on_escape.clone() else {
                continue;
            };
            if slot.state.read(cx).focus_handle(cx).is_focused(window) {
                let _ = self.cmd_tx.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
                break;
            }
        }
    }

    fn start_pick_directory(
        &self,
        request_id: String,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let prompt = title.unwrap_or_else(|| "Choose a folder".to_string());
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(prompt.clone())),
        });
        let cmd_tx = self.cmd_tx.clone();
        cx.spawn(async move |_this, cx| {
            let cmd = match receiver.await {
                Ok(Ok(Some(paths))) => {
                    let path = paths
                        .into_iter()
                        .next()
                        .map(|p| p.to_string_lossy().into_owned());
                    Cmd::DirectoryPicked {
                        request_id,
                        cancelled: path.is_none(),
                        path,
                        error: None,
                    }
                }
                Ok(Ok(None)) => Cmd::DirectoryPicked {
                    request_id,
                    path: None,
                    error: None,
                    cancelled: true,
                },
                Ok(Err(err)) => {
                    // zenity `.output()` waits for the user. Do that on the
                    // background executor so the GPUI foreground runtime
                    // can keep painting while the dialog is open.
                    let prompt = prompt.clone();
                    let pick = cx
                        .background_executor()
                        .spawn(async move { zenity_pick_directory(&prompt) })
                        .await;
                    zenity_to_cmd(request_id, pick, &err.to_string())
                }
                Err(_) => Cmd::DirectoryPicked {
                    request_id,
                    path: None,
                    error: Some("folder picker was cancelled internally".into()),
                    cancelled: false,
                },
            };
            let _ = cmd_tx.send(cmd);
        })
        .detach();
    }

    fn click(&self, callback_id: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let cmd_tx = self.cmd_tx.clone();
        move |_, _, _| {
            let _ = cmd_tx.send(Cmd::Callback {
                id: callback_id.clone(),
                value: None,
                seq: None,
            });
        }
    }

    fn input_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        self.used_inputs.insert(key.to_string());

        if let Some(slot) = self.inputs.get_mut(key) {
            slot.on_change = node.on_change.clone();
            slot.on_submit = node.on_submit.clone();
            slot.on_blur = node.on_blur.clone();
            slot.on_escape = node.on_escape.clone();
            let state = slot.state.clone();
            let force = matches!(
                (slot.wait_for_seq, self.tree_seq),
                (Some(wait), Some(seq)) if wait == seq
            );
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let desired = node.text.clone().unwrap_or_default();
            let current = state.read(cx).value().to_string();
            if current != desired && (force || (!focused && slot.wait_for_seq.is_none())) {
                let desired = desired.clone();
                state.update(cx, |input, cx| {
                    input.set_value(desired, window, cx);
                });
            }
            if force {
                slot.wait_for_seq = None;
            }
            if let Some(placeholder) = node.placeholder.clone() {
                state.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx);
                });
            }
            if node.focus && !state.read(cx).focus_handle(cx).is_focused(window) {
                state.read(cx).focus_handle(cx).focus(window);
            }
            return state;
        }

        let placeholder = node.placeholder.clone().unwrap_or_default();
        let default = node.text.clone().unwrap_or_default();
        let want_focus = node.focus;
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(default)
        });
        self.inputs.insert(
            key.to_string(),
            InputSlot {
                state: state.clone(),
                on_change: node.on_change.clone(),
                on_submit: node.on_submit.clone(),
                on_blur: node.on_blur.clone(),
                on_escape: node.on_escape.clone(),
                wait_for_seq: None,
                submitted: None,
            },
        );

        let key_owned = key.to_string();
        cx.subscribe_in(
            &state,
            window,
            move |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let Some(slot) = this.inputs.get_mut(&key_owned) else {
                        return;
                    };
                    if slot.wait_for_seq.is_some() {
                        return;
                    }
                    let value = input.read(cx).value().to_string();
                    if slot.submitted.as_ref() == Some(&value) {
                        return;
                    }
                    slot.submitted = None;
                    let Some(id) = slot.on_change.clone() else {
                        return;
                    };
                    let _ = this.cmd_tx.send(Cmd::Callback {
                        id,
                        value: Some(value),
                        seq: None,
                    });
                }
                InputEvent::PressEnter { .. } => {
                    this.next_submit_seq = this.next_submit_seq.saturating_add(1);
                    let seq = this.next_submit_seq;
                    let (on_submit, value, state, clear) = {
                        let Some(slot) = this.inputs.get_mut(&key_owned) else {
                            return;
                        };
                        let value = input.read(cx).value().to_string();
                        slot.wait_for_seq = Some(seq);
                        slot.submitted = Some(value.clone());
                        let clear = slot.on_blur.is_none() && slot.on_escape.is_none();
                        (slot.on_submit.clone(), value, slot.state.clone(), clear)
                    };
                    if let Some(id) = on_submit {
                        let _ = this.cmd_tx.send(Cmd::Callback {
                            id,
                            value: Some(value),
                            seq: Some(seq),
                        });
                    }
                    // Compose fields (no blur/escape handlers) clear immediately so a
                    // stale render cannot put the text back before Clojure's tree arrives.
                    if clear {
                        state.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
                InputEvent::Blur => {
                    let Some(slot) = this.inputs.get(&key_owned) else {
                        return;
                    };
                    if slot.wait_for_seq.is_some() {
                        return;
                    }
                    let Some(id) = slot.on_blur.clone() else {
                        return;
                    };
                    let value = input.read(cx).value().to_string();
                    let _ = this.cmd_tx.send(Cmd::Callback {
                        id,
                        value: Some(value),
                        seq: None,
                    });
                }
                _ => {}
            },
        )
        .detach();

        if want_focus {
            state.read(cx).focus_handle(cx).focus(window);
        }

        state
    }

    fn render_node(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = widget_key(node, path);
        let scope = resolve_theme(node, window, cx);
        let prev = scope.as_ref().map(|_| Theme::global(cx).clone());
        if let Some(applied) = scope.as_ref() {
            activate_theme(applied, cx);
        }

        let element = match node.kind.as_str() {
            "window" => {
                let mut layout = node.clone();
                layout.width = None;
                layout.height = None;
                apply_style(v_flex().id(eid(&key)).size_full(), &layout, cx)
                    .children(self.render_children(node, path, window, cx))
                    .into_any_element()
            }
            "label" => {
                let mut el = apply_style(div().id(eid(&key)), node, cx)
                    .child(node.text.clone().unwrap_or_default());
                let on_click = node.on_click.clone();
                let on_double = node.on_double_click.clone();
                if on_click.is_some() || on_double.is_some() {
                    let cmd_tx = self.cmd_tx.clone();
                    el = el.cursor_pointer().on_click(move |event, _, _| {
                        if event.click_count() >= 2 {
                            if let Some(id) = on_double.clone() {
                                let _ = cmd_tx.send(Cmd::Callback {
                                    id,
                                    value: None,
                                    seq: None,
                                });
                                return;
                            }
                        }
                        if let Some(id) = on_click.clone() {
                            let _ = cmd_tx.send(Cmd::Callback {
                                id,
                                value: None,
                                seq: None,
                            });
                        }
                    });
                }
                el.into_any_element()
            }
            "button" => {
                let label = node.text.clone().unwrap_or_default();
                let mut button = Button::new(eid(&key)).label(label);
                button = apply_button_variant(button, node);
                if node.compact {
                    button = button.compact();
                }
                if let Some(callback_id) = node.on_click.clone() {
                    button = button.on_click(self.click(callback_id));
                }
                apply_style(button, node, cx).into_any_element()
            }
            "vstack" => {
                let mut el = apply_style(v_flex().id(eid(&key)), node, cx)
                    .children(self.render_children(node, path, window, cx));
                if let Some(callback_id) = node.on_click.clone() {
                    el = el.cursor_pointer().on_click(self.click(callback_id));
                }
                el.into_any_element()
            }
            "hstack" => {
                let mut el = apply_style(h_flex().id(eid(&key)), node, cx)
                    .children(self.render_children(node, path, window, cx));
                if let Some(callback_id) = node.on_click.clone() {
                    el = el.cursor_pointer().on_click(self.click(callback_id));
                }
                el.into_any_element()
            }
            "spacer" => {
                let el = apply_style(div().id(eid(&key)), node, cx);
                if node.size.is_some() || node.flex.is_some() {
                    el.into_any_element()
                } else {
                    el.flex_1().into_any_element()
                }
            }
            "checkbox" => {
                if node.shape.as_deref() == Some("circle") {
                    self.render_circle_checkbox(node, &key, cx)
                } else {
                    let checked = node.checked.unwrap_or(false);
                    let mut checkbox = Checkbox::new(eid(&key)).checked(checked);
                    if let Some(text) = node.text.clone() {
                        checkbox = checkbox.label(text);
                    }
                    if let Some(callback_id) = node.on_click.clone() {
                        let cmd_tx = self.cmd_tx.clone();
                        checkbox = checkbox.on_click(move |_, _, _| {
                            let _ = cmd_tx.send(Cmd::Callback {
                                id: callback_id.clone(),
                                value: None,
                                seq: None,
                            });
                        });
                    }
                    apply_style(checkbox, node, cx).into_any_element()
                }
            }
            "scroll" => self.render_scroll(node, path, &key, window, cx),
            "text-field" => {
                let state = self.input_slot(&key, node, window, cx);
                apply_style(Input::new(&state), node, cx).into_any_element()
            }
            other => div()
                .id(eid(&key))
                .text_color(cx.theme().danger)
                .child(format!("Unknown GPUI node: {other}"))
                .into_any_element(),
        };

        if let Some(prev) = prev {
            *Theme::global_mut(cx) = prev;
        }
        match scope {
            Some(applied) => ThemeScope::new(applied, element).into_any_element(),
            None => element,
        }
    }

    fn render_scroll(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // gpui-component's overflow_y_scrollbar wraps in size_full() (height
        // 100%). In a column with siblings that makes the scroller as tall as
        // the parent instead of the leftover space, so the window grows and
        // the list never scrolls. Bound the wrapper; the inner size_full then
        // fills that viewport.
        //
        // Viewport size (`:width` / `:height` / `:size` / leftover flex) lives
        // on the wrapper. Visual styles stay on the inner body so they are not
        // applied twice and so an explicit width is not swallowed by w_full().
        let viewport = scroll_viewport(node);
        let mut wrap = v_flex().id(eid(key)).min_h_0().overflow_hidden();
        wrap = match viewport.width {
            ScrollExtent::Px(width) => wrap.w(px(width)),
            ScrollExtent::Fill => wrap.w_full(),
        };
        wrap = match viewport.height {
            ScrollExtent::Px(height) => wrap.h(px(height)),
            ScrollExtent::Fill => wrap.flex_1(),
        };
        let mut inner = node.clone();
        inner.height = None;
        inner.width = None;
        inner.size = None;
        inner.flex = None;
        wrap.child(
            apply_style(v_flex().id(eid(&format!("{key}-body"))), &inner, cx)
                .overflow_y_scrollbar()
                .children(self.render_children(node, path, window, cx)),
        )
        .into_any_element()
    }

    fn render_children(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        node.children
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, child)| self.render_node(&child, &format!("{path}-{index}"), window, cx))
            .collect()
    }

    fn render_circle_checkbox(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let checked = node.checked.unwrap_or(false);
        let diameter = node.size.unwrap_or(30.0);
        let theme = cx.theme();
        let ring = if checked { theme.primary } else { theme.border };
        let mut mark = div()
            .id(eid(&format!("{key}-mark")))
            .flex()
            .items_center()
            .justify_center()
            .size(px(diameter))
            .rounded_full()
            .border_1()
            .border_color(ring)
            .cursor_pointer();
        if checked {
            mark = mark.child(
                div()
                    .text_size(px(16.))
                    .text_color(theme.primary)
                    .child("✓"),
            );
        }
        if let Some(callback_id) = node.on_click.clone() {
            mark = mark.on_click(self.click(callback_id));
        }
        let mut row = h_flex().id(eid(key)).items_center().gap(px(12.));
        row = row.child(mark);
        if let Some(text) = node.text.clone() {
            row = row.child(div().child(text));
        }
        let mut style = node.clone();
        style.size = None;
        apply_style(row, &style, cx).into_any_element()
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_theme(window, cx);
        self.apply_chrome(window);
        self.used_inputs.clear();
        let tree = self.tree.clone();
        let error = self.error.clone();

        let body = if let Some(error) = error {
            v_flex()
                .gap_2()
                .child(div().text_color(cx.theme().danger).child("Clojure error"))
                .child(div().text_color(cx.theme().foreground).child(error))
                .into_any_element()
        } else if let Some(tree) = tree.as_ref() {
            self.render_node(tree, "root", window, cx)
        } else {
            div()
                .text_color(cx.theme().muted_foreground)
                .child("Waiting for Clojure to render…")
                .into_any_element()
        };

        let used = std::mem::take(&mut self.used_inputs);
        self.inputs.retain(|key, _| used.contains(key));

        let show_footer = self.show_dev_chrome();
        let status = self.status.clone();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(v_flex().flex_1().min_h_0().child(body))
            .when(show_footer, |el| {
                el.child(
                    div()
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
    }
}

fn widget_key(node: &Node, path: &str) -> String {
    node.id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn parse_color(value: &str) -> Option<u32> {
    let value = value.trim().trim_start_matches('#');
    u32::from_str_radix(value, 16).ok()
}

/// One axis of a `scroll` viewport: a pixel size, or fill the parent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ScrollExtent {
    Px(f32),
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollViewport {
    width: ScrollExtent,
    height: ScrollExtent,
}

/// Size of the outer scroll viewport. `:size` is a square, matching
/// `apply_style` on other nodes. Omitted `:height` fills leftover column
/// space; omitted `:width` fills the parent width.
fn scroll_viewport(node: &Node) -> ScrollViewport {
    if let Some(size) = node.size {
        return ScrollViewport {
            width: ScrollExtent::Px(size),
            height: ScrollExtent::Px(size),
        };
    }
    ScrollViewport {
        width: node
            .width
            .map(ScrollExtent::Px)
            .unwrap_or(ScrollExtent::Fill),
        height: node
            .height
            .map(ScrollExtent::Px)
            .unwrap_or(ScrollExtent::Fill),
    }
}

fn apply_button_variant(button: Button, node: &Node) -> Button {
    match node.variant.as_deref() {
        Some("primary") => button.primary(),
        Some("ghost") => button.ghost(),
        Some("text") => button.text(),
        Some("danger") => button.danger(),
        Some("outline") => button.outline(),
        _ if node.primary => button.primary(),
        _ => button,
    }
}

fn apply_style<E: Styled>(mut el: E, node: &Node, cx: &App) -> E {
    if let Some(gap) = node.gap {
        el = el.gap(px(gap));
    }
    if let Some(padding) = node.padding {
        el = el.p(px(padding));
    }
    if let Some(width) = node.width {
        el = el.w(px(width));
    }
    if let Some(height) = node.height {
        el = el.h(px(height));
    }
    if let Some(size) = node.size {
        el = el.size(px(size));
    }
    if node.flex.unwrap_or(0.0) >= 1.0 {
        // Flex items default to min-height: auto (content size), so a
        // flex-1 child will not shrink below its contents. That prevents
        // nested overflow scroll from ever getting a bounded viewport.
        el = el.flex_1().min_h_0();
    }
    if let Some(font_size) = node.font_size {
        el = el.text_size(px(font_size));
    }
    if let Some(family) = &node.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(weight) = &node.font_weight {
        el = match weight.as_str() {
            "thin" => el.font_weight(gpui::FontWeight::THIN),
            "extralight" | "extra-light" | "ultralight" => {
                el.font_weight(gpui::FontWeight::EXTRA_LIGHT)
            }
            "bold" => el.font_weight(gpui::FontWeight::BOLD),
            "semibold" | "semi-bold" => el.font_weight(gpui::FontWeight::SEMIBOLD),
            "medium" => el.font_weight(gpui::FontWeight::MEDIUM),
            "light" => el.font_weight(gpui::FontWeight::LIGHT),
            _ => el.font_weight(gpui::FontWeight::NORMAL),
        };
    }
    if let Some(color) = node.color.as_deref().and_then(parse_color) {
        el = el.text_color(rgb(color));
    }
    if let Some(bg) = node.bg.as_deref().and_then(parse_color) {
        el = el.bg(rgb(bg));
    }
    if let Some(border) = node.border.as_deref().and_then(parse_color) {
        el = el.border_1().border_color(rgb(border));
    }
    if let Some(border) = node.border_bottom.as_deref().and_then(parse_color) {
        el = el.border_b_1().border_color(rgb(border));
    }
    if node.strikethrough {
        el = el.line_through();
    }
    if node.shadow {
        el = el.shadow_lg();
    }
    match node.align.as_deref() {
        Some("center") => el = el.items_center(),
        Some("end") => el = el.items_end(),
        Some("start") => el = el.items_start(),
        _ => {}
    }
    match node.justify.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("end") | Some("right") => el = el.justify_end(),
        Some("between") => el = el.justify_between(),
        _ => {}
    }
    if node_theme_pref(node).is_some() {
        if node.color.is_none() {
            el = el.text_color(cx.theme().foreground);
        }
        if node.bg.is_none()
            && matches!(
                node.kind.as_str(),
                "window" | "vstack" | "hstack" | "scroll"
            )
        {
            el = el.bg(cx.theme().background);
        }
    }
    el
}

fn node_theme_pref(node: &Node) -> Option<&str> {
    node.theme.as_deref().filter(|theme| !theme.is_empty())
}

#[derive(Clone)]
enum ThemeApply {
    Appearance(ThemeMode),
    Named(Rc<ThemeConfig>),
}

fn resolve_theme(node: &Node, window: &Window, cx: &App) -> Option<ThemeApply> {
    let pref = node_theme_pref(node)?;
    if catalog::is_appearance(pref) {
        let mode = match catalog::normalize(pref).as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::from(window.appearance()),
        };
        Some(ThemeApply::Appearance(mode))
    } else {
        match catalog::lookup(pref, ThemeMode::from(window.appearance()), cx) {
            Some(config) => Some(ThemeApply::Named(config)),
            None => {
                eprintln!("[host] unknown gpui-component theme {pref:?}; keeping current");
                None
            }
        }
    }
}

fn apply_theme_pref(pref: &str, window: &Window, cx: &mut App) {
    if catalog::is_appearance(pref) {
        catalog::reset_default_palettes(cx);
        let mode = match catalog::normalize(pref).as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::from(window.appearance()),
        };
        Theme::change(mode, None, cx);
        return;
    }
    match catalog::lookup(pref, ThemeMode::from(window.appearance()), cx) {
        Some(config) => {
            let already =
                catalog::names_equal(Theme::global(cx).theme_name(), config.name.as_ref());
            if !already {
                Theme::global_mut(cx).apply_config(&config);
            }
        }
        None => {
            eprintln!("[host] unknown gpui-component theme {pref:?}; keeping current");
        }
    }
}

fn activate_theme(applied: &ThemeApply, cx: &mut App) {
    match applied {
        ThemeApply::Appearance(mode) => {
            catalog::reset_default_palettes(cx);
            Theme::change(*mode, None, cx);
        }
        ThemeApply::Named(config) => {
            Theme::global_mut(cx).apply_config(config);
        }
    }
}

/// Switches gpui-component's global theme around a subtree during layout/paint.
/// Widgets such as Button read `cx.theme()` in `RenderOnce::render`, which runs
/// in `request_layout`, not when Clojure builds the tree.
///
/// Theme is process-global (`Theme::change` / `apply_config` on `App`). Nested
/// scopes work because layout/prepaint/paint of a subtree are synchronous: we
/// restore the previous `Theme` before a sibling is laid out. A second window
/// would share that global and is not supported today. Headless GPUI tests
/// cannot exercise this without a real window and GPU; Clojure tests cover
/// that `:theme` serializes per node.
struct ThemeScope {
    applied: ThemeApply,
    child: AnyElement,
}

impl ThemeScope {
    fn new(applied: ThemeApply, child: AnyElement) -> Self {
        Self { applied, child }
    }
}

fn with_theme_apply<R>(applied: &ThemeApply, cx: &mut App, f: impl FnOnce(&mut App) -> R) -> R {
    let prev = Theme::global(cx).clone();
    activate_theme(applied, cx);
    let result = f(cx);
    *Theme::global_mut(cx) = prev;
    result
}

impl IntoElement for ThemeScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ThemeScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        with_theme_apply(&self.applied, cx, |cx| {
            (self.child.request_layout(window, cx), ())
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        with_theme_apply(&self.applied, cx, |cx| {
            let _ = self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        with_theme_apply(&self.applied, cx, |cx| {
            self.child.paint(window, cx);
        });
    }
}

fn eid(path: &str) -> SharedString {
    SharedString::from(path.to_string())
}

fn quit_host(cx: &mut App) {
    cx.quit();
    // Unbundled macOS binaries often ignore `[NSApp terminate:]`, which is
    // what GPUI's quit() schedules. The window is already gone; kill the
    // host so Clojure sees the socket close and exits too.
    #[cfg(target_os = "macos")]
    std::process::exit(0);
}

pub fn open_window(
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    event_rx: async_channel::Receiver<HostEvent>,
    cx: &mut App,
) {
    use gpui::{size, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

    // GPUI's macOS default is to keep the NSApplication running after the
    // last window closes. The close-button path also goes through an
    // async try_borrow_mut; if App is already borrowed, on_window_closed
    // never fires. Hook should-close too, and always quit this
    // single-window host — don't wait for windows().is_empty().
    cx.on_window_closed(|cx| {
        quit_host(cx);
    })
    .detach();

    let bounds = Bounds::centered(None, size(px(580.), px(820.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("clj-gpui".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| RootView::new(nrepl_port, cmd_tx, event_rx, window, cx));
            let root = cx.new(|cx| Root::new(view, window, cx));
            window.on_window_should_close(cx, |_, cx| {
                quit_host(cx);
                true
            });
            root
        },
    )
    .unwrap();
    cx.activate(true);
}

#[cfg(test)]
mod zenity_tests {
    use super::{zenity_from_output, ZenityPick};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn exit(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn success_with_path_is_picked() {
        assert_eq!(
            zenity_from_output(exit(0), b"/tmp/docs\n", b""),
            ZenityPick::Picked("/tmp/docs".into())
        );
    }

    #[test]
    fn success_with_empty_stdout_is_cancelled() {
        assert_eq!(
            zenity_from_output(exit(0), b"  \n", b""),
            ZenityPick::Cancelled
        );
    }

    #[test]
    fn exit_1_is_user_cancel() {
        assert_eq!(zenity_from_output(exit(1), b"", b""), ZenityPick::Cancelled);
    }

    #[test]
    fn other_nonzero_is_failure() {
        assert_eq!(
            zenity_from_output(exit(255), b"", b"display is not set\n"),
            ZenityPick::Failed("display is not set".into())
        );
    }
}

#[cfg(test)]
mod scroll_viewport_tests {
    use super::{scroll_viewport, Node, ScrollExtent};

    fn node_with(width: Option<f32>, height: Option<f32>, size: Option<f32>) -> Node {
        Node {
            width,
            height,
            size,
            ..Node::default()
        }
    }

    #[test]
    fn flex_scroll_no_height_fills_parent() {
        let v = scroll_viewport(&node_with(None, None, None));
        assert_eq!(v.width, ScrollExtent::Fill);
        assert_eq!(v.height, ScrollExtent::Fill);
    }

    #[test]
    fn fixed_height_keeps_full_width() {
        let v = scroll_viewport(&node_with(None, Some(220.0), None));
        assert_eq!(v.width, ScrollExtent::Fill);
        assert_eq!(v.height, ScrollExtent::Px(220.0));
    }

    #[test]
    fn explicit_width_constrains_viewport() {
        let v = scroll_viewport(&node_with(Some(300.0), None, None));
        assert_eq!(v.width, ScrollExtent::Px(300.0));
        assert_eq!(v.height, ScrollExtent::Fill);
    }

    #[test]
    fn explicit_width_and_height() {
        let v = scroll_viewport(&node_with(Some(300.0), Some(220.0), None));
        assert_eq!(v.width, ScrollExtent::Px(300.0));
        assert_eq!(v.height, ScrollExtent::Px(220.0));
    }

    #[test]
    fn size_is_a_square_viewport() {
        let v = scroll_viewport(&node_with(Some(300.0), Some(220.0), Some(180.0)));
        assert_eq!(v.width, ScrollExtent::Px(180.0));
        assert_eq!(v.height, ScrollExtent::Px(180.0));
    }
}
