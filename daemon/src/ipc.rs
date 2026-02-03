use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    std::env::var("YOWL_SOCKET_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut path = std::env::temp_dir();
            let uid = unsafe { libc::getuid() };
            path.push(format!("yowl-{uid}.sock"));
            path
        })
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    pub fn bind() -> std::io::Result<Self> {
        let path = socket_path();

        // Remove stale socket if it exists
        if path.exists() {
            std::fs::remove_file(&path)?;
        }

        let listener = UnixListener::bind(&path)?;
        log::info!("IPC server listening on {}", path.display());

        Ok(Self { listener, path })
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.listener.set_nonblocking(nonblocking)
    }

    pub fn accept(&self) -> std::io::Result<Connection> {
        let (stream, _) = self.listener.accept()?;
        stream.set_nonblocking(true)?;
        log::debug!("client connected");
        Ok(Connection::new(stream))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Connection {
    fn new(stream: UnixStream) -> Self {
        let writer = stream.try_clone().expect("failed to clone stream");
        Self {
            reader: BufReader::new(stream),
            writer,
        }
    }

    pub fn read_command(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        let bytes = self.reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None); // EOF - client disconnected
        }
        Ok(Some(line.trim().to_string()))
    }

    pub fn send(&mut self, response: &str) -> std::io::Result<()> {
        writeln!(self.writer, "{}", response)?;
        self.writer.flush()
    }
}

pub fn handle_command(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    match parts[0].to_uppercase().as_str() {
        "PING" => "PONG".to_string(),
        "START" => {
            log::info!("START command received - recording would begin here");
            "OK".to_string()
        }
        "STOP" => {
            log::info!("STOP command received - recording would end here");
            "OK".to_string()
        }
        _ => format!("ERROR unknown command: {}", parts[0]),
    }
}
