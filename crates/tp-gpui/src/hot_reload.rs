use dioxus_devtools_types::DevserverMsg;
use gpui::App;
use std::net::SocketAddr;
use tungstenite::Message;

pub fn devserver_address() -> Option<SocketAddr> {
    match dioxus_cli_config::devserver_raw_addr() {
        Some(address) if address.ip().is_loopback() => return Some(address),
        Some(_) => eprintln!("TP hot reload: the dx server must use a loopback address"),
        None => eprintln!("TP hot reload: launch with dx serve --hot-patch"),
    }
    None
}

pub fn start(cx: &mut App, address: SocketAddr) {
    let build_id = dioxus_cli_config::build_id();
    let pid = std::process::id();
    let reference = subsecond::aslr_reference();
    let endpoint =
        format!("ws://{address}/_dioxus?aslr_reference={reference}&build_id={build_id}&pid={pid}");
    let (sender, receiver) = async_channel::bounded(16);

    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let (mut socket, _) = tungstenite::connect(endpoint)?;
            println!("TP hot reload connected (PID {pid})");
            loop {
                match socket.read()? {
                    Message::Text(text) => match serde_json::from_str(&text)? {
                        DevserverMsg::HotReload(message)
                            if message.for_build_id == Some(build_id)
                                && message.for_pid == Some(pid) =>
                        {
                            if let Some(patch) = message.jump_table {
                                if sender.send_blocking(patch).is_err() {
                                    break;
                                }
                            }
                        }
                        DevserverMsg::FullReloadCommand => {
                            eprintln!("TP hot reload: a full rebuild is required");
                        }
                        _ => {}
                    },
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("TP hot reload disconnected: {error:#}");
        }
    });

    cx.spawn(async move |cx| {
        while let Ok(patch) = receiver.recv().await {
            let result = cx.update(|cx| {
                // Only accept code from the local dx session for this build and process.
                // Apply it between UI updates so a render cannot straddle the patch.
                match unsafe { subsecond::apply_patch(patch) } {
                    Ok(()) => {
                        cx.refresh_windows();
                        println!("TP hot reload applied (PID {pid})");
                    }
                    Err(error) => eprintln!("TP hot reload failed: {error}"),
                }
            });
            if result.is_err() {
                break;
            }
        }
    })
    .detach();
}
