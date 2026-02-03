"""Yowl kitten - voice dictation for kitty terminal."""

from kittens.tui.handler import result_handler
from kitty.boss import Boss
from yowl.ipc import Client


def main(args: list[str]) -> None:
    """Not used when no_ui=True."""
    pass


@result_handler(no_ui=True)
def handle_result(args: list[str], answer: str, target_window_id: int, boss: Boss) -> None:
    """Handle the keybinding - runs directly in kitty process (no overlay)."""
    try:
        with Client() as client:
            if client.ping():
                result = "PONG - daemon is alive"
            else:
                result = "ERROR - unexpected response from daemon"
    except FileNotFoundError:
        result = "ERROR - daemon socket not found"
    except ConnectionRefusedError:
        result = "ERROR - daemon not responding"
    except Exception as e:
        result = f"ERROR - {e}"

    w = boss.window_id_map.get(target_window_id)
    if w is not None:
        w.paste_text(result)
