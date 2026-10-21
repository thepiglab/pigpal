//! Generic HL7 v2 / MLLP. Vendor codes are never guessed or converted.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub code: String,
    pub label: String,
    pub identifier_raw: String,
    pub sub_id: String,
    pub value_type: String,
    pub value_raw: String,
    pub value_text: String,
    pub numeric: Option<f64>,
    pub units: String,
    pub units_raw: String,
    pub result_status: String,
    pub abnormal_flags: String,
    pub device_time_raw: String,
    pub patient_raw: String,
    pub order_raw: String,
    pub source: String,
    pub repeat_index: usize,
    pub segment_raw: String,
}
impl Observation {
    pub fn key(&self) -> String {
        serde_json::to_string(&(
            &self.source,
            &self.patient_raw,
            &self.order_raw,
            &self.identifier_raw,
            &self.sub_id,
            &self.units_raw,
            self.repeat_index,
        ))
        .unwrap()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub control_id: String,
    pub message_type: String,
    pub text: String,
    pub observations: Vec<Observation>,
    pub warnings: Vec<String>,
    pub repeated_control_id: bool,
}

#[derive(Default)]
pub struct Framer {
    active: bool,
    pending_end: bool,
    bytes: Vec<u8>,
}
impl Framer {
    /// Bounded to 1 MiB. Broken frames remain available in wire.bin.
    pub fn push(&mut self, data: &[u8]) -> (Vec<Vec<u8>>, usize) {
        let mut frames = Vec::new();
        let mut discarded = 0;
        for &b in data {
            if b == 0x0b {
                if self.active {
                    discarded += 1;
                }
                self.active = true;
                self.pending_end = false;
                self.bytes.clear();
                continue;
            }
            if !self.active {
                continue;
            }
            if self.pending_end {
                if b == 0x0d {
                    frames.push(std::mem::take(&mut self.bytes));
                } else {
                    discarded += 1;
                }
                self.active = false;
                self.pending_end = false;
                continue;
            }
            if b == 0x1c {
                self.pending_end = true;
                continue;
            }
            self.bytes.push(b);
            if self.bytes.len() > 1024 * 1024 {
                self.bytes.clear();
                self.active = false;
                discarded += 1;
            }
        }
        (frames, discarded)
    }
    pub fn has_partial(&self) -> bool {
        self.active
    }
}
pub fn wrap(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len() + 3);
    bytes.push(11);
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&[28, 13]);
    bytes
}
fn field<'a>(parts: &'a [&str], i: usize) -> &'a str {
    parts.get(i).copied().unwrap_or("")
}
fn header(text: &str) -> Result<(char, [char; 4], Vec<&str>)> {
    let line = text.split(['\r', '\n']).next().unwrap_or("");
    ensure!(
        line.starts_with("MSH") && line.len() > 4,
        "Missing MSH header; raw bytes retained"
    );
    let sep = line.chars().nth(3).unwrap();
    let parts: Vec<_> = line.split(sep).collect();
    let enc: Vec<_> = field(&parts, 1).chars().collect();
    ensure!(parts.len() >= 12 && enc.len() >= 4, "Incomplete MSH header");
    let separators = [sep, enc[0], enc[1], enc[2], enc[3]];
    ensure!(
        separators.iter().all(|c| c.is_ascii_graphic())
            && separators
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == 5,
        "Invalid HL7 separators"
    );
    Ok((sep, [enc[0], enc[1], enc[2], enc[3]], parts))
}
pub fn parse(bytes: &[u8]) -> Result<Message> {
    let latin: String = bytes.iter().map(|&b| char::from(b)).collect();
    let (_, _, h) = header(&latin)?;
    let charset = field(&h, 17).to_ascii_uppercase();
    let text = match charset.as_str() {
        "" | "ASCII" | "UNICODE UTF-8" | "UTF-8" => std::str::from_utf8(bytes)?.to_string(),
        "8859/1" | "ISO IR100" | "ISO-8859-1" => latin,
        _ => bail!("Unsupported charset {charset}; raw bytes retained"),
    };
    let (sep, enc, h) = header(&text)?;
    ensure!(!field(&h, 8).is_empty(), "Missing message type");
    let mut observations = Vec::new();
    let mut warnings = Vec::new();
    let mut patient = String::new();
    let mut order = String::new();
    for line in text.split(['\r', '\n']).filter(|s| !s.is_empty()) {
        let f: Vec<_> = line.split(sep).collect();
        match field(&f, 0) {
            "PID" => {
                patient = field(&f, 3).into();
                order.clear();
            }
            "OBR" => order = format!("{}|{}", field(&f, 2), field(&f, 3)),
            "OBX" => {
                if f.len() < 6 {
                    warnings.push("Short OBX retained without interpretation".into());
                    continue;
                }
                let ident: Vec<_> = field(&f, 3).split(enc[0]).collect();
                for (repeat, raw) in field(&f, 5).split(enc[1]).enumerate() {
                    let decoded = unescape(raw, sep, enc);
                    let numeric = if field(&f, 2) == "NM" {
                        strict_number(&decoded)
                    } else {
                        None
                    };
                    if field(&f, 2) == "NM" && !raw.is_empty() && numeric.is_none() {
                        warnings.push(format!("Non-numeric NM retained: {}", field(&f, 3)));
                    }
                    observations.push(Observation {
                        code: field(&ident, 0).into(),
                        label: unescape(
                            if field(&ident, 1).is_empty() {
                                field(&ident, 0)
                            } else {
                                field(&ident, 1)
                            },
                            sep,
                            enc,
                        ),
                        identifier_raw: field(&f, 3).into(),
                        sub_id: field(&f, 4).into(),
                        value_type: field(&f, 2).into(),
                        value_raw: raw.into(),
                        value_text: decoded,
                        numeric,
                        units: field(&f, 6).split(enc[0]).next().unwrap_or("").into(),
                        units_raw: field(&f, 6).into(),
                        result_status: field(&f, 11).into(),
                        abnormal_flags: field(&f, 8).into(),
                        device_time_raw: field(&f, 14).into(),
                        patient_raw: patient.clone(),
                        order_raw: order.clone(),
                        source: format!("{}|{}", field(&h, 2), field(&h, 3)),
                        repeat_index: repeat,
                        segment_raw: line.into(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(Message {
        control_id: field(&h, 9).into(),
        message_type: field(&h, 8).into(),
        observations,
        warnings,
        repeated_control_id: false,
        text,
    })
}
pub fn strict_number(s: &str) -> Option<f64> {
    if s.is_empty()
        || s.trim() != s
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
    {
        return None;
    }
    s.parse::<f64>().ok().filter(|n| n.is_finite())
}
fn unescape(s: &str, sep: char, enc: [char; 4]) -> String {
    let parts: Vec<_> = s.split(enc[2]).collect();
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 0 {
            out.push_str(p);
            continue;
        }
        if i == parts.len() - 1 {
            out.push(enc[2]);
            out.push_str(p);
            continue;
        }
        match *p {
            "F" => out.push(sep),
            "S" => out.push(enc[0]),
            "R" => out.push(enc[1]),
            "E" => out.push(enc[2]),
            "T" => out.push(enc[3]),
            ".br" => out.push('\n'),
            _ => {
                out.push(enc[2]);
                out.push_str(p);
                out.push(enc[2]);
            }
        }
    }
    out
}
/// Only original-mode ORU acknowledgment, after the caller durably stores the message.
pub fn acknowledgment(msg: &Message) -> Option<Vec<u8>> {
    let (sep, enc, h) = header(&msg.text).ok()?;
    if field(&h, 9).is_empty() || !field(&h, 14).is_empty() || !field(&h, 15).is_empty() {
        return None;
    }
    let ty: Vec<_> = field(&h, 8).split(enc[0]).collect();
    if field(&ty, 0) != "ORU" {
        return None;
    }
    let ack = format!("ACK{}{}", enc[0], field(&ty, 1));
    let time = chrono::Utc::now().format("%Y%m%d%H%M%S+0000").to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let line = [
        "MSH",
        field(&h, 1),
        "PigPal",
        "ResearchCapture",
        field(&h, 2),
        field(&h, 3),
        &time,
        "",
        &ack,
        &id,
        field(&h, 10),
        field(&h, 11),
    ]
    .join(&sep.to_string());
    let text = format!("{line}\rMSA{sep}AA{sep}{}\r", msg.control_id);
    // Avoid misencoding routing fields for Latin-1 senders.
    if matches!(
        field(&h, 17).to_ascii_uppercase().as_str(),
        "8859/1" | "ISO IR100" | "ISO-8859-1"
    ) {
        let bytes: Option<Vec<u8>> = text.chars().map(|c| u8::try_from(c as u32).ok()).collect();
        Some(wrap(&bytes?))
    } else {
        Some(wrap(text.as_bytes()))
    }
}
pub fn demo(n: u64) -> Vec<u8> {
    let time = chrono::Utc::now().format("%Y%m%d%H%M%S+0000");
    wrap(format!("MSH|^~\\&|SYNTHETIC|BENCH|||{time}||ORU^R01|DEMO-{n}|P|2.5\rPID|||SYNTHETIC\rOBR|1|DEMO|DEMO\rOBX|1|NM|HR^Heart rate^DEMO||{}|bpm|||||F\rOBX|2|NM|SPO2^Oxygen saturation^DEMO||{}|%|||||F\rOBX|3|NM|SYS^Systolic pressure^DEMO||{}|mmHg|||||F\rOBX|4|NM|DIA^Diastolic pressure^DEMO||{}|mmHg|||||F\r",72+n%7,97+n%3,116+n%9,72+n%5).as_bytes())
}
