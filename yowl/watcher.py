"""
Kitty watcher for yowl speech-to-text daemon.

This module is loaded by kitty at startup via the `watcher` config option.
The on_load() callback starts the speech daemon, which then monitors the
kitty process and shuts down when kitty exits.

Configuration (add to ~/.config/kitty/kitty.conf):
    watcher /path/to/kitty-yowl/watcher.py
"""

import os
import syslog
from typing import Any

from kitty.boss import Boss


DAEMON_STARTED = False


def _log(message: str) -> None:
    """Log to syslog with yowl prefix."""
    syslog.syslog(syslog.LOG_INFO, f"[yowl] {message}")


def _start_daemon(kitty_pid: int) -> None:
    """
    Start the yowl speech daemon.

    Args:
        kitty_pid: The PID of the kitty process, passed to daemon
                   so it can monitor for kitty exit.
    """
    global DAEMON_STARTED

    if DAEMON_STARTED:
        _log("daemon already started, skipping")
        return

    _log(f"starting daemon (kitty_pid={kitty_pid})")

    # TODO: Actually spawn the Rust daemon here, passing kitty_pid
    # Example:
    #   subprocess.Popen(
    #       ["yowl-daemon", "--parent-pid", str(kitty_pid)],
    #       start_new_session=True,
    #   )

    DAEMON_STARTED = True
    _log("daemon started successfully")


def on_load(boss: Boss, data: dict[str, Any]) -> None:
    """
    Called once when this watcher module is first loaded.

    This effectively runs at kitty startup, making it the ideal place
    to spawn the speech daemon.
    """
    kitty_pid = os.getpid()
    _log(f"on_load called (kitty_pid={kitty_pid})")

    try:
        _start_daemon(kitty_pid)
    except Exception as e:
        _log(f"failed to start daemon: {e}")
