mod logging;

use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::logging::init()?;
    log::info!("yowl daemon started");

    loop {
        thread::sleep(Duration::from_secs(3));
        log::info!("heartbeat");
    }
}
