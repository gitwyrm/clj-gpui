use crate::protocol::{Cmd, HostEvent, Node};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, ClickEvent, Context, Entity, Focusable,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Root,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

struct InputSlot {
    state: Entity<InputState>,
    on_change: Option<String>,
    on_submit: Option<String>,
    sync_after_submit: bool,
}

pub struct RootView {
    tree: Option<Node>,
    status: String,
    error: Option<String>,
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    inputs: HashMap<String, InputSlot>,
    used_inputs: HashSet<String>,
}

impl RootView {
    pub fn new(
        nrepl_port: u16,
        cmd_tx: mpsc::Sender<Cmd>,
        event_rx: async_channel::Receiver<HostEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = cmd_tx.send(Cmd::Render);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_rx.recv().await {
                let _ = this.update(cx, |view, cx| {
                    match event {
                        HostEvent::Ready { nrepl_port, .. } => {
                            view.nrepl_port = nrepl_port;
                            view.status = format!("nREPL 127.0.0.1:{nrepl_port} · connected");
                        }
                        HostEvent::Tree(tree) => {
                            view.tree = Some(tree);
                            view.error = None;
                            view.status = format!(
                                "nREPL 127.0.0.1:{} · live · hot reload on",
                                view.nrepl_port
                            );
                        }
                        HostEvent::Error(err) => {
                            view.error = Some(err);
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
        }
    }

    fn click(&self, callback_id: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let cmd_tx = self.cmd_tx.clone();
        move |_, _, _| {
            let _ = cmd_tx.send(Cmd::Callback {
                id: callback_id.clone(),
                value: None,
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
            let state = slot.state.clone();
            let sync = slot.sync_after_submit;
            if sync {
                slot.sync_after_submit = false;
            }
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let desired = node.text.clone().unwrap_or_default();
            let current = state.read(cx).value().to_string();
            if current != desired && (!focused || sync) {
                let desired = desired.clone();
                state.update(cx, |input, cx| {
                    input.set_value(desired, window, cx);
                });
            }
            if let Some(placeholder) = node.placeholder.clone() {
                state.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx);
                });
            }
            return state;
        }

        let placeholder = node.placeholder.clone().unwrap_or_default();
        let default = node.text.clone().unwrap_or_default();
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
                sync_after_submit: false,
            },
        );

        let key_owned = key.to_string();
        cx.subscribe(
            &state,
            move |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    if let Some(id) = this
                        .inputs
                        .get(&key_owned)
                        .and_then(|slot| slot.on_change.clone())
                    {
                        let value = input.read(cx).value().to_string();
                        let _ = this.cmd_tx.send(Cmd::Callback {
                            id,
                            value: Some(value),
                        });
                    }
                }
                InputEvent::PressEnter { .. } => {
                    if let Some(slot) = this.inputs.get_mut(&key_owned) {
                        slot.sync_after_submit = true;
                        if let Some(id) = slot.on_submit.clone() {
                            let value = input.read(cx).value().to_string();
                            let _ = this.cmd_tx.send(Cmd::Callback {
                                id,
                                value: Some(value),
                            });
                        }
                    }
                }
                _ => {}
            },
        )
        .detach();

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
        match node.kind.as_str() {
            "label" => apply_style(div().id(eid(&key)), node)
                .child(node.text.clone().unwrap_or_default())
                .into_any_element(),
            "button" => {
                let label = node.text.clone().unwrap_or_default();
                let mut button = Button::new(eid(&key)).label(label);
                if node.primary {
                    button = button.primary();
                }
                if let Some(callback_id) = node.on_click.clone() {
                    button = button.on_click(self.click(callback_id));
                }
                apply_style(button, node).into_any_element()
            }
            "vstack" => apply_style(v_flex().id(eid(&key)), node)
                .children(self.render_children(node, path, window, cx))
                .into_any_element(),
            "hstack" => apply_style(h_flex().id(eid(&key)), node)
                .children(self.render_children(node, path, window, cx))
                .into_any_element(),
            "spacer" => {
                let el = apply_style(div().id(eid(&key)), node);
                if node.size.is_some() || node.flex.is_some() {
                    el.into_any_element()
                } else {
                    el.flex_1().into_any_element()
                }
            }
            "checkbox" => {
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
                        });
                    });
                }
                apply_style(checkbox, node).into_any_element()
            }
            "scroll" => apply_style(v_flex().id(eid(&key)), node)
                .overflow_y_scrollbar()
                .children(self.render_children(node, path, window, cx))
                .into_any_element(),
            "text-field" => {
                let state = self.input_slot(&key, node, window, cx);
                apply_style(Input::new(&state), node).into_any_element()
            }
            other => div()
                .id(eid(&key))
                .text_color(cx.theme().danger)
                .child(format!("Unknown GPUI node: {other}"))
                .into_any_element(),
        }
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
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex().flex_1().p_4().child(
                    v_flex()
                        .flex_1()
                        .p_4()
                        .gap_3()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().secondary)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(body),
                ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status.clone()),
            )
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

fn apply_style<E: Styled>(mut el: E, node: &Node) -> E {
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
        el = el.flex_1();
    }
    if let Some(font_size) = node.font_size {
        el = el.text_size(px(font_size));
    }
    if let Some(weight) = &node.font_weight {
        el = match weight.as_str() {
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
    el
}

fn eid(path: &str) -> SharedString {
    SharedString::from(path.to_string())
}

pub fn open_window(
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    event_rx: async_channel::Receiver<HostEvent>,
    cx: &mut App,
) {
    use gpui::{size, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

    // GPUI's default is platform-specific: on macOS the process stays alive
    // after the last window closes (QuitMode::Explicit). This is a
    // single-window app, so quit when the window goes away.
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();

    let bounds = Bounds::centered(None, size(px(640.), px(760.)), cx);
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
            let view = cx.new(|cx| RootView::new(nrepl_port, cmd_tx, event_rx, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .unwrap();
    cx.activate(true);
}
