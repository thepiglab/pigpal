use pigpal::{
    export,
    protocol::{self, Framer},
    recorder::Recorder,
    store::{self, Root},
    transport::{Source, SourceConfig},
};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Duration, Instant},
};

#[test]
fn every_mllp_fragment_boundary_and_multiple_frames() {
    let input = protocol::demo(1);
    for cut in 0..=input.len() {
        let mut f = Framer::default();
        let (mut a, e) = f.push(&input[..cut]);
        assert_eq!(e, 0);
        let (b, e) = f.push(&input[cut..]);
        a.extend(b);
        assert_eq!(e, 0);
        assert_eq!(a.len(), 1);
        let m = protocol::parse(&a[0]).unwrap();
        assert_eq!(m.observations.len(), 4);
        assert_eq!(m.observations[0].numeric, Some(73.0));
    }
    let mut joined = input.clone();
    joined.extend(input);
    assert_eq!(Framer::default().push(&joined).0.len(), 2);
}
#[test]
fn malformed_oversized_and_nested_frames_recover() {
    let mut f = Framer::default();
    assert_eq!(f.push(b"\x0bMSH\x1cx").1, 1);
    let mut huge = vec![11];
    huge.extend(vec![b'a'; 1024 * 1024 + 1]);
    assert_eq!(f.push(&huge).1, 1);
    let mut nested = b"\x0bbroken".to_vec();
    nested.extend(protocol::demo(3));
    let (frames, bad) = f.push(&nested);
    assert_eq!(frames.len(), 1);
    assert_eq!(bad, 1);
}
#[test]
fn obx_repeats_invalid_numbers_and_patient_boundaries_retained() {
    let m=protocol::parse(b"MSH|^~\\&|A|B|||20261019||ORU^R01|1|P|2.5\rPID|||P1\rOBR||O1|O2\rOBX|1|NM|x^Custom||12~NaN~13|u|||||F\rPID|||P2\rOBX|2|ST|x^Custom||A\\F\\B|u\r").unwrap();
    assert_eq!(m.observations.len(), 4);
    assert_eq!(m.observations[1].numeric, None);
    assert_eq!(m.observations[3].value_text, "A|B");
    assert_eq!(m.observations[3].order_raw, "");
    assert_ne!(m.observations[0].key(), m.observations[3].key());
    assert_eq!(protocol::strict_number(" 12"), None);
    assert_eq!(protocol::strict_number("1e309"), None);
    assert_eq!(protocol::strict_number("-1.2e2"), Some(-120.0));
}
#[test]
fn charset_and_ack_negotiation_are_conservative() {
    let bytes = protocol::demo(0);
    let message = protocol::parse(&bytes[1..bytes.len() - 2]).unwrap();
    assert!(protocol::acknowledgment(&message).is_some());
    let enhanced = protocol::parse(b"MSH|^~\\&|A|B|||20261019||ORU^R01|1|P|2.5|||AL|AL\r").unwrap();
    assert!(protocol::acknowledgment(&enhanced).is_none());
    let ack = protocol::parse(b"MSH|^~\\&|A|B|||20261019||ACK^R01|1|P|2.5\r").unwrap();
    assert!(protocol::acknowledgment(&ack).is_none());
    assert!(
        protocol::parse(b"MSH|^~\\&|A|B|||20261019||ORU^R01|1|P|2.5||||||UNICODE UTF-16\r")
            .is_err()
    );
}
fn root() -> (tempfile::TempDir, Root) {
    let temp = tempfile::tempdir().unwrap();
    let root = Root::open(temp.path().join("data")).unwrap();
    (temp, root)
}
#[test]
fn exclusive_workspace_lock_and_interrupted_recovery() {
    let (_temp, root) = root();
    assert!(Root::open(root.path.clone()).is_err());
    let s = root.create("Subject / ../../", "Test", true).unwrap();
    let rec = Recorder::start(&root, s, SourceConfig::default()).unwrap();
    let path = root.path.clone();
    drop(rec);
    drop(root);
    let recovered = Root::open(path).unwrap();
    let s = recovered.sessions().unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].state, "interrupted");
    assert_eq!(s[0].segments[0].state, "interrupted");
}
#[test]
fn notes_keep_original_anchor_and_resume_preserves_files() {
    let (_temp, root) = root();
    let s = root.create("SYNTHETIC", "Notes", true).unwrap();
    let mut rec = Recorder::start(&root, s, SourceConfig::default()).unwrap();
    let marker = rec.marker("Note opened", "", None).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    rec.marker("Note", "Later text", Some(&marker)).unwrap();
    rec.stop(&root, false).unwrap();
    let first = rec.journal.dir.join("journal.ndjson");
    let original = fs::read(&first).unwrap();
    let dir = root.session_dir(&rec.session.id);
    let mut resumed = Recorder::start(&root, rec.session.clone(), SourceConfig::default()).unwrap();
    assert!(resumed.mic.is_none());
    resumed.stop(&root, false).unwrap();
    assert_eq!(original, fs::read(first).unwrap());
    let mut notes = vec![];
    store::visit_events(&dir, |e| {
        if e.kind == "marker" || e.kind == "annotation" {
            notes.push(e);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(notes[1].data["amends_id"], marker);
    assert!(notes[1].elapsed_s > notes[0].elapsed_s);
    assert_eq!(resumed.session.segments.len(), 2);
}
fn connect(port: u16) -> TcpStream {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
            return s;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn wait(rec: &mut Recorder, predicate: impl Fn(&Recorder) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !predicate(rec) {
        rec.poll().unwrap();
        assert!(Instant::now() < end, "capture timeout: {}", rec.status);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn actual_tcp_fragmentation_durable_ack_and_raw_archive() {
    let (_temp, root) = root();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let c = SourceConfig {
        mode: Source::TcpListen,
        port,
        acknowledge: true,
        ..Default::default()
    };
    let s = root.create("Bench", "TCP", false).unwrap();
    let mut rec = Recorder::start(&root, s, c).unwrap();
    let mut stream = connect(port);
    let input = protocol::demo(5);
    let expected = input.clone();
    let sender = std::thread::spawn(move || {
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        for part in input.chunks(7) {
            stream.write_all(part).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut b = [0; 1024];
        let n = stream.read(&mut b).unwrap();
        b[..n].to_vec()
    });
    wait(&mut rec, |r| r.messages == 1);
    assert_eq!(
        fs::read(rec.journal.dir.join("wire.bin")).unwrap(),
        expected
    );
    assert!(
        String::from_utf8(fs::read(rec.journal.dir.join("journal.ndjson")).unwrap())
            .unwrap()
            .contains("DEMO-5")
    );
    let ack = sender.join().unwrap();
    assert!(String::from_utf8_lossy(&ack).contains("MSA|AA|DEMO-5"));
    rec.stop(&root, false).unwrap();
}
#[test]
fn raw_tcp_capture_tracks_receipt_without_fabricating_measurements() {
    let (_temp, root) = root();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let c = SourceConfig {
        mode: Source::TcpListen,
        port,
        hl7: false,
        ..Default::default()
    };
    let s = root.create("Bench", "Raw TCP", false).unwrap();
    let mut rec = Recorder::start(&root, s, c).unwrap();
    assert!(rec.last_received_elapsed_s.is_none());
    let mut stream = connect(port);
    let input = [1, 2, 0x5b, 0xf6, 0, 0x80, 0xff, 0, 0];
    stream.write_all(&input).unwrap();
    wait(&mut rec, |r| r.bytes == input.len() as u64);
    assert_eq!(rec.source().port, port);
    assert!(!rec.source().hl7);
    assert!(rec.last_received_elapsed_s.is_some());
    assert_eq!(rec.messages, 0);
    assert!(rec.latest.is_empty());
    assert_eq!(fs::read(rec.journal.dir.join("wire.bin")).unwrap(), input);
    drop(stream);
    rec.stop(&root, false).unwrap();
}
#[test]
fn waveline_binary_tcp_reassembles_records_without_promoting_hypotheses() {
    let (_temp, root) = root();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let c = SourceConfig {
        mode: Source::TcpListen,
        port,
        hl7: false,
        ..Default::default()
    };
    let s = root.create("Bench", "Binary TCP", false).unwrap();
    let mut rec = Recorder::start(&root, s, c).unwrap();
    let mut stream = connect(port);
    let mut record = vec![0; pigpal::waveline::RECORD_LEN];
    record[..4].copy_from_slice(&pigpal::waveline::MAGIC);
    let len = record.len() as u32;
    record[4..8].copy_from_slice(&len.to_le_bytes());
    record[14..16].copy_from_slice(&108_u16.to_le_bytes());
    record[64..66].copy_from_slice(&100_u16.to_le_bytes());
    record[36..48].copy_from_slice(&[73, 0, 0, 0, 33, 0, 0, 0, 46, 0, 0, 0]);
    let mut expected = record.clone();
    expected.extend(&record);
    let sender = std::thread::spawn(move || {
        for chunk in record.chunks(13) {
            stream.write_all(chunk).unwrap();
        }
        stream.write_all(&record).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        let error = stream.read(&mut [0; 32]).unwrap_err();
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            "Unexpected read result while checking that application ACKs are off: {error}"
        );
    });
    wait(&mut rec, |r| r.binary_records == 2);
    sender.join().unwrap();
    wait(&mut rec, |r| r.status.starts_with("Source disconnected"));
    let mut reconnected = connect(port);
    reconnected.write_all(&expected[..1956]).unwrap();
    expected.extend_from_within(..1956);
    wait(&mut rec, |r| r.binary_records == 3);
    assert_eq!(rec.binary_latest.as_ref().unwrap().stream_offset, 0);
    drop(reconnected);
    assert_eq!(
        rec.binary_latest.as_ref().unwrap().fields[0].value,
        Some(108.0)
    );
    assert_eq!(rec.messages, 0);
    assert!(rec.latest.is_empty());
    assert!(rec.history.is_empty());
    rec.marker("Reference", "Candidate fields remain separate", None)
        .unwrap();
    let marker = rec
        .events
        .iter()
        .rev()
        .find(|e| e.kind == "marker")
        .unwrap();
    assert_eq!(marker.data["context"], serde_json::json!([]));
    rec.stop(&root, false).unwrap();
    assert_eq!(
        fs::read(rec.journal.dir.join("wire.bin")).unwrap(),
        expected
    );
    let inspection = pigpal::waveline::inspect_file(&rec.journal.dir.join("wire.bin")).unwrap();
    assert_eq!(inspection.records, 3);
    assert_eq!(inspection.discarded_bytes, 0);
    let (events, _) = store::recent_events(&root.session_dir(&rec.session.id)).unwrap();
    let decoded: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "binary_record")
        .collect();
    assert_eq!(decoded.len(), 3);
    assert!(decoded.iter().all(|e| e.data["mapping_verified"] == false));
    assert!(decoded.iter().all(|e| e.data["arterial_pressure"]
        == serde_json::json!({
            "systolic":73.0, "diastolic":33.0, "mean":46.0
        })));
    let export_dir = export::csvs(&root.session_dir(&rec.session.id)).unwrap();
    for path in [
        rec.journal.dir.join("waveline_live.csv"),
        export_dir.join("arterial_pressure.csv"),
    ] {
        let mut csv = csv::Reader::from_path(path).unwrap();
        let headers = csv.headers().unwrap().clone();
        let indices = [
            "arterial_systolic_candidate",
            "arterial_diastolic_candidate",
            "arterial_mean_candidate",
        ]
        .map(|name| headers.iter().position(|header| header == name).unwrap());
        let rows = csv.records().map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        for row in rows {
            assert_eq!(indices.map(|index| &row[index]), ["73", "33", "46"]);
        }
    }
    let mut csv = csv::Reader::from_path(export_dir.join("waveline_candidates.csv")).unwrap();
    let fields: Vec<_> = csv.records().map(Result::unwrap).collect();
    assert_eq!(fields.len(), 27);
    assert_eq!(&fields[0][8], "14");
    assert_eq!(&fields[0][11], "108");
    assert_eq!(&fields[0][12], "6c 00");
    assert_eq!(&fields[9][6], "1956");
    assert_eq!(&fields[18][6], "0");
    assert_ne!(&fields[18][4], &fields[0][4]);
    assert!(fields.iter().all(|f| &f[13] == "true" && &f[14] == "false"));
    assert_eq!(
        csv::Reader::from_path(export_dir.join("observations.csv"))
            .unwrap()
            .records()
            .count(),
        0
    );
    let archive = root.path.join("exports/waveline.zip");
    export::archive(&root.session_dir(&rec.session.id), &archive, false).unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    assert!(archive.by_name("exports/waveline_candidates.csv").is_ok());
    let mut archived_wire = Vec::new();
    archive
        .by_name(&format!("segments/{}/wire.bin", rec.journal.segment_id))
        .unwrap()
        .read_to_end(&mut archived_wire)
        .unwrap();
    assert_eq!(archived_wire, expected);
}
#[test]
fn actual_udp_retains_datagrams_and_decodes() {
    let (_temp, root) = root();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let c = SourceConfig {
        mode: Source::Udp,
        port,
        ..Default::default()
    };
    let s = root.create("Bench", "UDP", false).unwrap();
    let mut rec = Recorder::start(&root, s, c).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .send_to(&protocol::demo(8), ("127.0.0.1", port))
        .unwrap();
    wait(&mut rec, |r| r.messages == 1);
    rec.stop(&root, false).unwrap();
}
#[test]
fn exports_are_valid_preserve_bytes_and_escape_spreadsheet_formulas() {
    let (_temp, root) = root();
    let s = root.create("Bench", "Export", false).unwrap();
    let mut rec = Recorder::start(
        &root,
        s,
        SourceConfig {
            mode: Source::Notes,
            ..Default::default()
        },
    )
    .unwrap();
    rec.marker("=1+1", "comma, quote\" and\nnewline", None)
        .unwrap();
    rec.stop(&root, false).unwrap();
    let dir = root.session_dir(&rec.session.id);
    let output = root.path.join("exports/all.zip");
    export::archive(&dir, &output, true).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let original = fs::read(dir.join(entry.name())).unwrap();
        let mut zipped = vec![];
        entry.read_to_end(&mut zipped).unwrap();
        assert_eq!(original, zipped);
    }
    let text = fs::read_to_string(dir.join("exports/events.csv")).unwrap();
    assert!(text.contains("'=1+1"));
    assert_eq!(export::safe_cell(" @formula"), "' @formula");
}
#[test]
fn partial_journal_tail_is_reported_without_rewriting() {
    let (_temp, root) = root();
    let s = root.create("Bench", "Tail", false).unwrap();
    let mut rec = Recorder::start(
        &root,
        s,
        SourceConfig {
            mode: Source::Notes,
            ..Default::default()
        },
    )
    .unwrap();
    rec.stop(&root, false).unwrap();
    let path = rec.journal.dir.join("journal.ndjson");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"incomplete\"").unwrap();
    drop(file);
    let before = fs::read(&path).unwrap();
    let warnings = store::visit_events(&root.session_dir(&rec.session.id), |_| Ok(())).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(before, fs::read(path).unwrap());
}
#[test]
fn live_export_is_rejected() {
    let (_temp, root) = root();
    let s = root.create("Bench", "Active", true).unwrap();
    let mut rec = Recorder::start(&root, s, SourceConfig::default()).unwrap();
    assert!(export::csvs(&root.session_dir(&rec.session.id)).is_err());
    rec.stop(&root, false).unwrap();
}

#[test]
fn synthetic_data_cannot_enter_a_real_session() {
    let (_temp, root) = root();
    let session = root.create("Bench", "Real source", false).unwrap();
    assert!(Recorder::start(&root, session, SourceConfig::default()).is_err());
}

#[test]
#[ignore = "Requires separately installed FFmpeg and ffprobe; uses generated video only"]
fn synthetic_video_is_finalized_and_decodable() {
    use pigpal::media::Video;
    let temp = tempfile::tempdir().unwrap();
    let mut video =
        Video::start(temp.path(), "lavfi", "testsrc2=size=320x240:rate=10", 0.0).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let metadata = video.stop().unwrap();
    assert_eq!(metadata["state"], "closed");
    let movie = walkdir::WalkDir::new(temp.path())
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().and_then(|x| x.to_str()) == Some("mkv"))
        .unwrap();
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(movie.path())
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames,codec_name",
            "-of",
            "json",
        ])
        .arg(movie.path())
        .output()
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["streams"][0]["codec_name"], "h264");
    assert!(
        info["streams"][0]["nb_read_frames"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 10
    );
}
