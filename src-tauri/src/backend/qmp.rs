//! Minimaler QMP-Client (QEMU Machine Protocol) über TCP auf 127.0.0.1.
//! QEMU erlaubt nur eine Verbindung gleichzeitig – wir verbinden uns daher pro Befehl neu.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::logger;

pub struct Qmp {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Qmp {
    /// `read_timeout`: wie lange auf Antworten gewartet wird (savevm kann dauern).
    pub fn connect(port: u16, read_timeout: Duration) -> std::io::Result<Self> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(400))?;
        stream.set_read_timeout(Some(read_timeout))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let writer = stream.try_clone()?;
        let mut qmp = Self { reader: BufReader::new(stream), writer };
        // Begrüßung lesen, dann in den Befehlsmodus wechseln
        qmp.read_message()?;
        qmp.send_raw(&json!({ "execute": "qmp_capabilities" }))?;
        Ok(qmp)
    }

    fn read_message(&mut self) -> std::io::Result<Value> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "QMP-Verbindung geschlossen"));
            }
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                return Ok(v);
            }
        }
    }

    /// Sendet einen Befehl und wartet auf die Antwort (Ereignisse werden übersprungen).
    fn send_raw(&mut self, cmd: &Value) -> std::io::Result<Value> {
        let text = format!("{cmd}\r\n");
        self.writer.write_all(text.as_bytes())?;
        loop {
            let msg = self.read_message()?;
            if msg.get("return").is_some() || msg.get("error").is_some() {
                return Ok(msg);
            }
        }
    }

    pub fn execute(&mut self, command: &str, arguments: Option<Value>) -> AppResult<Value> {
        let mut cmd = json!({ "execute": command });
        if let Some(a) = arguments {
            cmd["arguments"] = a;
        }
        logger::write("QMP", &cmd.to_string());
        let res = self.send_raw(&cmd).map_err(|e| {
            AppError::new("Die Verbindung zur VM ist abgebrochen", "Prüfe, ob das QEMU-Fenster noch geöffnet ist.")
                .with_details(e.to_string())
        })?;
        if let Some(err) = res.get("error") {
            let desc = err.get("desc").and_then(|d| d.as_str()).unwrap_or("Unbekannter Fehler").to_string();
            logger::error(&format!("QMP {command}: {desc}"));
            return Err(AppError::new(
                "QEMU hat den Befehl abgelehnt",
                "Versuche es noch einmal. Bleibt der Fehler, schau in die Log-Datei.",
            )
            .with_details(desc));
        }
        Ok(res["return"].clone())
    }

    /// Befehl für den klassischen Monitor (savevm/loadvm/delvm). Diese melden Fehler als Text.
    pub fn hmp(&mut self, command_line: &str) -> AppResult<String> {
        let out = self.execute("human-monitor-command", Some(json!({ "command-line": command_line })))?;
        let text = out.as_str().unwrap_or("").trim().to_string();
        if !text.is_empty() {
            logger::write("QMP-HMP", &text);
        }
        let lower = text.to_lowercase();
        if lower.contains("error") || lower.contains("could not") || lower.contains("failed") || lower.contains("does not support") {
            return Err(AppError::new(
                "QEMU konnte den Sicherungspunkt nicht verarbeiten",
                "Sicherungspunkte funktionieren nur mit Festplatten im qcow2-Format, die Nestbox selbst angelegt hat.",
            )
            .with_details(text));
        }
        Ok(text)
    }

    /// running | paused | … (siehe QMP query-status)
    pub fn status(&mut self) -> AppResult<String> {
        let v = self.execute("query-status", None)?;
        Ok(v.get("status").and_then(|s| s.as_str()).unwrap_or("unknown").to_string())
    }
}

/// Einen freien TCP-Port auf 127.0.0.1 finden.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(44_444)
}

pub fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}
