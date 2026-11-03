//! The pairing window's camera in scripted runs of the whole app
//! (docs/design/20261028_02_PAIRING_QR_SCAN_PLAN.md, section 5): libmorph's
//! scripted camera (`camera-qr`, `camera-deny`, `camera-deny-forever`,
//! `tap`, `touch`, `suspend`, `resume`) drives [`App`] at the Galaxy S24's
//! size, started with `--camera` to have the Android window on the host.
//! What the app did is read back from [`probe`]: the camera events in order
//! and the window's texts. The scripted viewfinder draws nothing (libmorph
//! gives scripted runs no preview frames), so these check state and
//! controls, not the image. Each run needs a GPU adapter; without one it
//! says so and passes.

use std::sync::mpsc;
use std::time::Duration;

use libflankr_proto::link::{
    self, Incoming, OFFER_LEN, PairAck, PairRequest, PairResult, PairingPayload, StaticKeypair,
};
use libmorph::script::{OutDir, Script, ScriptRun};
use libmorph::{GpuError, MorphError};

use super::App;
use super::probe::{self, Probe};
use crate::ui::Hit;

/// Runs `script` on a fresh app with `args`; `None` without a GPU. Its
/// keys are files in a directory of its own (`--keys`), and its data
/// directory too (`--data`: settings, each remote's state), so the host's
/// key store and data, and any remote paired there, are neither read nor
/// changed.
fn run(script: &str, args: &[&str]) -> Option<Probe> {
    run_in(&tempfile::tempdir().unwrap(), script, args)
}

/// As [`run`], in `dir`, which the test can look in afterwards.
fn run_in(dir: &tempfile::TempDir, script: &str, args: &[&str]) -> Option<Probe> {
    let keys = dir.path().join("keys");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let run = ScriptRun {
        spec: libmorph::libdevice::GALAXY_S24,
        out: OutDir::Exact(dir.path().join("out")),
        overlay: false,
        fallback_adapter: false,
        args: std::iter::once("companion")
            .chain(args.iter().copied())
            .chain(["--keys", keys.to_str().unwrap()])
            .chain(["--data", data.to_str().unwrap()])
            .map(String::from)
            .collect(),
    };
    probe::take();
    let script = Script::parse(script).unwrap();
    match libmorph::script::run::<App>(&script, run) {
        Err(MorphError::Gpu(GpuError::Adapter(error))) => {
            eprintln!("skipped: no GPU adapter ({error})");
            None
        }
        result => {
            result.unwrap();
            Some(probe::take())
        }
    }
}

/// A scripted run's data, the app's and the client's (each remote's
/// `state.redb`), are in the test's own directory, never the host's.
#[test]
fn a_scripted_run_keeps_its_data_in_the_tests_directory() {
    let dir = tempfile::tempdir().unwrap();
    let Some(p) = run_in(&dir, "frames 2\n", &[]) else {
        return;
    };
    let data = dir.path().join("data");
    assert_eq!(p.data_dirs, Some((Some(data.clone()), Some(data))));
}

/// The window as it opens with the camera, for its controls' places.
fn opened() -> Option<Probe> {
    run("frames 2\n", &["--camera"])
}

fn tap((x, y): (f32, f32)) -> String {
    format!("tap {x} {y}\n")
}

/// The camera events of a run, without the others.
fn camera(p: &Probe) -> Vec<&str> {
    p.log
        .iter()
        .filter_map(|l| l.strip_prefix("camera "))
        .collect()
}

/// Run 1 (the 2026-11-02 22:46 EST amendment): the first-run window opens
/// the camera at once, with no tap; then a code that is not a pairing link
/// is ignored while the scan goes on.
#[test]
fn the_window_opens_the_camera_at_once_then_ignores_other_codes() {
    let Some(first) = opened() else { return };
    assert_eq!(camera(&first), ["opened 1"], "{:?}", first.log);
    assert_eq!(first.state, "Running(ScanId(1))");
    assert!(first.shows("Point at the QR code"), "{:?}", first.texts);
    assert!(!first.shows("Tap to scan"), "{:?}", first.texts);
    assert!(first.shows("Paste a link"));
    assert!(!first.shows("paste a pairing URL"), "no field yet");
    assert!(!first.shows("not paired"), "no status lines");
    let p = run(
        "frames 2\ncamera-qr https://example.com\nframes 1\n",
        &["--camera"],
    )
    .unwrap();
    assert_eq!(camera(&p), ["opened 1", "qr 1 19 bytes"], "{:?}", p.log);
    assert_eq!(p.state, "Running(ScanId(1))");
    assert!(p.shows("Not a flankr pairing code."), "{:?}", p.texts);
    assert!(!p.shows("Tap to scan"));
}

/// What the test daemon does with a pairing request once its words are
/// out: holds it until the phone closes it (the owner has not typed yes),
/// adds the phone (yes), or refuses it (no).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Answer {
    Hold,
    Add,
    Refuse,
}

/// The test daemon in pair mode: a `flankr pair` run on 127.0.0.1, as
/// flankr-companion's tests/link.rs builds one. It hands each pairing
/// request's words to the test, then answers it as `answer` says.
struct Daemon {
    url: String,
    /// `127.0.0.1:<port>`.
    addr: String,
    held: mpsc::Receiver<String>,
}

impl Daemon {
    fn start() -> Daemon {
        Daemon::answering(Answer::Hold)
    }

    fn answering(answer: Answer) -> Daemon {
        let key = link::generate_keypair().unwrap();
        let offer = link::generate_offer().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let url = PairingPayload {
            server_key: key.public,
            addresses: vec![addr.clone()],
            offer: Some(offer),
        }
        .url()
        .unwrap();
        let (held_tx, held) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                while let Ok((stream, _)) = listener.accept().await {
                    let (key, held) = (key.clone(), held_tx.clone());
                    tokio::spawn(serve(stream, key, offer, held, answer));
                }
            });
        });
        Daemon { url, addr, held }
    }
}

/// One connection: a pairing request with the right offer is held (its
/// words go to the test) until the phone closes it; anything else is
/// dropped.
async fn serve(
    stream: tokio::net::TcpStream,
    key: StaticKeypair,
    offer: [u8; OFFER_LEN],
    held: mpsc::Sender<String>,
    answer: Answer,
) {
    let Ok(Incoming::Pair(pair)) = link::accept_incoming(stream, &key, |_| false).await else {
        return;
    };
    let request: PairRequest = match &pair.request {
        Ok(request) if request.offer == offer => request.clone(),
        _ => return,
    };
    let Ok(mut stream) = pair
        .respond(PairAck::Accepted {
            expires_in_secs: 60,
        })
        .await
    else {
        return;
    };
    let words = link::pairing_words_text(&key.public, &offer, &request.device_key);
    let _ = held.send(words);
    let result = match answer {
        // Held, as the owner has not typed yes; until the phone closes it.
        Answer::Hold => {
            let _ = stream.recv_message().await;
            PairResult::Refused
        }
        Answer::Add => PairResult::Added,
        Answer::Refuse => PairResult::Refused,
    };
    let _ = link::send_pair_result(&mut stream, result).await;
    // Kept open until the phone has read the result.
    let _ = stream.recv_message().await;
}

/// Scans the daemon's code: a tap on the area, the code, and `frames`.
fn scan(first: &Probe, daemon: &Daemon, frames: u32) -> String {
    format!(
        "frames 2\n{}frames 1\ncamera-qr {}\nframes {frames}\n",
        tap(first.at(Hit::Area)),
        daemon.url
    )
}

/// Run 2: a real pairing URL from a daemon in pair mode, scanned: the scan
/// stops, the window says the code was read and where it connects, then
/// the six words the daemon has show in a modal.
#[test]
fn a_scanned_pairing_url_stops_the_scan_and_shows_the_daemons_words() {
    let Some(first) = opened() else { return };
    let daemon = Daemon::start();
    let script = format!(
        "frames 2\n{}frames 1\ncamera-qr {}\nframes 45\n",
        tap(first.at(Hit::Area)),
        daemon.url
    );
    let p = run(&script, &["--camera"]).unwrap();
    let words = daemon
        .held
        .recv_timeout(Duration::from_secs(5))
        .expect("the daemon held a pairing request");
    let qr = format!("qr 1 {} bytes", daemon.url.len());
    assert_eq!(
        camera(&p),
        ["opened 1", qr.as_str(), "closed 1"],
        "{:?}",
        p.log
    );
    assert_eq!(p.state, "Idle");
    let words: Vec<&str> = words.split_whitespace().collect();
    assert_eq!(words.len(), 6);
    let (one, two) = (words[..3].join(" "), words[3..].join(" "));
    assert!(
        p.texts.contains(&one) && p.texts.contains(&two),
        "the daemon's words {words:?} on {:?}",
        p.texts
    );
    assert!(p.shows("If flankr pair on 127.0.0.1"), "{:?}", p.texts);
    assert!(p.shows("Details") && p.shows("Cancel"));
    // The camera is not shown again during the pairing.
    assert!(!p.shows("Tap to scan") && !p.shows("Paste a link"));
    // The words are in the modal, titled, with Details and Cancel only.
    assert!(p.shows("Confirm on your Mac"), "{:?}", p.texts);
    let hits: Vec<Hit> = p.hits.iter().map(|&(h, _)| h).collect();
    assert_eq!(hits, [Hit::Details, Hit::Cancel]);

    // Cancel in the modal: the pairing ends, and the window says so with
    // Tap to scan.
    let script = format!(
        "frames 2\n{}frames 1\ncamera-qr {}\nframes 45\n{}frames 2\n",
        tap(first.at(Hit::Area)),
        daemon.url,
        tap(p.at(Hit::Cancel))
    );
    let c = run(&script, &["--camera"]).unwrap();
    assert!(
        c.shows("Pairing cancelled: nothing was added."),
        "{:?}",
        c.texts
    );
    assert!(c.shows("Tap to scan") && !c.shows("Confirm on your Mac"));
    // A cancelled pairing does not reopen the window: no new scan.
    assert!(!camera(&c).contains(&"opened 2"), "{:?}", c.log);
}

/// A scanned code whose daemon has not answered yet (a port that takes
/// the connection and says nothing): the camera stops and the window says
/// the code was read and where it is connecting, with Cancel, which ends it.
#[test]
fn a_scanned_code_shows_read_and_connecting_until_the_daemon_answers() {
    let Some(first) = opened() else { return };
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = PairingPayload {
        server_key: link::generate_keypair().unwrap().public,
        addresses: vec![silent.local_addr().unwrap().to_string()],
        offer: Some(link::generate_offer().unwrap()),
    }
    .url()
    .unwrap();
    let script = format!(
        "frames 2\n{}frames 1\ncamera-qr {url}\nframes 4\n",
        tap(first.at(Hit::Area))
    );
    let p = run(&script, &["--camera"]).unwrap();
    assert_eq!(p.state, "Idle", "the camera stopped");
    assert!(p.shows("Code read"), "{:?}", p.texts);
    assert!(p.shows("Connecting to 127.0.0.1\u{2026}"), "{:?}", p.texts);
    assert!(!p.shows("pairing\u{2026}") && !p.shows("Tap to scan"));
    let hits: Vec<Hit> = p.hits.iter().map(|&(h, _)| h).collect();
    assert_eq!(hits, [Hit::Cancel], "no Tap to scan on the check");
    let script = format!("{script}{}frames 2\n", tap(p.at(Hit::Cancel)));
    let c = run(&script, &["--camera"]).unwrap();
    assert!(
        c.shows("Pairing cancelled: nothing was added."),
        "{:?}",
        c.texts
    );
    assert!(c.shows("Tap to scan"));
    drop(silent);
}

/// The owner types yes: the modal closes, and the window says it is
/// paired, with the address and the fingerprint, until Done, which goes on
/// to the overview.
#[test]
fn a_confirmed_pairing_shows_paired_until_done() {
    let Some(first) = opened() else { return };
    let daemon = Daemon::answering(Answer::Add);
    let p = run(&scan(&first, &daemon, 60), &["--camera"]).unwrap();
    daemon
        .held
        .recv_timeout(Duration::from_secs(5))
        .expect("the daemon had a pairing request");
    assert!(p.shows("Paired with 127.0.0.1"), "{:?}", p.texts);
    let reached = format!("{} \u{b7} fingerprint ", daemon.addr);
    assert!(p.shows(&reached), "{:?}", p.texts);
    assert!(!p.shows("Confirm on your Mac"));
    let hits: Vec<Hit> = p.hits.iter().map(|&(h, _)| h).collect();
    assert_eq!(hits, [Hit::Details, Hit::Done], "no Tap to scan");
    // Still there many frames later: it waits for Done.
    let script = format!(
        "{}{}frames 2\n",
        scan(&first, &daemon, 60),
        tap(p.at(Hit::Details))
    );
    let d = run(&script, &["--camera"]).unwrap();
    assert!(d.shows("Paired with 127.0.0.1"), "{:?}", d.texts);
    assert!(d.shows("Daemon fingerprint"), "{:?}", d.texts);
    assert!(d.shows("BLAKE2s-256 fingerprint"), "{:?}", d.texts);
    assert!(d.shows("Hide details"));
    // Done: on to the overview.
    let script = format!(
        "{}{}frames 2\n",
        scan(&first, &daemon, 60),
        tap(p.at(Hit::Done))
    );
    let done = run(&script, &["--camera"]).unwrap();
    assert!(
        done.log.iter().any(|l| l == "pairing done, overview"),
        "{:?}",
        done.log
    );
}

/// The owner types no: the modal closes and the window keeps the reason,
/// with Tap to scan again.
#[test]
fn a_refused_pairing_keeps_the_window_with_its_reason() {
    let Some(first) = opened() else { return };
    let daemon = Daemon::answering(Answer::Refuse);
    let p = run(&scan(&first, &daemon, 60), &["--camera"]).unwrap();
    assert!(
        p.shows("not confirmed on 127.0.0.1: nothing was added"),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Tap to scan again"), "{:?}", p.texts);
    assert!(!p.shows("Confirm on your Mac") && !p.shows("Paired with"));
    assert!(p.hits.iter().any(|(h, _)| *h == Hit::Area));
}

/// Runs 3 and 4 (the 2026-11-02 22:46 EST amendment): the window opened
/// with CAMERA not granted asks for it (the scripted camera answers the
/// opening's `start_qr` as Android's request would); refused, the window
/// says the camera is off, still pastes, and asks again on a tap; Open
/// Settings only when Android will not ask again.
#[test]
fn opened_without_camera_permission_it_asks_and_a_refusal_says_the_camera_is_off() {
    let script = "camera-deny\nframes 2\n";
    let Some(p) = run(script, &["--camera"]) else {
        return;
    };
    assert_eq!(camera(&p), ["denied 1 settings_only=false"], "{:?}", p.log);
    assert!(
        p.shows("Camera is off for flankr. Tap to scan to be asked again"),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Tap to scan") && p.shows("Paste a link"));
    assert!(!p.shows("Open Settings"));
    let area = tap(p.at(Hit::Area));
    let again = run(&format!("{script}{area}frames 1\n"), &["--camera"]).unwrap();
    assert_eq!(
        camera(&again),
        ["denied 1 settings_only=false", "opened 2"],
        "a tap asks again"
    );
    // Refused, the link can still be pasted.
    let paste = run(
        &format!("{script}{}frames 1\n", tap(p.at(Hit::PasteLink))),
        &["--camera"],
    )
    .unwrap();
    assert!(
        paste.shows("paste a pairing URL") && paste.shows("Pair"),
        "{:?}",
        paste.texts
    );

    let script = "camera-deny-forever\nframes 2\n";
    let p = run(script, &["--camera"]).unwrap();
    assert_eq!(camera(&p), ["denied 1 settings_only=true"]);
    assert!(
        p.shows("Camera is off for flankr, and Android won't ask again"),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Open Settings") && p.shows("Paste a link"));
    // Open Settings: libmorph has no settings page off Android.
    let script = format!("{script}{}frames 1\n", tap(p.at(Hit::OpenSettings)));
    let p = run(&script, &["--camera"]).unwrap();
    assert!(
        p.log
            .iter()
            .any(|l| l == "open_app_settings Err(Unsupported)"),
        "{:?}",
        p.log
    );
}

/// Run 5: two fingers moving apart zoom in, through `zoom_by`, and the
/// ratio shows.
#[test]
fn a_pinch_zooms_the_scan() {
    let Some(first) = opened() else { return };
    let script = format!(
        "frames 2\n{}frames 1\n\
         touch 1 start 400 1000\ntouch 2 start 600 1000\nframes 1\n\
         touch 2 move 650 1000\nframes 1\ntouch 2 move 700 1000\nframes 1\n\
         touch 1 end 400 1000\ntouch 2 end 700 1000\nframes 1\n",
        tap(first.at(Hit::Area))
    );
    let p = run(&script, &["--camera"]).unwrap();
    let zooms: Vec<f32> = camera(&p)
        .iter()
        .filter_map(|l| l.strip_prefix("zoom 1 "))
        .map(|l| {
            assert!(l.ends_with(" app"), "{l}");
            l.split(' ').next().unwrap().parse().unwrap()
        })
        .collect();
    assert_eq!(zooms.len(), 2, "{:?}", p.log);
    assert!(zooms[0] > 1.0 && zooms[1] > zooms[0], "{zooms:?}");
    assert!((zooms[1] - 1.5).abs() < 0.01, "300 / 200 apart: {zooms:?}");
    assert!(p.shows("1.5\u{d7}"), "{:?}", p.texts);
    // The pinch's touches were no tap: the scan still runs.
    assert_eq!(p.state, "Running(ScanId(1))");
}

/// Run 6: the scan keeps running across a suspend; Back on the first-run
/// window stops it and returns to the placeholder; Paste a link stops it
/// and shows the field.
#[test]
fn the_scan_survives_a_suspend_and_back_or_paste_stops_it() {
    let Some(first) = opened() else { return };
    let area = tap(first.at(Hit::Area));
    let script = format!("frames 2\n{area}frames 1\nsuspend\nresume\nframes 2\n");
    let p = run(&script, &["--camera"]).unwrap();
    assert_eq!(camera(&p), ["opened 1", "suspended 1", "opened 1"]);
    assert_eq!(p.state, "Running(ScanId(1))");
    assert!(p.shows("Point at the QR code"));
    let script = format!("frames 2\n{area}frames 1\nkey back\nframes 1\n");
    let p = run(&script, &["--camera"]).unwrap();
    assert_eq!(camera(&p), ["opened 1", "closed 1"]);
    assert_eq!(p.state, "Idle");
    assert!(p.shows("Tap to scan"));
    let script = format!(
        "frames 2\n{area}frames 1\n{}frames 1\n",
        tap(first.at(Hit::PasteLink))
    );
    let p = run(&script, &["--camera"]).unwrap();
    assert_eq!(camera(&p), ["opened 1", "closed 1"]);
    assert!(
        p.shows("paste a pairing URL") && p.shows("Pair"),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Tap to scan"), "the placeholder over the field");
}

/// Run 7: without `--camera` (the desktop and the web), today's field and
/// Pair, and nothing of the camera.
#[test]
fn without_a_camera_the_window_is_the_paste_screen() {
    let Some(p) = run("frames 2\n", &[]) else {
        return;
    };
    assert!(p.shows("paste a pairing URL"), "{:?}", p.texts);
    assert!(p.texts.iter().any(|t| t == "Pair"));
    assert!(!p.shows("Tap to scan") && !p.shows("Paste a link"));
    assert!(p.shows("not paired"), "the status lines");
    assert!(!p.hits.iter().any(|(h, _)| *h == Hit::Area));
    assert!(p.log.is_empty());
}

/// A paired remote, for the add mode's runs.
fn remote(n: u8, name: &str, active: bool) -> flankr_companion::RemoteInfo {
    flankr_companion::RemoteInfo {
        id: flankr_companion::RemoteId([n; 8]),
        name: name.into(),
        host: format!("{name}.tail.ts.net:7347"),
        fingerprint: format!("{n:02x}{n:02x} {n:02x}{n:02x}"),
        paired_at: Some(1_761_600_000),
        active,
        online: true,
        fingerprint_hex: String::new(),
        fingerprint_words: String::new(),
        nickname: None,
        display_name: name.into(),
        last_session: None,
    }
}

/// The update on which [`run_paired`]'s remotes are given: after the
/// client's own first events (an empty list: no remote is paired in the
/// test's key directory), as in the drawer's tests.
const READY: u32 = 4;

/// As [`run`] with `--camera`, on a phone paired with m4 (in use) and
/// mini, connected from update [`READY`]. Until then the first-run window
/// shows, and opens the camera as scan 1, which closes when the overview
/// replaces it.
fn run_paired(script: &str) -> Option<Probe> {
    use flankr_companion::Event;
    probe::take();
    probe::injected(u32::MAX);
    probe::inject(
        READY,
        Event::Remotes(vec![remote(1, "m4", true), remote(2, "mini", false)]),
    );
    probe::inject(
        READY,
        Event::Connected {
            server: "m4".into(),
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join("keys");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let run = ScriptRun {
        spec: libmorph::libdevice::GALAXY_S24,
        out: OutDir::Exact(dir.path().join("out")),
        overlay: false,
        fallback_adapter: false,
        args: ["companion", "--camera", "--keys", keys.to_str().unwrap()]
            .into_iter()
            .chain(["--data", data.to_str().unwrap()])
            .map(String::from)
            .collect(),
    };
    let script = Script::parse(script).unwrap();
    match libmorph::script::run::<App>(&script, run) {
        Err(MorphError::Gpu(GpuError::Adapter(error))) => {
            eprintln!("skipped: no GPU adapter ({error})");
            None
        }
        result => {
            result.unwrap();
            Some(probe::take())
        }
    }
}

fn tap_rect(r: libcontrols::Rect) -> String {
    let [x, y] = r.center();
    format!("tap {x} {y}\n")
}

/// The camera events after the first-run window's scan 1 closed.
fn camera_after_start(p: &Probe) -> Vec<&str> {
    let events = camera(p);
    assert_eq!(events[..2], ["opened 1", "closed 1"], "{:?}", p.log);
    events[2..].to_vec()
}

/// The drawer opened from the overview's ≡, then its **+** ("pair another
/// remote"): the script's lines, ending on the add mode.
fn pair_another() -> Option<String> {
    let at = run_paired("frames 8\n")?;
    let menu = at.drawer.menu.expect("the overview's menu");
    let open = run_paired(&format!("frames 8\n{}frames 10\n", tap_rect(menu))).unwrap();
    let plus = *open.drawer.cards.last().expect("the drawer's +");
    Some(format!(
        "{}frames 10\n{}frames 4\n",
        tap_rect(menu),
        tap_rect(plus)
    ))
}

/// The owner's report (2026-11-02): "pair another" opens the camera at
/// once, as the first-run window does, with no tap; a phone already paired
/// starts no camera on the overview.
#[test]
fn pair_another_opens_the_camera_as_first_pairing_does() {
    let Some(again) = pair_another() else { return };
    let add = format!("frames 8\n{again}");
    let overview = run_paired("frames 8\n").unwrap();
    assert!(
        camera_after_start(&overview).is_empty(),
        "{:?}",
        overview.log
    );
    let p = run_paired(&add).unwrap();
    assert!(p.drawer.adding, "the add mode");
    assert_eq!(camera_after_start(&p), ["opened 2"], "{:?}", p.log);
    assert_eq!(p.state, "Running(ScanId(2))");
    assert!(p.shows("Point at the QR code"), "{:?}", p.texts);
    assert!(!p.shows("Tap to scan"));
    assert!(p.shows("Paste a link") && p.shows("Cancel"));
}

/// "Pair another" with CAMERA refused, as on the S23: Android is asked on
/// opening; refused, the window says the camera is off and pastes; closed
/// and opened again, it asks again; refused for good, Open Settings.
#[test]
fn pair_another_without_camera_permission_asks_on_every_opening() {
    let Some(again) = pair_another() else { return };
    // Refused from the add mode's opening on (the first-run window's scan
    // before the remotes came is granted).
    let add = format!("frames 8\ncamera-deny\n{again}");
    let denied = run_paired(&add).unwrap();
    assert_eq!(
        camera_after_start(&denied),
        ["denied 2 settings_only=false"]
    );
    assert!(
        denied.shows("Camera is off for flankr. Tap to scan to be asked again"),
        "{:?}",
        denied.texts
    );
    assert!(denied.shows("Tap to scan") && denied.shows("Paste a link"));
    assert!(!denied.shows("Open Settings"));
    // Cancel, back to the overview, and + again: asked again.
    let cancel = tap(denied.at(Hit::Cancel));
    let back = run_paired(&format!("{add}{cancel}frames 2\nkey back\nframes 4\n")).unwrap();
    assert!(
        !back.drawer.adding && !back.drawer.settings,
        "{:?}",
        back.drawer
    );
    let p = run_paired(&format!(
        "{add}{cancel}frames 2\nkey back\nframes 4\n{again}"
    ))
    .unwrap();
    assert_eq!(
        camera_after_start(&p),
        ["denied 2 settings_only=false", "opened 3"],
        "{:?}",
        p.log
    );
    let forever = run_paired(&format!("frames 8\ncamera-deny-forever\n{again}")).unwrap();
    assert_eq!(
        camera_after_start(&forever),
        ["denied 2 settings_only=true"]
    );
    assert!(
        forever.shows("Camera is off for flankr, and Android won't ask again"),
        "{:?}",
        forever.texts
    );
    assert!(forever.shows("Open Settings") && forever.shows("Paste a link"));
}

/// A link whose every address is a loopback one, scanned where those are
/// the phone itself (`--remote-loopback`, as on Android): nothing is
/// connected to and no device code is offered; the window says the code
/// works only on its own computer, and how to make it reachable (plan
/// 20261028_01, the 2026-11-03 00:07 EST amendment, item 1).
#[test]
fn a_loopback_only_code_is_refused_without_the_device_code() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = PairingPayload {
        server_key: link::generate_keypair().unwrap().public,
        addresses: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
        offer: Some(link::generate_offer().unwrap()),
    }
    .url()
    .unwrap();
    let script = format!("frames 2\ncamera-qr {url}\nframes 20\n");
    let Some(p) = run(&script, &["--camera", "--remote-loopback"]) else {
        return;
    };
    assert!(
        p.shows("This pairing code only works on the computer that made it"),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Tap to scan again"), "{:?}", p.texts);
    for device_code in [
        "couldn't be asked directly",
        "Paste this code",
        "flankrkey1",
    ] {
        assert!(!p.shows(device_code), "{device_code}: {:?}", p.texts);
    }
    assert!(!p.shows("Run **flankr pair** again") && !p.shows("Confirm on your Mac"));
    assert!(
        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "nothing connected to the loopback address"
    );
}

/// A link to an address that takes no connection: no device code, since
/// it would need the same address; the window says the daemon can't be
/// reached (the same amendment, item 2).
#[test]
fn an_unreachable_code_says_so_without_the_device_code() {
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().to_string()
    };
    let url = PairingPayload {
        server_key: link::generate_keypair().unwrap().public,
        addresses: vec![closed.clone()],
        offer: Some(link::generate_offer().unwrap()),
    }
    .url()
    .unwrap();
    let script = format!("frames 2\ncamera-qr {url}\nframes 20\n");
    let Some(p) = run(&script, &["--camera"]) else {
        return;
    };
    assert!(
        p.shows(&format!("Can't reach 127.0.0.1 at {closed}")),
        "{:?}",
        p.texts
    );
    assert!(p.shows("Tap to scan again"), "{:?}", p.texts);
    for device_code in [
        "couldn't be asked directly",
        "Paste this code",
        "flankrkey1",
    ] {
        assert!(!p.shows(device_code), "{device_code}: {:?}", p.texts);
    }
}
