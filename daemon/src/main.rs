mod ipc;
mod logging;
mod state;

use std::io::ErrorKind;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::logging::init()?;

    let parent_pid = std::os::unix::process::parent_id();
    log::info!("yowl daemon started (parent_pid={parent_pid})");

    let server = ipc::Server::bind()?;
    server.set_nonblocking(true)?;

    let state = state::DaemonState::new();
    let mut connection: Option<ipc::Connection> = None;

    loop {
        if std::os::unix::process::parent_id() != parent_pid {
            log::info!("parent process exited, shutting down");
            break;
        }

        match server.accept() {
            Ok(conn) => {
                connection = Some(conn);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => log::warn!("accept error: {e}"),
        }

        if let Some(ref mut conn) = connection {
            match conn.read_command() {
                Ok(Some(cmd)) => {
                    log::debug!("received command: {cmd}");
                    let response = ipc::handle_command(&cmd, &state);
                    if let Err(e) = conn.send(&response) {
                        log::warn!("send error: {e}");
                        connection = None;
                    }
                    if cmd.to_uppercase() == "SHUTDOWN" {
                        log::info!("shutdown command received");
                        break;
                    }
                }
                Ok(None) => {
                    log::debug!("client disconnected");
                    connection = None;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => {
                    log::warn!("read error: {e}");
                    connection = None;
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}
