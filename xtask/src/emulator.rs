//! `cargo xtask verify-android-emulator`: the committed APK on a real Android
//! runtime. A headless emulator (`emulator`, `avdmanager` and `adb` are
//! external tools here, like the SDK, the JDK, qemu and Chromium) boots the
//! pinned x86_64 system image, whose ARM translation runs the shipped arm64
//! library, installs `HSE-BLE-Radar-arm64-v1.0.0.apk` with its runtime
//! permissions granted, launches the app, and drives the loopback API through
//! `adb forward`:
//!
//! * `/` serves the committed dashboard bytes; the three documents carry the
//!   documented keys, `native_available` is `true` (the cross-compiled
//!   library loaded on the device), the device list is empty;
//! * `POST /api/scan/start` is accepted (an adapter is present) or refused
//!   with a documented reason; when accepted, the service is in the
//!   foreground with the scanning notification, a stop pauses it with the
//!   idle notification and keeps it, and a `kill -9` of the app process is
//!   followed by the `START_STICKY` restart that resumes the scan;
//! * `am force-stop` ends the app and, with it, the API;
//! * the crash log carries nothing for the app.
//!
//! Needs KVM (the emulator is started with `-accel on`), so it runs in CI's
//! `android-emulator` job; a sandbox without `/dev/kvm` cannot run it.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::apilive::{self, HttpResponse};
use crate::dashboard::{self, SCAN_STARTED_JSON, SCAN_STOPPED_JSON};

/// The app's package, as `AndroidManifest.xml` declares it.
pub const PACKAGE: &str = "com.hse.bleradar";
const ACTIVITY: &str = "com.hse.bleradar/.MainActivity";
const SERVICE: &str = "com.hse.bleradar/.RadarScanService";
/// The console port the emulator is told to use; its adb serial follows.
const CONSOLE_PORT: u16 = 5554;
const AVD_NAME: &str = "bleradar-proof";
/// The port `ApiHttpServer.DEFAULT_PORT` binds inside the guest.
const GUEST_API_PORT: u16 = 8080;
const BOOT_TIMEOUT: Duration = Duration::from_secs(480);
const API_TIMEOUT: Duration = Duration::from_secs(90);
const RESTART_TIMEOUT: Duration = Duration::from_secs(90);
const ADB_TIMEOUT: Duration = Duration::from_secs(120);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
/// The notification titles `res/values/strings.xml` gives the two states.
const SCANNING_TITLE: &str = "BLE Radar is scanning";
const IDLE_TITLE: &str = "BLE Radar is idle";
/// The refusals a start may answer on a device without a usable adapter.
const ADAPTER_REFUSALS: [&str; 2] = [
    r#"{"error":"Bluetooth is off"}"#,
    r#"{"error":"Bluetooth LE scanner unavailable"}"#,
];

/// What the command needs from the caller.
pub struct Config<'a> {
    /// The repository root.
    pub root: &'a Path,
    /// The Android SDK (`emulator/`, `platform-tools/`, `cmdline-tools/`).
    pub sdk_root: &'a Path,
    /// The APK to install: the committed artifact.
    pub apk: &'a Path,
    /// The `sdkmanager` package of the system image the AVD is created from.
    pub image_package: &'a str,
}

// ===== pure helpers (unit-tested) =====

/// `getprop sys.boot_completed` answers `1` once the system has booted.
pub fn boot_completed(getprop: &str) -> bool {
    getprop.trim() == "1"
}

/// `adb forward tcp:0 tcp:<port>` prints the local port it allocated.
pub fn forwarded_port(output: &str) -> Option<u16> {
    output
        .lines()
        .rev()
        .find_map(|line| line.trim().parse().ok())
}

/// `adb install` prints `Success` on its own line.
pub fn install_succeeded(output: &str) -> bool {
    output.lines().any(|line| line.trim() == "Success")
}

/// `am start -W` prints `Status: ok` once the activity is up.
pub fn launch_succeeded(output: &str) -> bool {
    output.contains("Status: ok")
}

/// Whether `ro.product.cpu.abilist` lets the shipped arm64 library load.
pub fn abilist_runs_arm64(abilist: &str) -> bool {
    abilist.split(',').any(|abi| abi.trim() == "arm64-v8a")
}

/// The `ServiceRecord` block of `service` in `dumpsys activity services`
/// output: from its header line to the next record or the end.
pub fn service_record<'a>(dumpsys: &'a str, service: &str) -> Option<&'a str> {
    let header = format!("{service}}}");
    for (at, _) in dumpsys.match_indices("ServiceRecord{") {
        let rest = &dumpsys[at..];
        let line_len = rest.find('\n').unwrap_or(rest.len());
        if !rest[..line_len].contains(&header) {
            continue;
        }
        let end = rest[line_len..]
            .find("ServiceRecord{")
            .map_or(rest.len(), |next| line_len + next);
        return Some(&rest[..end]);
    }
    None
}

/// Whether `service` is running in the foreground, or `None` when it has no
/// record (it is not running at all).
pub fn service_is_foreground(dumpsys: &str, service: &str) -> Option<bool> {
    service_record(dumpsys, service).map(|record| record.contains("isForeground=true"))
}

/// Every `android.title` in `dumpsys notification --noredact` output.
pub fn notification_titles(dumpsys: &str) -> Vec<String> {
    dumpsys
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("android.title=String (")?;
            Some(rest.strip_suffix(')').unwrap_or(rest).to_string())
        })
        .collect()
}

/// The `FATAL EXCEPTION` headers in a crash log that belong to `package`
/// (the `Process:` line follows within a few lines).
pub fn crash_headers<'a>(logcat: &'a str, package: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = logcat.lines().collect();
    let process = format!("Process: {package}");
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            line.contains("FATAL EXCEPTION")
                && lines[*index..(*index + 4).min(lines.len())]
                    .iter()
                    .any(|following| following.contains(&process))
        })
        .map(|(_, line)| *line)
        .collect()
}

/// The integer value of `"key":` in a flat JSON object, if present.
pub fn json_integer(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)? + pattern.len();
    let digits: String = json[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().ok()
}

/// Whether the JSON text carries `"key":<literal>` verbatim.
pub fn json_has(json: &str, key: &str, literal: &str) -> bool {
    json.contains(&format!("\"{key}\":{literal}"))
}

/// The keys a document must carry, checked against what it does.
fn require_keys(label: &str, body: &[u8], expected: &[&str]) -> Result<(), String> {
    let text = String::from_utf8_lossy(body);
    let keys = dashboard::json_object_keys(&text);
    let expected: BTreeSet<String> = expected.iter().map(|k| (*k).to_string()).collect();
    if keys != expected {
        return Err(format!(
            "{label}: keys {keys:?} (expected {expected:?}); body: {text}"
        ));
    }
    Ok(())
}

// ===== adb =====

struct Adb {
    exe: PathBuf,
    serial: String,
}

impl Adb {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.exe);
        command.args(["-s", &self.serial]);
        command.args(args);
        command
    }

    /// Runs `adb <args>` with a timeout; the combined output when it exits 0.
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let outcome = dashboard::run_with_timeout(self.command(args), ADB_TIMEOUT)?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&outcome.stdout),
            String::from_utf8_lossy(&outcome.stderr)
        );
        if !outcome.status.success() {
            return Err(format!(
                "adb {} exited with {}: {}",
                args.join(" "),
                outcome.status,
                text.trim()
            ));
        }
        Ok(text)
    }

    fn shell(&self, command: &str) -> Result<String, String> {
        self.run(&["shell", command])
    }

    /// Like [`shell`](Self::shell), but a failure is an empty answer (for
    /// diagnostics and best-effort commands).
    fn shell_lenient(&self, command: &str) -> String {
        dashboard::run_with_timeout(self.command(&["shell", command]), ADB_TIMEOUT)
            .map(|outcome| String::from_utf8_lossy(&outcome.stdout).into_owned())
            .unwrap_or_default()
    }

    /// Whether `adb devices` lists this serial as `device` (booted enough for
    /// shell commands).
    fn is_online(&self) -> bool {
        let mut command = Command::new(&self.exe);
        command.arg("devices");
        dashboard::run_with_timeout(command, ADB_TIMEOUT)
            .map(|outcome| {
                String::from_utf8_lossy(&outcome.stdout)
                    .lines()
                    .any(|line| {
                        let mut parts = line.split_whitespace();
                        parts.next() == Some(self.serial.as_str()) && parts.next() == Some("device")
                    })
            })
            .unwrap_or(false)
    }
}

// ===== HTTP through the forward =====

fn get(port: u16, target: &str) -> Result<HttpResponse, String> {
    apilive::http_request(
        port,
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes(),
    )
}

fn post(port: u16, target: &str) -> Result<HttpResponse, String> {
    apilive::http_request(
        port,
        format!(
            "POST {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .as_bytes(),
    )
}

/// Polls `GET /api/status` until it answers `200`; the elapsed time.
fn wait_for_api(port: u16, timeout: Duration) -> Result<Duration, String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < timeout {
        match get(port, "/api/status") {
            Ok(response) if response.status == 200 => return Ok(started.elapsed()),
            Ok(response) => last = format!("status {}", response.status),
            Err(error) => last = error,
        }
        thread::sleep(Duration::from_secs(2));
    }
    Err(format!(
        "the API did not answer within {}s (last: {last})",
        timeout.as_secs()
    ))
}

fn body_text(response: &HttpResponse) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

// ===== the emulator =====

fn tool(sdk_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = sdk_root.join(relative);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} not found (install it with: sdkmanager --install $(cargo xtask android-sdk-packages --emulator))",
            path.display()
        ))
    }
}

/// Creates the AVD under `avd_home`, which every tool is pointed at
/// explicitly: `avdmanager` and the emulator otherwise resolve the AVD
/// directory differently (`ANDROID_USER_HOME`/`XDG_CONFIG_HOME` versus
/// `$HOME/.android/avd`), which on a CI runner left the emulator unable to
/// find the AVD that had just been created. Returns the tool's output.
fn create_avd(
    sdk_root: &Path,
    avdmanager: &Path,
    avd_home: &Path,
    image_package: &str,
) -> Result<String, String> {
    let mut command = Command::new(avdmanager);
    command
        .args([
            "create",
            "avd",
            "--force",
            "--name",
            AVD_NAME,
            "--package",
            image_package,
        ])
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        .env("ANDROID_AVD_HOME", avd_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn avdmanager: {e}"))?;
    // "Do you wish to create a custom hardware profile?" — no.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"no\n");
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("waiting for avdmanager: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(format!(
            "avdmanager create avd failed with {}: {text}",
            output.status
        ));
    }
    let ini = avd_home.join(format!("{AVD_NAME}.ini"));
    if !ini.is_file() {
        return Err(format!(
            "avdmanager reported success but {} does not exist: {text}",
            ini.display()
        ));
    }
    Ok(text)
}

fn delete_avd(sdk_root: &Path, avdmanager: &Path, avd_home: &Path) {
    let _ = Command::new(avdmanager)
        .args(["delete", "avd", "--name", AVD_NAME])
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        .env("ANDROID_AVD_HOME", avd_home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn launch_emulator(
    sdk_root: &Path,
    emulator: &Path,
    avd_home: &Path,
    log: &Path,
) -> Result<Child, String> {
    let stdout = fs::File::create(log).map_err(|e| format!("creating {}: {e}", log.display()))?;
    let stderr = stdout
        .try_clone()
        .map_err(|e| format!("cloning the emulator log handle: {e}"))?;
    Command::new(emulator)
        .args(["-avd", AVD_NAME, "-port", &CONSOLE_PORT.to_string()])
        .args([
            "-no-window",
            "-no-audio",
            "-no-boot-anim",
            "-no-snapshot",
            "-no-metrics",
            "-gpu",
            "swiftshader_indirect",
            "-camera-back",
            "none",
            "-camera-front",
            "none",
            "-memory",
            "2048",
            "-cores",
            "2",
            "-accel",
            "on",
        ])
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        .env("ANDROID_AVD_HOME", avd_home)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| format!("failed to spawn the emulator: {e}"))
}

fn tail(path: &Path, lines: usize) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    let from = all.len().saturating_sub(lines);
    all[from..].join("\n")
}

/// Waits for the guest to boot; fails early if the emulator process exits.
fn wait_for_boot(adb: &Adb, emulator: &mut Child, log: &Path) -> Result<Duration, String> {
    let started = Instant::now();
    loop {
        if let Some(status) = emulator
            .try_wait()
            .map_err(|e| format!("waiting for the emulator: {e}"))?
        {
            return Err(format!(
                "the emulator exited with {status} before the guest booted; its log ends with:\n{}",
                tail(log, 40)
            ));
        }
        if started.elapsed() > BOOT_TIMEOUT {
            return Err(format!(
                "the guest did not boot within {}s; the emulator log ends with:\n{}",
                BOOT_TIMEOUT.as_secs(),
                tail(log, 40)
            ));
        }
        if adb.is_online() && boot_completed(&adb.shell_lenient("getprop sys.boot_completed")) {
            return Ok(started.elapsed());
        }
        thread::sleep(Duration::from_secs(5));
    }
}

/// Waits for the device to be back in the `device` state with its boot
/// property set (after `adb root` restarts adbd).
fn wait_until_online(adb: &Adb, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if adb.is_online() && boot_completed(&adb.shell_lenient("getprop sys.boot_completed")) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(2));
    }
    Err(format!(
        "the device did not come back within {}s after adb root",
        timeout.as_secs()
    ))
}

fn shutdown(adb: &Adb, emulator: &mut Child) {
    let _ = adb.run(&["emu", "kill"]);
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = emulator.try_wait() {
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
    let _ = emulator.kill();
    let _ = emulator.wait();
}

// ===== the checks =====

/// The report of one run: one line per verified fact.
pub type Report = Vec<String>;

fn exercise(adb: &Adb, config: &Config, report: &mut Report) -> Result<(), String> {
    let dashboard_path = config.root.join(dashboard::DASHBOARD_ASSET_PATH);
    let dashboard_bytes = fs::read(&dashboard_path)
        .map_err(|e| format!("reading {}: {e}", dashboard_path.display()))?;
    let apk = config.apk.to_str().ok_or("the APK path is not UTF-8")?;

    let abilist = adb.shell("getprop ro.product.cpu.abilist")?;
    let abilist = abilist.trim().to_string();
    if !abilist_runs_arm64(&abilist) {
        return Err(format!(
            "the image's ABIs are `{abilist}`, without arm64-v8a: the shipped library cannot load"
        ));
    }
    let release = adb.shell_lenient("getprop ro.build.version.release");
    let sdk = adb.shell_lenient("getprop ro.build.version.sdk");
    report.push(format!(
        "guest: Android {} (API {}), abilist {abilist}",
        release.trim(),
        sdk.trim()
    ));

    // Root first, so the forward below survives adbd's restart.
    println!("== adb root ==");
    let root = adb.run(&["root"])?;
    // adbd restarts: the device drops off for a moment before it is back.
    thread::sleep(Duration::from_secs(3));
    wait_until_online(adb, Duration::from_secs(60))?;
    report.push(format!("adb root: {}", root.trim()));
    let _ = adb.shell_lenient("input keyevent 82");
    let bluetooth = adb.shell_lenient("svc bluetooth enable; settings get global bluetooth_on");
    report.push(format!(
        "bluetooth_on after `svc bluetooth enable`: {}",
        bluetooth.trim()
    ));

    println!("== adb install -r -g {} ==", config.apk.display());
    let install = adb.run(&["install", "-r", "-g", apk])?;
    if !install_succeeded(&install) {
        return Err(format!(
            "adb install did not report Success: {}",
            install.trim()
        ));
    }
    let size = fs::metadata(config.apk).map(|m| m.len()).unwrap_or(0);
    report.push(format!(
        "install: Success ({} bytes, runtime permissions granted)",
        size
    ));

    println!("== am start -W {ACTIVITY} ==");
    let launch = adb.shell(&format!("am start -W -n {ACTIVITY}"))?;
    if !launch_succeeded(&launch) {
        return Err(format!("the activity did not start: {}", launch.trim()));
    }
    report.push("launch: MainActivity started (Status: ok)".to_string());

    let forward = adb.run(&["forward", "tcp:0", &format!("tcp:{GUEST_API_PORT}")])?;
    let port = forwarded_port(&forward)
        .ok_or_else(|| format!("adb forward printed no port: {}", forward.trim()))?;
    let api_after = wait_for_api(port, API_TIMEOUT)?;
    report.push(format!(
        "API: http://127.0.0.1:{GUEST_API_PORT}/ in the guest answered {:.1}s after the launch (adb forward tcp:{port})",
        api_after.as_secs_f64()
    ));

    println!("== the documents ==");
    let index = get(port, "/")?;
    if index.status != 200 || index.body != dashboard_bytes {
        return Err(format!(
            "GET / answered {} with {} bytes; expected the committed dashboard ({} bytes)",
            index.status,
            index.body.len(),
            dashboard_bytes.len()
        ));
    }
    report.push(format!(
        "GET /: the committed dashboard.html, byte-identical ({} bytes)",
        dashboard_bytes.len()
    ));
    let status = get(port, "/api/status")?;
    require_keys(
        "GET /api/status",
        &status.body,
        &["scanning", "device_count", "native_available", "uptime_ms"],
    )?;
    let status_text = body_text(&status);
    if !json_has(&status_text, "native_available", "true") {
        return Err(format!(
            "native_available is not true on the device (the library did not load): {status_text}"
        ));
    }
    if !json_has(&status_text, "scanning", "false") {
        return Err(format!(
            "the app is scanning before any request: {status_text}"
        ));
    }
    if json_integer(&status_text, "device_count") != Some(0) {
        return Err(format!(
            "devices are tracked before any scan: {status_text}"
        ));
    }
    report.push(format!("GET /api/status before any scan: {status_text}"));
    let devices = get(port, "/api/devices")?;
    require_keys(
        "GET /api/devices",
        &devices.body,
        &["devices", "scanning", "native_available", "timestamp_ms"],
    )?;
    let devices_text = body_text(&devices);
    if !devices_text.starts_with(r#"{"devices":[]"#) {
        return Err(format!("the device list is not empty: {devices_text}"));
    }
    report.push("GET /api/devices: the documented keys, no device".to_string());
    let updates = get(port, "/api/updates")?;
    require_keys(
        "GET /api/updates",
        &updates.body,
        &["last_check_ms", "next_check_ms", "retry_count"],
    )?;
    report.push(format!("GET /api/updates: {}", body_text(&updates)));

    println!("== scan control ==");
    let start = post(port, "/api/scan/start")?;
    let start_text = body_text(&start);
    let scanning = match start.status {
        200 if start_text == SCAN_STARTED_JSON => true,
        409 if ADAPTER_REFUSALS.contains(&start_text.as_str()) => false,
        other => {
            return Err(format!(
                "POST /api/scan/start answered {other} {start_text}; expected 200 {SCAN_STARTED_JSON} or 409 with an adapter refusal"
            ));
        }
    };
    report.push(format!(
        "POST /api/scan/start: {} {start_text}",
        start.status
    ));

    if scanning {
        let status = body_text(&get(port, "/api/status")?);
        if !json_has(&status, "scanning", "true") {
            return Err(format!("status does not report the scan: {status}"));
        }
        let services = adb.shell(&format!("dumpsys activity services {PACKAGE}"))?;
        match service_is_foreground(&services, SERVICE) {
            Some(true) => {}
            Some(false) => {
                return Err("RadarScanService is running but not in the foreground".into());
            }
            None => return Err("RadarScanService has no service record after the start".into()),
        }
        let titles = notification_titles(&adb.shell("dumpsys notification --noredact")?);
        if !titles.iter().any(|title| title == SCANNING_TITLE) {
            return Err(format!(
                "the scanning notification is missing; titles: {titles:?}"
            ));
        }
        report.push(format!(
            "scanning: status {status}; RadarScanService isForeground=true; notification \"{SCANNING_TITLE}\""
        ));

        let stop = post(port, "/api/scan/stop")?;
        let stop_text = body_text(&stop);
        if stop.status != 200 || stop_text != SCAN_STOPPED_JSON {
            return Err(format!(
                "POST /api/scan/stop answered {} {stop_text}",
                stop.status
            ));
        }
        let paused = body_text(&get(port, "/api/status")?);
        if !json_has(&paused, "scanning", "false") {
            return Err(format!("status does not report the pause: {paused}"));
        }
        let services = adb.shell(&format!("dumpsys activity services {PACKAGE}"))?;
        if service_is_foreground(&services, SERVICE) != Some(true) {
            return Err(
                "after the pause RadarScanService is not a running foreground service".into(),
            );
        }
        let titles = notification_titles(&adb.shell("dumpsys notification --noredact")?);
        if !titles.iter().any(|title| title == IDLE_TITLE) {
            return Err(format!(
                "the idle notification is missing; titles: {titles:?}"
            ));
        }
        report.push(format!(
            "paused: POST /api/scan/stop {stop_text}; the service stays in the foreground with \"{IDLE_TITLE}\""
        ));

        let resumed = post(port, "/api/scan/start")?;
        if resumed.status != 200 {
            return Err(format!(
                "the second start answered {} {}",
                resumed.status,
                body_text(&resumed)
            ));
        }

        println!("== kill -9 the app process: START_STICKY must resume the scan ==");
        let pid: u32 = adb
            .shell(&format!("pidof {PACKAGE}"))?
            .trim()
            .parse()
            .map_err(|e| format!("pidof {PACKAGE} gave no pid: {e}"))?;
        adb.shell(&format!("kill -9 {pid}"))?;
        let killed = Instant::now();
        let mut restarted: Option<(u32, Duration)> = None;
        while killed.elapsed() < RESTART_TIMEOUT {
            let current = adb.shell_lenient(&format!("pidof {PACKAGE}"));
            if let Ok(new_pid) = current.trim().parse::<u32>()
                && new_pid != pid
                && let Ok(response) = get(port, "/api/status")
                && response.status == 200
                && json_has(&body_text(&response), "scanning", "true")
            {
                restarted = Some((new_pid, killed.elapsed()));
                break;
            }
            thread::sleep(Duration::from_secs(2));
        }
        let (new_pid, after) = restarted.ok_or_else(|| {
            format!(
                "after kill -9 of pid {pid} the scan did not resume within {}s (sticky restart)",
                RESTART_TIMEOUT.as_secs()
            )
        })?;
        report.push(format!(
            "kill -9 {pid}: the system restarted the service (pid {new_pid}) and the scan resumed {:.1}s later",
            after.as_secs_f64()
        ));
    } else {
        // Without an adapter the service is only bound by the activity; a
        // stop is a no-op answered with the live state.
        let stop = post(port, "/api/scan/stop")?;
        if stop.status != 200 || body_text(&stop) != SCAN_STOPPED_JSON {
            return Err(format!(
                "POST /api/scan/stop answered {} {}",
                stop.status,
                body_text(&stop)
            ));
        }
        report.push(
            "no adapter on this image: the start was refused as documented, a stop answers the idle state; the foreground and restart paths were not exercised"
                .to_string(),
        );
    }

    println!("== crash log ==");
    let crashes = adb.shell_lenient("logcat -d -b crash -v brief");
    let headers = crash_headers(&crashes, PACKAGE);
    if !headers.is_empty() {
        return Err(format!(
            "the app crashed on the device:\n{}\n{}",
            headers.join("\n"),
            crashes
        ));
    }
    let update_lines = adb
        .shell_lenient("logcat -d -s UpdateCheckService:* UpdateManager:*")
        .lines()
        .filter(|line| line.contains("UpdateCheckService") || line.contains("UpdateManager"))
        .count();
    report.push(format!(
        "logcat: no FATAL EXCEPTION for {PACKAGE}; {update_lines} update-check line(s)"
    ));

    println!("== am force-stop: the API must end with the app ==");
    adb.shell(&format!("am force-stop {PACKAGE}"))?;
    let stopped = Instant::now();
    let mut unreachable = 0;
    while stopped.elapsed() < Duration::from_secs(30) && unreachable < 3 {
        match get(port, "/api/status") {
            Ok(response) if response.status == 200 => unreachable = 0,
            _ => unreachable += 1,
        }
        thread::sleep(Duration::from_secs(1));
    }
    if unreachable < 3 {
        return Err("the API still answers after am force-stop".to_string());
    }
    report
        .push("am force-stop: the API is unreachable, the service ended with the app".to_string());
    Ok(())
}

/// The whole command.
pub fn run(config: &Config) -> Result<(), String> {
    let adb_exe = tool(config.sdk_root, "platform-tools/adb")?;
    let emulator_exe = tool(config.sdk_root, "emulator/emulator")?;
    let avdmanager = tool(config.sdk_root, "cmdline-tools/latest/bin/avdmanager")?;
    if !config.apk.is_file() {
        return Err(format!("APK not found: {}", config.apk.display()));
    }
    let work = crate::xtask_temp_dir("verify-android-emulator");
    let avd_home = work.join("avd");
    crate::recreate_dirs(&[&work, &avd_home])?;
    let log = work.join("emulator.log");

    println!(
        "== avdmanager create avd {AVD_NAME} ({}) ==",
        config.image_package
    );
    let created = create_avd(
        config.sdk_root,
        &avdmanager,
        &avd_home,
        config.image_package,
    )?;
    println!("{}", created.trim());

    println!("== booting the emulator (headless, -accel on) ==");
    let mut emulator = launch_emulator(config.sdk_root, &emulator_exe, &avd_home, &log)?;
    let adb = Adb {
        exe: adb_exe,
        serial: format!("emulator-{CONSOLE_PORT}"),
    };
    let mut report = Report::new();
    let outcome = wait_for_boot(&adb, &mut emulator, &log).and_then(|boot| {
        report.push(format!(
            "boot: {:.0}s ({})",
            boot.as_secs_f64(),
            config.image_package
        ));
        println!("guest booted after {:.0}s", boot.as_secs_f64());
        exercise(&adb, config, &mut report)
    });
    if outcome.is_err() {
        println!("== diagnostics ==");
        println!("-- logcat (warnings and above, last 120 lines) --");
        let logcat = adb.shell_lenient("logcat -d -v time *:W");
        let lines: Vec<&str> = logcat.lines().collect();
        println!("{}", lines[lines.len().saturating_sub(120)..].join("\n"));
        println!("-- emulator log (last 40 lines) --");
        println!("{}", tail(&log, 40));
    }
    println!("== shutting the emulator down ==");
    shutdown(&adb, &mut emulator);
    delete_avd(config.sdk_root, &avdmanager, &avd_home);
    outcome?;
    println!("== verify-android-emulator: report ==");
    for line in &report {
        println!("  {line}");
    }
    println!("verify-android-emulator: the committed APK ran on a real Android runtime, all green");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_outputs_are_parsed() {
        assert!(boot_completed("1\r\n"));
        assert!(!boot_completed("\n"));
        assert_eq!(forwarded_port("34567\n"), Some(34567));
        assert_eq!(
            forwarded_port("* daemon started successfully\n41000\n"),
            Some(41000)
        );
        assert_eq!(forwarded_port("error: no devices\n"), None);
        assert!(install_succeeded("Performing Streamed Install\nSuccess\n"));
        assert!(!install_succeeded(
            "adb: failed to install x.apk: Failure [INSTALL_FAILED_NO_MATCHING_ABIS]\n"
        ));
        assert!(launch_succeeded(
            "Starting: Intent { cmp=com.hse.bleradar/.MainActivity }\nStatus: ok\nLaunchState: COLD\n"
        ));
        assert!(!launch_succeeded("Error: Activity class does not exist.\n"));
        assert!(abilist_runs_arm64("x86_64,arm64-v8a"));
        assert!(!abilist_runs_arm64("x86_64,x86"));
    }

    const DUMPSYS_SERVICES: &str = "ACTIVITY MANAGER SERVICES (dumpsys activity services)\n  User 0 active services:\n  * ServiceRecord{9f2f9f2 u0 com.hse.bleradar/.UpdateCheckService}\n    intent={cmp=com.hse.bleradar/.UpdateCheckService}\n    isForeground=true foregroundId=2\n  * ServiceRecord{c0ffee u0 com.hse.bleradar/.RadarScanService}\n    intent={cmp=com.hse.bleradar/.RadarScanService}\n    packageName=com.hse.bleradar\n    isForeground=false foregroundId=0 foregroundNoti=null\n    startRequested=false delayedStop=false stopIfKilled=false\n";

    #[test]
    fn service_records_are_found_and_read() {
        let record = service_record(DUMPSYS_SERVICES, SERVICE).unwrap();
        assert!(record.starts_with("ServiceRecord{c0ffee"), "{record}");
        assert!(record.contains("startRequested=false"));
        assert!(!record.contains("UpdateCheckService"));
        assert_eq!(
            service_is_foreground(DUMPSYS_SERVICES, SERVICE),
            Some(false)
        );
        assert_eq!(
            service_is_foreground(DUMPSYS_SERVICES, "com.hse.bleradar/.UpdateCheckService"),
            Some(true)
        );
        assert_eq!(
            service_is_foreground(DUMPSYS_SERVICES, "com.other/.Svc"),
            None
        );
        let promoted = DUMPSYS_SERVICES.replace(
            "isForeground=false foregroundId=0",
            "isForeground=true foregroundId=1",
        );
        assert_eq!(service_is_foreground(&promoted, SERVICE), Some(true));
    }

    #[test]
    fn notification_titles_and_crashes_are_extracted() {
        let dumpsys = "  NotificationRecord(0x1: pkg=com.hse.bleradar user=UserHandle{0} id=1)\n      extras={\n        android.title=String (BLE Radar is scanning)\n        android.text=String (…)\n      }\n";
        assert_eq!(
            notification_titles(dumpsys),
            vec![SCANNING_TITLE.to_string()]
        );
        let crash = "--------- beginning of crash\nE/AndroidRuntime( 1234): FATAL EXCEPTION: main\nE/AndroidRuntime( 1234): Process: com.hse.bleradar, PID: 1234\nE/AndroidRuntime( 1234): java.lang.IllegalStateException: boom\n";
        assert_eq!(crash_headers(crash, PACKAGE).len(), 1);
        assert!(crash_headers(crash, "com.other").is_empty());
        assert!(crash_headers("", PACKAGE).is_empty());
    }

    #[test]
    fn json_values_are_read_without_a_parser() {
        let status =
            r#"{"scanning":true,"device_count":0,"native_available":true,"uptime_ms":1234}"#;
        assert_eq!(json_integer(status, "uptime_ms"), Some(1234));
        assert_eq!(json_integer(status, "device_count"), Some(0));
        assert_eq!(json_integer(status, "missing"), None);
        assert!(json_has(status, "scanning", "true"));
        assert!(!json_has(status, "scanning", "false"));
        assert!(
            require_keys(
                "x",
                status.as_bytes(),
                &["scanning", "device_count", "native_available", "uptime_ms"]
            )
            .is_ok()
        );
        assert!(
            require_keys("x", status.as_bytes(), &["scanning"])
                .unwrap_err()
                .contains("expected")
        );
    }
}
