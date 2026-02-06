"""Yowl kitten - voice dictation for kitty terminal."""

from kittens.tui.handler import result_handler
from kitty.boss import Boss
from kitty.fast_data_types import add_timer, get_boss
from yowl.ipc import Client

polling_active = False
target_window_id: int | None = None

# Polling interval in seconds
POLL_INTERVAL = 0.1


def main(args: list[str]) -> None:
    """Not used when no_ui=True."""
    pass


def poll_callback(timer_id: int | None = None) -> None:
    """Poll the daemon for new text and inject it into the terminal."""
    global polling_active

    if target_window_id is None:
        polling_active = False
        return

    try:
        with Client() as client:
            is_recording, text = client.poll()
            if not is_recording:
                # Daemon says it's idle, stop polling
                polling_active = False
                return
            if text:
                boss = get_boss()
                if boss is not None:
                    w = boss.window_id_map.get(target_window_id)
                    if w is not None:
                        w.paste_text(text)
    except Exception:
        # Connection errors - stop polling
        polling_active = False
        return

    # Schedule next poll
    add_timer(poll_callback, POLL_INTERVAL, False)


def _start_recording(window_id: int) -> str:
    """Start recording and begin polling loop."""
    global polling_active, target_window_id

    if polling_active:
        return "ERROR - already recording"

    with Client() as client:
        response = client.start()
        if response != "OK":
            return f"Start failed: {response}"

    polling_active = True
    target_window_id = window_id

    # Start the polling loop
    add_timer(poll_callback, POLL_INTERVAL, False)

    return "Recording started"


def _stop_recording() -> str:
    """Stop recording - polling loop will stop when daemon reports IDLE."""
    with Client() as client:
        response = client.stop()
        if response != "OK":
            return f"Stop failed: {response}"

    return "Recording stopped"


def execute_command(args: list[str], window_id: int) -> str:
    """Execute the command based on args and return result string."""
    command = args[1] if len(args) > 1 else "ping"

    if command == "start":
        return _start_recording(window_id)
    elif command == "stop":
        return _stop_recording()
    elif command == "ping":
        with Client() as client:
            if client.ping():
                return "PONG - daemon is alive"
            return "ERROR - unexpected response from daemon"
    else:
        return f"ERROR - unknown command: {command}"


@result_handler(no_ui=True)
def handle_result(args: list[str], answer: str, target_window_id: int, boss: Boss) -> None:
    """Handle the keybinding - runs directly in kitty process (no overlay)."""
    try:
        result = execute_command(args, target_window_id)
    except FileNotFoundError:
        result = "ERROR - daemon socket not found"
    except ConnectionRefusedError:
        result = "ERROR - daemon not responding"
    except Exception as e:
        result = f"ERROR - {e}"

    # Only paste error messages, not success confirmations
    if result.startswith("ERROR"):
        w = boss.window_id_map.get(target_window_id)
        if w is not None:
            w.paste_text(result)
