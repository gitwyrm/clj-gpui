mod action_bridge;
mod bridge;
mod catalog;
mod chat;
mod extra;
mod mapping;
mod overlay;
mod preview;
mod protocol;
mod renderer;
mod rows;

use anyhow::Result;
use gpui_kit::application;
use gpui_kit::assets as gpui_kit_assets;
use std::sync::Arc;

fn main() -> Result<()> {
    if let Some(request) = preview::parse_capture_args(std::env::args()) {
        preview::run_helper(request);
        return Ok(());
    }

    let protocol_test = std::env::args().any(|a| a == "--protocol-test");
    if protocol_test {
        return bridge::protocol_test();
    }

    let host = bridge::start()?;
    let nrepl_port = host.nrepl_port;
    let cmd_tx = host.cmd_tx.clone();
    let event_rx = host.event_rx.clone();

    // Kit Avatar paints the image slot whenever `:src` is set; without an
    // HTTP client remote URLs stay empty circles (NullHttpClient). Same
    // requirement as Kit Storybook / the text_max_lines example.
    let http_client = reqwest_client::ReqwestClient::user_agent("clj-gpui/host")?;

    application()
        .with_assets(gpui_kit_assets::Assets)
        .with_http_client(Arc::new(http_client))
        .run(move |cx| {
            gpui_kit::init(cx);
            renderer::open_window(nrepl_port, cmd_tx, event_rx, cx);
        });

    drop(host);
    Ok(())
}
