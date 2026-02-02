mod logging;

use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::logging::init()?;

    let parent_pid = std::os::unix::process::parent_id();
    log::info!("yowl daemon started (parent_pid={parent_pid})");

    loop {
        if std::os::unix::process::parent_id() != parent_pid {
            log::info!("parent process exited, shutting down");
            break;
        }
        thread::sleep(Duration::from_secs(3));
        log::debug!("heartbeat");
    }

    Ok(())
}
