"""IPC client for communicating with the yowl daemon."""

import os
import socket
from pathlib import Path


def socket_path() -> Path:
    """Get the socket path, matching the daemon's logic."""
    env_path = os.environ.get("YOWL_SOCKET_PATH")
    if env_path:
        return Path(env_path)
    import tempfile
    return Path(tempfile.gettempdir()) / f"yowl-{os.getuid()}.sock"


class Client:
    """IPC client for the yowl daemon."""

    def __init__(self, path: Path | None = None):
        self.path = path or socket_path()
        self.sock: socket.socket | None = None

    def connect(self) -> None:
        """Connect to the daemon."""
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(self.path))

    def close(self) -> None:
        """Close the connection."""
        if self.sock:
            self.sock.close()
            self.sock = None

    def send(self, command: str) -> str:
        """Send a command and return the response."""
        if not self.sock:
            raise RuntimeError("Not connected")
        self.sock.sendall(f"{command}\n".encode())
        response = b""
        while not response.endswith(b"\n"):
            chunk = self.sock.recv(1024)
            if not chunk:
                break
            response += chunk
        return response.decode().strip()

    def ping(self) -> bool:
        """Send PING and return True if PONG received."""
        return self.send("PING") == "PONG"

    def start(self) -> str:
        """Send START command and return the response."""
        return self.send("START")

    def stop(self) -> str:
        """Send STOP command and return the response."""
        return self.send("STOP")

    def __enter__(self) -> "Client":
        self.connect()
        return self

    def __exit__(self, *args) -> None:
        self.close()
