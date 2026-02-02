"""
Kitty watcher for yowl speech-to-text daemon.

This module is loaded by kitty at startup via the `watcher` config option.
The on_load() callback starts the speech daemon, which then monitors the
kitty process and shuts down when kitty exits.

Configuration (add to ~/.config/kitty/kitty.conf):
    watcher /path/to/kitty-yowl/watcher.py
"""

import os
import shutil
import subprocess
import syslog
from pathlib import Path
from typing import Any

from kitty.boss import Boss


DAEMON_STARTED = False
DAEMON_PROCESS: subprocess.Popen | None = None

SYSLOG_LEVELS = {
    "off": -1,
    "error": syslog.LOG_ERR,
    "warn": syslog.LOG_WARNING,
    "info": syslog.LOG_INFO,
    "debug": syslog.LOG_DEBUG,
    "trace": syslog.LOG_DEBUG,
}


def init_logging() -> None:
    """Initialize syslog with level from YOWL_LOG_LEVEL (default: warn)."""
    syslog.openlog("yowl", syslog.LOG_PID, syslog.LOG_USER)
    level = SYSLOG_LEVELS.get(os.environ.get("YOWL_LOG_LEVEL", "warn"), syslog.LOG_WARNING)
    if level == -1:
        syslog.setlogmask(0)
    else:
        syslog.setlogmask(syslog.LOG_UPTO(level))


def log(message: str, level: int = syslog.LOG_INFO) -> None:
    """Log a message to syslog."""
    syslog.syslog(level, f"[yowl] {message}")


def get_daemon_path() -> Path:
    """
    Get the path to the daemon binary.

    Search order:
    1. YOWL_DAEMON_PATH environment variable (explicit override)
    2. 'yowl-daemon' on PATH (installed to /usr/local/bin, ~/.local/bin, etc.)
    3. Development builds: ../daemon/target/release/daemon or debug
    """
    env_path = os.environ.get("YOWL_DAEMON_PATH")
    if env_path:
        path = Path(env_path)
        if path.exists():
            return path
        raise FileNotFoundError(f"YOWL_DAEMON_PATH set but not found: {env_path}")

    which_path = shutil.which("yowl-daemon")
    if which_path:
        return Path(which_path)

    project_root = Path(__file__).parent.parent
    for build_type in ("release", "debug"):
        dev_path = project_root / "daemon" / "target" / build_type / "daemon"
        if dev_path.exists():
            return dev_path

    raise FileNotFoundError(
        "yowl-daemon not found. Install with 'cargo install --path daemon' "
        "or build with 'cargo build --release' for development."
    )


def start_daemon() -> None:
    """Start the yowl speech daemon."""
    global DAEMON_STARTED, DAEMON_PROCESS

    if DAEMON_STARTED:
        log("daemon already started, skipping", syslog.LOG_DEBUG)
        return

    daemon_path = get_daemon_path()
    log(f"starting daemon at {daemon_path}", syslog.LOG_DEBUG)

    DAEMON_PROCESS = subprocess.Popen(
        [str(daemon_path)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    DAEMON_STARTED = True
    log(f"daemon started (pid={DAEMON_PROCESS.pid})")


def on_load(boss: Boss, data: dict[str, Any]) -> None:
    """
    Called once when this watcher module is first loaded.

    This effectively runs at kitty startup, making it the ideal place
    to spawn the speech daemon.
    """
    init_logging()
    log("on_load called", syslog.LOG_DEBUG)

    try:
        start_daemon()
    except Exception as e:
        log(f"failed to start daemon: {e}", syslog.LOG_ERR)
