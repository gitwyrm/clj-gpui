use crate::protocol::{Cmd, HostEvent, Node};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, ClickEvent, Context, Div, SharedString, Styled,
    Window,
};
use std::sync::mpsc;

const BG: u32 = 0x1a1b26;
const SURFACE: u32 = 0x24283b;
const TEXT: u32 = 0xc0caf5;
const MUTED: u32 = 0x9aa3b5;
const ACCENT: u32 = 0x7aa2f7;
const BUTTON: u32 = 0x3d59a1;
const BUTTON_HOVER: u32 = 0x5470c6;
const BUTTON_ACTIVE: u32 = 0x2e447c;
const BORDER: u32 = 0x3b4261;
const CHECK: u32 = 0x9ece6a;

pub struct RootView {
    tree: Option<Node>,
    status: String,
    error: Option<String>,
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
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
                            view.status =
                                format!("nREPL 127.0.0.1:{} · live · hot reload on", view.nrepl_port);
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
        }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if let Some(error) = &self.error {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_color(rgb(0xf7768e))
                        .child("Clojure error"),
                )
                .child(div().text_color(rgb(TEXT)).child(error.clone()))
                .into_any_element()
        } else if let Some(tree) = &self.tree {
            render_node(tree, "root", cx)
        } else {
            div()
                .text_color(rgb(MUTED))
                .child("Waiting for Clojure to render…")
                .into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex_1()
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .p_4()
                            .gap_3()
                            .rounded_lg()
                            .bg(rgb(SURFACE))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .child(body),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .text_color(rgb(MUTED))
                    .child(self.status.clone()),
            )
    }
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

fn clickable(
    el: gpui::Stateful<Div>,
    on_click: Option<String>,
    cx: &mut Context<RootView>,
) -> gpui::Stateful<Div> {
    if let Some(callback_id) = on_click {
        el.on_click(cx.listener(move |this, _: &ClickEvent, _window, _cx| {
            let _ = this.cmd_tx.send(Cmd::Callback(callback_id.clone()));
        }))
    } else {
        el
    }
}

fn render_node(node: &Node, path: &str, cx: &mut Context<RootView>) -> AnyElement {
    match node.kind.as_str() {
        "label" => apply_style(div().id(eid(path)), node)
            .child(node.text.clone().unwrap_or_default())
            .into_any_element(),
        "button" => {
            let label = node.text.clone().unwrap_or_default();
            clickable(
                apply_style(div().id(eid(path)), node)
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(BUTTON))
                    .text_color(rgb(0xffffff))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .hover(|s| s.bg(rgb(BUTTON_HOVER)))
                    .active(|s| s.bg(rgb(BUTTON_ACTIVE))),
                node.on_click.clone(),
                cx,
            )
            .child(label)
            .into_any_element()
        }
        "vstack" => apply_style(div().id(eid(path)).flex().flex_col(), node)
            .children(render_children(node, path, cx))
            .into_any_element(),
        "hstack" => apply_style(
            div()
                .id(eid(path))
                .flex()
                .flex_row()
                .items_center(),
            node,
        )
        .children(render_children(node, path, cx))
        .into_any_element(),
        "spacer" => {
            let el = apply_style(div().id(eid(path)), node);
            if node.size.is_some() || node.flex.is_some() {
                el.into_any_element()
            } else {
                el.flex_1().into_any_element()
            }
        }
        "checkbox" => {
            let checked = node.checked.unwrap_or(false);
            // Visual only. A nested `.id()` / `on_click` takes the hit and, with
            // a parent handler too, toggles twice — looking like a no-op.
            let mark = div()
                .size(px(18.))
                .rounded_sm()
                .border_1()
                .border_color(if checked { rgb(CHECK) } else { rgb(ACCENT) })
                .bg(if checked { rgb(CHECK) } else { rgb(0x1a1b26) });
            clickable(
                apply_style(
                    div()
                        .id(eid(path))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .cursor(gpui::CursorStyle::PointingHand)
                        .block_mouse_except_scroll(),
                    node,
                ),
                node.on_click.clone(),
                cx,
            )
            .child(mark)
            .when_some(node.text.clone(), |el, text| el.child(text))
            .into_any_element()
        }
        "scroll" => apply_style(
            div()
                .id(eid(path))
                .flex()
                .flex_col()
                .overflow_y_scroll(),
            node,
        )
        .children(render_children(node, path, cx))
        .into_any_element(),
        other => div()
            .id(eid(path))
            .text_color(rgb(0xf7768e))
            .child(format!("Unknown GPUI node: {other}"))
            .into_any_element(),
    }
}

fn render_children(
    node: &Node,
    path: &str,
    cx: &mut Context<RootView>,
) -> Vec<AnyElement> {
    node.children
        .iter()
        .enumerate()
        .map(|(index, child)| render_node(child, &format!("{path}-{index}"), cx))
        .collect()
}

pub fn open_window(nrepl_port: u16, cmd_tx: mpsc::Sender<Cmd>, event_rx: async_channel::Receiver<HostEvent>, cx: &mut App) {
    use gpui::{size, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

    let bounds = Bounds::centered(None, size(px(560.), px(720.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("ClojureGPUI".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |_, cx| {
            cx.new(|cx| RootView::new(nrepl_port, cmd_tx, event_rx, cx))
        },
    )
    .unwrap();
    cx.activate(true);
}
