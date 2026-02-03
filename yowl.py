"""Yowl kitten - voice dictation for kitty terminal."""

from kitty.boss import Boss
from yowl.ipc import Client


def main(args: list[str]) -> str:
    """Main entry point - runs in overlay window."""
    try:
        with Client() as client:
            if client.ping():
                return "PONG - daemon is alive"
            return "ERROR - unexpected response from daemon"
    except FileNotFoundError:
        return "ERROR - daemon socket not found"
    except ConnectionRefusedError:
        return "ERROR - daemon not responding"
    except Exception as e:
        return f"ERROR - {e}"


def handle_result(args: list[str], answer: str, target_window_id: int, boss: Boss) -> None:
    """Handle the result from main() - runs in kitty process."""
    w = boss.window_id_map.get(target_window_id)
    if w is not None:
        w.paste_text(answer)
