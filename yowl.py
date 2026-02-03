"""Yowl kitten - voice dictation for kitty terminal."""

from kittens.tui.handler import result_handler
from kitty.boss import Boss
from yowl.ipc import Client


def main(args: list[str]) -> None:
    """Not used when no_ui=True."""
    pass


def execute_command(args: list[str]) -> str:
    """Execute the command based on args and return result string."""
    command = args[1] if args else "ping"

    with Client() as client:
        if command == "start":
            response = client.start()
            if response == "OK":
                return "Recording started"
            return f"Start failed: {response}"
        elif command == "stop":
            response = client.stop()
            if response == "OK":
                return "Recording stopped"
            return f"Stop failed: {response}"
        elif command == "ping":
            if client.ping():
                return "PONG - daemon is alive"
            return "ERROR - unexpected response from daemon"
        else:
            return f"ERROR - unknown command: {command}"


@result_handler(no_ui=True)
def handle_result(args: list[str], answer: str, target_window_id: int, boss: Boss) -> None:
    """Handle the keybinding - runs directly in kitty process (no overlay)."""
    try:
        result = execute_command(args)
    except FileNotFoundError:
        result = "ERROR - daemon socket not found"
    except ConnectionRefusedError:
        result = "ERROR - daemon not responding"
    except Exception as e:
        result = f"ERROR - {e}"

    w = boss.window_id_map.get(target_window_id)
    if w is not None:
        w.paste_text(result)
