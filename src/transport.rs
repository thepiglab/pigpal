use crate::protocol;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Demo,
    TcpConnect,
    TcpListen,
    Udp,
    Serial,
    Notes,
}
impl Source {
    pub const ALL: [Self; 6] = [
        Self::Demo,
        Self::TcpConnect,
        Self::TcpListen,
        Self::Udp,
        Self::Serial,
        Self::Notes,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Demo => "Synthetic demo",
            Self::TcpConnect => "TCP connect",
            Self::TcpListen => "TCP listener",
            Self::Udp => "UDP listener",
            Self::Serial => "Serial / RS-232",
            Self::Notes => "Notes & media",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub mode: Source,
    pub host: String,
    pub bind: String,
    pub port: u16,
    pub allow_any_peer: bool,
    pub hl7: bool,
    pub acknowledge: bool,
    pub serial_port: String,
    pub baud: u32,
}
impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            mode: Source::Demo,
            host: "127.0.0.1".into(),
            bind: "127.0.0.1".into(),
            port: 2575,
            allow_any_peer: false,
            hl7: true,
            acknowledge: false,
            serial_port: String::new(),
            baud: 9600,
        }
    }
}
impl SourceConfig {
    pub fn validate(&self) -> Result<()> {
        if matches!(
            self.mode,
            Source::TcpConnect | Source::TcpListen | Source::Udp
        ) {
            ensure!(self.port > 0, "Enter the confirmed monitor port");
            if self.mode == Source::TcpConnect || !self.allow_any_peer {
                self.host
                    .parse::<IpAddr>()
                    .context("Enter the monitor's IP address")?;
            }
            if self.mode != Source::TcpConnect {
                self.bind
                    .parse::<IpAddr>()
                    .context("Enter the laptop's local bind IP, or 0.0.0.0")?;
            }
        }
        if self.mode == Source::Serial {
            ensure!(
                !self.serial_port.trim().is_empty() && self.baud > 0,
                "Enter a serial port and baud rate"
            );
        }
        Ok(())
    }
}
pub enum Input {
    Connected {
        connection: String,
        peer: String,
    },
    Bytes {
        connection: String,
        peer: String,
        bytes: Vec<u8>,
        reply: Option<SyncSender<Vec<u8>>>,
        received_utc: chrono::DateTime<chrono::Utc>,
        received_at: Instant,
    },
    Disconnected {
        connection: String,
    },
    Status(String),
    Error(String),
}
pub struct Worker {
    stop: Arc<AtomicBool>,
    pub rx: Receiver<Input>,
    join: thread::JoinHandle<()>,
}
impl Worker {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
    pub fn finished(&self) -> bool {
        self.join.is_finished()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
fn emit(tx: &SyncSender<Input>, stop: &AtomicBool, mut item: Input) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        match tx.try_send(item) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(i)) => {
                item = i;
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
pub fn start(c: SourceConfig) -> Result<Worker> {
    c.validate()?;
    let (tx, rx) = mpsc::sync_channel(128);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let join = thread::spawn(move || {
        if let Err(e) = run(&c, &tx, &flag) {
            emit(&tx, &flag, Input::Error(format!("{e:#}")));
        }
    });
    Ok(Worker { stop, rx, join })
}
fn read_stream(
    mut stream: impl Read + Write,
    peer: String,
    c: &SourceConfig,
    tx: &SyncSender<Input>,
    stop: &AtomicBool,
) -> Result<()> {
    let connection = crate::store::id();
    emit(
        tx,
        stop,
        Input::Connected {
            connection: connection.clone(),
            peer: peer.clone(),
        },
    );
    let mut buf = [0; 64 * 1024];
    while !stop.load(Ordering::Relaxed) {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let received_at = Instant::now();
                let received_utc = crate::store::utc();
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                if !emit(
                    tx,
                    stop,
                    Input::Bytes {
                        connection: connection.clone(),
                        peer: peer.clone(),
                        bytes: buf[..n].to_vec(),
                        reply: c.acknowledge.then_some(reply_tx),
                        received_utc,
                        received_at,
                    },
                ) {
                    break;
                }
                if c.acknowledge {
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        match reply_rx.recv_timeout(Duration::from_millis(200)) {
                            Ok(bytes) => {
                                if !bytes.is_empty() {
                                    stream.write_all(&bytes)?;
                                    emit(
                                        tx,
                                        stop,
                                        Input::Status("ACK sent after durable capture".into()),
                                    );
                                }
                                break;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    emit(tx, stop, Input::Disconnected { connection });
    Ok(())
}
fn run(c: &SourceConfig, tx: &SyncSender<Input>, stop: &AtomicBool) -> Result<()> {
    match c.mode {
        Source::Notes => {
            emit(
                tx,
                stop,
                Input::Status("Notes and media session; no monitor connection".into()),
            );
        }
        Source::Demo => {
            emit(
                tx,
                stop,
                Input::Connected {
                    connection: "demo".into(),
                    peer: "synthetic".into(),
                },
            );
            let mut n = 0;
            while !stop.load(Ordering::Relaxed) {
                emit(
                    tx,
                    stop,
                    Input::Bytes {
                        connection: "demo".into(),
                        peer: "synthetic".into(),
                        bytes: protocol::demo(n),
                        reply: None,
                        received_utc: crate::store::utc(),
                        received_at: Instant::now(),
                    },
                );
                n += 1;
                for _ in 0..10 {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        Source::TcpConnect => {
            emit(
                tx,
                stop,
                Input::Status(format!("Connecting to {}:{}", c.host, c.port)),
            );
            let addr = SocketAddr::new(c.host.parse()?, c.port);
            let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
            stream.set_read_timeout(Some(Duration::from_millis(200)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            read_stream(stream, addr.to_string(), c, tx, stop)?;
        }
        Source::TcpListen => {
            let listener = TcpListener::bind(SocketAddr::new(c.bind.parse()?, c.port))?;
            listener.set_nonblocking(true)?;
            emit(
                tx,
                stop,
                Input::Status(format!(
                    "Listening on {}:{}; waiting for monitor",
                    c.bind, c.port
                )),
            );
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if !c.allow_any_peer && peer.ip() != c.host.parse::<IpAddr>()? {
                            emit(
                                tx,
                                stop,
                                Input::Status(format!("Rejected unconfigured peer {peer}")),
                            );
                            continue;
                        }
                        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                        if let Err(e) = read_stream(stream, peer.to_string(), c, tx, stop) {
                            emit(tx, stop, Input::Status(format!("Connection ended: {e}")));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(100))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Source::Udp => {
            let socket = UdpSocket::bind(SocketAddr::new(c.bind.parse()?, c.port))?;
            socket.set_read_timeout(Some(Duration::from_millis(200)))?;
            emit(
                tx,
                stop,
                Input::Status(format!("UDP listening on {}:{}", c.bind, c.port)),
            );
            let mut buf = [0; 65535];
            while !stop.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buf) {
                    Ok((n, peer)) => {
                        let received_at = Instant::now();
                        let received_utc = crate::store::utc();
                        if c.allow_any_peer || peer.ip() == c.host.parse::<IpAddr>()? {
                            let connection = crate::store::id();
                            emit(
                                tx,
                                stop,
                                Input::Bytes {
                                    connection: connection.clone(),
                                    peer: peer.to_string(),
                                    bytes: buf[..n].to_vec(),
                                    reply: None,
                                    received_utc,
                                    received_at,
                                },
                            );
                            emit(tx, stop, Input::Disconnected { connection });
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Source::Serial => {
            let port = serialport::new(&c.serial_port, c.baud)
                .timeout(Duration::from_millis(200))
                .open()?;
            read_stream(port, c.serial_port.clone(), c, tx, stop)?;
        }
    }
    Ok(())
}
