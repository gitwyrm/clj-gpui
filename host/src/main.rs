mod bridge;
mod protocol;
mod renderer;

use anyhow::Result;

fn main() -> Result<()> {
    let protocol_test = std::env::args().any(|a| a == "--protocol-test");
    if protocol_test {
        return bridge::protocol_test();
    }

    let host = bridge::start()?;
    let nrepl_port = host.nrepl_port;
    let cmd_tx = host.cmd_tx.clone();
    let event_rx = host.event_rx.clone();

    gpui::Application::new().run(move |cx| {
        gpui_component::init(cx);
        renderer::open_window(nrepl_port, cmd_tx, event_rx, cx);
    });

    drop(host);
    Ok(())
}
