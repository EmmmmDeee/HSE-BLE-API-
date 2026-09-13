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
//! * with the image's Bluetooth adapter enabled, `POST /api/scan/start` is
//!   accepted, the service is in the foreground with the scanning
//!   notification, a stop pauses it with the idle notification and keeps
//!   it, and a `kill -9` of the app process is followed by the
//!   `START_STICKY` restart that resumes the scan;
//! * the update check the first launch starts ran to its decision and its
//!   `dataSync` service finished (no record, no notification left);
//! * `am force-stop` ends the app and, with it, the API;
//! * after a relaunch and a new scan, revoking `BLUETOOTH_SCAN` makes the
//!   platform kill the app: no scan and no foreground service survive it,
//!   and the report records whether the sticky restart ran the "revoked,
//!   stopping" branch;
//! * the crash and main logs carry no Java or native crash of the app.
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
const UPDATE_SERVICE: &str = "com.hse.bleradar/.UpdateCheckService";
/// The runtime permission whose revocation the proof exercises (API 31+).
const REVOKED_PERMISSION: &str = "android.permission.BLUETOOTH_SCAN";
/// What `RadarScanService.onStartCommand` logs when a sticky restart finds
/// the permissions revoked and stops the service instead of promoting it.
const REVOKED_RESTART_LOG: &str = "Bluetooth permissions revoked on sticky restart";
/// `UpdateCheckService.buildUpdateNotification`'s title, which must be gone
/// once the check finished.
const UPDATE_NOTIFICATION_TITLE: &str = "Checking for updates...";
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
/// The notification texts (`setContentText`) `res/values/strings.xml`
/// gives the two states; the title is the app name.
const SCANNING_TEXT: &str = "BLE Radar is scanning";
const IDLE_TEXT: &str = "BLE Radar is idle";
/// How long the adapter may take to report enabled after `svc bluetooth enable`.
const BLUETOOTH_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Every `android.title` and `android.text` in `dumpsys notification
/// --noredact` output, in order (the service's state string is the text).
pub fn notification_strings(dumpsys: &str) -> Vec<String> {
    dumpsys
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("android.title=String (")
                .or_else(|| line.strip_prefix("android.text=String ("))?;
            Some(rest.strip_suffix(')').unwrap_or(rest).to_string())
        })
        .collect()
}

/// The crash headers in a log that belong to `package`: a Java `FATAL
/// EXCEPTION` (its `Process:` line follows within a few lines), a native
/// `Fatal signal` naming the package's process, or a tombstone header
/// (`>>> package <<<`).
pub fn crash_headers<'a>(logcat: &'a str, package: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = logcat.lines().collect();
    let process = format!("Process: {package}");
    let native_owner = format!("({package}");
    let tombstone = format!(">>> {package} <<<");
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            (line.contains("FATAL EXCEPTION")
                && lines[*index..(*index + 4).min(lines.len())]
                    .iter()
                    .any(|following| following.contains(&process)))
                || (line.contains("Fatal signal") && line.contains(&native_owner))
                || line.contains(&tombstone)
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

/// The `Update decision: N` line `UpdateCheckService` logs once it assessed
/// the bundled manifest: the ordinal, if the check ran that far.
pub fn update_decision_logged(logcat: &str) -> Option<i64> {
    logcat
        .lines()
        .filter_map(|line| line.split("Update decision: ").nth(1))
        .filter_map(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .find_map(|digits| digits.parse().ok())
}

/// The `ps` lines (after the header) about emulator-side processes — the
/// launcher or QEMU, netsim, adb — excluding this command's own process
/// tree, whose arguments name the subcommand.
pub fn emulator_process_lines(ps: &str) -> Vec<String> {
    ps.lines()
        .skip(1)
        .filter(|line| !line.contains("verify-android-emulator"))
        .filter(|line| {
            ["emulator", "qemu-system", "netsim", "adb"]
                .iter()
                .any(|needle| line.contains(needle))
        })
        .map(|line| line.trim().to_string())
        .collect()
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

/// How often a request is retried when the guest's server is between
/// instances (the activity relaunching unbinds and rebinds the service, and
/// the new instance rebinds the port a moment later).
const REQUEST_ATTEMPTS: u32 = 5;

fn with_retry(
    label: &str,
    mut attempt: impl FnMut() -> Result<HttpResponse, String>,
) -> Result<HttpResponse, String> {
    let mut last = String::new();
    for _ in 0..REQUEST_ATTEMPTS {
        match attempt() {
            Ok(response) => return Ok(response),
            Err(error) => last = error,
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err(format!(
        "{label}: no answer in {REQUEST_ATTEMPTS} attempts (last: {last})"
    ))
}

fn get(port: u16, target: &str) -> Result<HttpResponse, String> {
    with_retry(&format!("GET {target}"), || {
        apilive::http_request(
            port,
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
    })
}

fn post(port: u16, target: &str) -> Result<HttpResponse, String> {
    with_retry(&format!("POST {target}"), || {
        apilive::http_request(
            port,
            format!(
                "POST {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
    })
}

/// Polls `GET /api/status` until it carries `"key":<literal>`.
fn wait_for_status(port: u16, key: &str, literal: &str) -> Result<String, String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < PROMOTION_TIMEOUT {
        if let Ok(response) = get(port, "/api/status") {
            last = body_text(&response);
            if json_has(&last, key, literal) {
                return Ok(last);
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "within {}s /api/status never reported \"{key}\":{literal} (last: {last})",
        PROMOTION_TIMEOUT.as_secs()
    ))
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

/// How long the service may take to be promoted (or re-promoted) with the
/// expected notification after a control request answered.
const PROMOTION_TIMEOUT: Duration = Duration::from_secs(20);

/// Waits until `RadarScanService` is a running foreground service whose
/// notification carries `title`; the time it took.
fn wait_for_service_state(adb: &Adb, title: &str) -> Result<Duration, String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < PROMOTION_TIMEOUT {
        let services = adb.shell_lenient(&format!("dumpsys activity services {PACKAGE}"));
        let foreground = service_is_foreground(&services, SERVICE);
        let titles = notification_strings(&adb.shell_lenient("dumpsys notification --noredact"));
        if foreground == Some(true) && titles.iter().any(|candidate| candidate == title) {
            return Ok(started.elapsed());
        }
        last = format!("isForeground={foreground:?}, notification titles {titles:?}");
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "within {}s RadarScanService was not a foreground service with the \"{title}\" notification (last: {last})",
        PROMOTION_TIMEOUT.as_secs()
    ))
}

fn body_text(response: &HttpResponse) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

/// `POST /api/scan/start`, which must be accepted: a service instance the
/// activity's relaunch is destroying refuses a start (its engine is closed)
/// and the instance the relaunch creates takes the next one, so an
/// "unavailable" refusal is retried briefly. The `200` body.
fn start_scan(port: u16) -> Result<String, String> {
    let mut start = post(port, "/api/scan/start")?;
    let mut start_text = body_text(&start);
    for _ in 0..REQUEST_ATTEMPTS {
        if !(start.status == 409 && start_text.contains("scanner unavailable")) {
            break;
        }
        thread::sleep(Duration::from_secs(1));
        start = post(port, "/api/scan/start")?;
        start_text = body_text(&start);
    }
    if start.status != 200 || start_text != SCAN_STARTED_JSON {
        return Err(format!(
            "POST /api/scan/start answered {} {start_text}; expected 200 {SCAN_STARTED_JSON} (the adapter was enabled above, so a refusal is a regression)",
            start.status
        ));
    }
    Ok(start_text)
}

/// Whether the API stopped answering: three consecutive failed status
/// requests (each retries on its own) before `timeout` runs out.
fn api_unreachable(port: u16, timeout: Duration) -> bool {
    let started = Instant::now();
    let mut failures = 0;
    while started.elapsed() < timeout && failures < 3 {
        match get(port, "/api/status") {
            Ok(response) if response.status == 200 => failures = 0,
            _ => failures += 1,
        }
        thread::sleep(Duration::from_secs(1));
    }
    failures >= 3
}

/// The app's process id, which must exist.
fn app_pid(adb: &Adb) -> Result<u32, String> {
    adb.shell(&format!("pidof {PACKAGE}"))?
        .trim()
        .parse()
        .map_err(|e| format!("pidof {PACKAGE} gave no pid: {e}"))
}

/// Launches the activity (`am start -W`), which must report `Status: ok`.
fn launch_activity(adb: &Adb) -> Result<(), String> {
    let launch = adb.shell(&format!("am start -W -n {ACTIVITY}"))?;
    if !launch_succeeded(&launch) {
        return Err(format!("the activity did not start: {}", launch.trim()));
    }
    Ok(())
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

/// Deletes the AVD; a failure is reported, not swallowed, so a runner never
/// silently accumulates multi-gigabyte AVDs.
fn delete_avd(sdk_root: &Path, avdmanager: &Path, avd_home: &Path) -> Result<(), String> {
    let output = Command::new(avdmanager)
        .args(["delete", "avd", "--name", AVD_NAME])
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        .env("ANDROID_AVD_HOME", avd_home)
        .output()
        .map_err(|e| format!("failed to spawn avdmanager delete: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "avdmanager delete avd failed with {}: {}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
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

/// Waits for `bluetooth_on` to read `1`; the time it took.
fn wait_for_bluetooth(adb: &Adb) -> Result<Duration, String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < BLUETOOTH_TIMEOUT {
        last = adb
            .shell_lenient("settings get global bluetooth_on")
            .trim()
            .to_string();
        if last == "1" {
            return Ok(started.elapsed());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err(format!(
        "bluetooth_on stayed `{last}` for {}s after `svc bluetooth enable`: the pinned image no longer exposes an adapter, which this proof requires (a refused start would skip the foreground and restart checks)",
        BLUETOOTH_TIMEOUT.as_secs()
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
    // The adapter transition is asynchronous: wait for the enabled state so a
    // start is never refused for an adapter that was still coming up.
    let _ = adb.shell_lenient("svc bluetooth enable");
    let bluetooth_after = wait_for_bluetooth(adb)?;
    report.push(format!(
        "bluetooth_on=1 {:.1}s after `svc bluetooth enable`",
        bluetooth_after.as_secs_f64()
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
    launch_activity(adb)?;
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
    let start_text = start_scan(port)?;
    report.push(format!("POST /api/scan/start: 200 {start_text}"));

    {
        // The instance that answered may be the one an activity relaunch is
        // replacing; the scan is awaited on whichever instance serves next.
        let status = wait_for_status(port, "scanning", "true")?;
        // The start answers from the handler thread while the queued
        // startForegroundService command is still on its way to the main
        // thread, so the promotion is awaited rather than sampled once.
        let promoted = wait_for_service_state(adb, SCANNING_TEXT)?;
        report.push(format!(
            "scanning: status {status}; RadarScanService isForeground=true with \"{SCANNING_TEXT}\" {:.1}s after the start",
            promoted.as_secs_f64()
        ));

        let stop = post(port, "/api/scan/stop")?;
        let stop_text = body_text(&stop);
        if stop.status != 200 || stop_text != SCAN_STOPPED_JSON {
            return Err(format!(
                "POST /api/scan/stop answered {} {stop_text}",
                stop.status
            ));
        }
        wait_for_status(port, "scanning", "false")?;
        wait_for_service_state(adb, IDLE_TEXT)?;
        report.push(format!(
            "paused: POST /api/scan/stop {stop_text}; the service stays in the foreground with \"{IDLE_TEXT}\""
        ));

        let resumed = post(port, "/api/scan/start")?;
        if resumed.status != 200 {
            return Err(format!(
                "the second start answered {} {}",
                resumed.status,
                body_text(&resumed)
            ));
        }
        // Wait for that start's own promotion so the kill below tests the
        // sticky restart, not a race with a start command still queued.
        wait_for_service_state(adb, SCANNING_TEXT)?;

        println!("== kill -9 the app process: START_STICKY must resume the scan ==");
        let pid = app_pid(adb)?;
        report.push(format!("app process before the kill: pid {pid}"));
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
    }

    println!("== the update check the first launch started ==");
    // MainActivity.onStart starts UpdateCheckService (never checked before on
    // this fresh install) through startForegroundService, asynchronously: it
    // must have been promoted without the platform's foreground timeout
    // killing the app (the crash log below would show that), assessed the
    // bundled manifest, and finished — no service record and no "Checking
    // for updates..." notification left. Both are awaited, and every dump is
    // a fallible command, so a failed adb call never reads as "nothing left".
    let decision_awaited = Instant::now();
    let (decision, update_lines) = loop {
        let update_log = adb.shell("logcat -d -s UpdateCheckService:* UpdateManager:*")?;
        if let Some(decision) = update_decision_logged(&update_log) {
            let lines = update_log
                .lines()
                .filter(|line| {
                    line.contains("UpdateCheckService") || line.contains("UpdateManager")
                })
                .count();
            break (decision, lines);
        }
        if decision_awaited.elapsed() > PROMOTION_TIMEOUT {
            return Err(format!(
                "the update check did not reach a decision within {}s; its log:\n{update_log}",
                PROMOTION_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    };
    let finish_awaited = Instant::now();
    loop {
        let services = adb.shell(&format!("dumpsys activity services {PACKAGE}"))?;
        let notifications = notification_strings(&adb.shell("dumpsys notification --noredact")?);
        let record = service_record(&services, UPDATE_SERVICE);
        let notified = notifications
            .iter()
            .any(|title| title == UPDATE_NOTIFICATION_TITLE);
        if record.is_none() && !notified {
            break;
        }
        if finish_awaited.elapsed() > PROMOTION_TIMEOUT {
            return Err(format!(
                "UpdateCheckService did not finish within {}s of its decision: service record {}, \"{UPDATE_NOTIFICATION_TITLE}\" notification {}\n{}",
                PROMOTION_TIMEOUT.as_secs(),
                if record.is_some() {
                    "present"
                } else {
                    "absent"
                },
                if notified { "present" } else { "absent" },
                record.unwrap_or_default()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
    report.push(format!(
        "update check: ran on the first launch to decision {decision} ({update_lines} log lines) and the dataSync service finished — no record, no notification left"
    ));

    println!("== am force-stop: the API must end with the app ==");
    adb.shell(&format!("am force-stop {PACKAGE}"))?;
    if !api_unreachable(port, Duration::from_secs(30)) {
        return Err("the API still answers after am force-stop".to_string());
    }
    report
        .push("am force-stop: the API is unreachable, the service ended with the app".to_string());

    println!("== pm revoke {REVOKED_PERMISSION} while scanning: nothing may survive it ==");
    // Relaunch, scan again, then revoke a permission the scan needs: the
    // platform kills the app for it. Whatever it does next — restart the
    // sticky service (whose onStartCommand must then stop it rather than
    // promote it without the permission, which API 34 rejects) or leave it
    // down — no scan and no foreground service may remain, and the API is
    // either gone or answering an idle state.
    launch_activity(adb)?;
    wait_for_api(port, API_TIMEOUT)?;
    start_scan(port)?;
    wait_for_status(port, "scanning", "true")?;
    wait_for_service_state(adb, SCANNING_TEXT)?;
    let pid = app_pid(adb)?;
    adb.shell(&format!("pm revoke {PACKAGE} {REVOKED_PERMISSION}"))?;
    let revoked = Instant::now();
    let mut killed = false;
    let mut settled = None;
    let mut last = String::new();
    while revoked.elapsed() < RESTART_TIMEOUT {
        // Every probe is a fallible command: a transport failure is an error,
        // never "killed" or "no service". `pidof` itself exits 1 with no
        // output when nothing matches, hence the `||`.
        let current = adb.shell(&format!("pidof {PACKAGE} || echo none"))?;
        killed = killed
            || !current
                .split_whitespace()
                .any(|candidate| candidate == pid.to_string());
        let services = adb.shell(&format!("dumpsys activity services {PACKAGE}"))?;
        let foreground = service_is_foreground(&services, SERVICE);
        // The API must be gone (no answer after the request's own retries) or
        // answering an idle state; any other answer keeps the loop going.
        let api = match get(port, "/api/status") {
            Ok(response) if response.status == 200 => {
                json_has(&body_text(&response), "scanning", "false").then_some("answering idle")
            }
            Ok(response) => {
                last = format!("status {}", response.status);
                None
            }
            Err(_) => Some("unreachable"),
        };
        if killed && foreground != Some(true) && api.is_some() {
            settled = api;
            break;
        }
        last = format!("killed={killed}, isForeground={foreground:?}, api={api:?} ({last})");
        thread::sleep(Duration::from_secs(2));
    }
    let settled = settled.ok_or_else(|| {
        format!(
            "within {}s of the revocation the app did not settle (last: {last})",
            RESTART_TIMEOUT.as_secs()
        )
    })?;
    // A sticky restart may still be pending (its delay grows with each
    // restart): watch the service's own log for the branch through the rest
    // of the window, and classify only once the line appears or the window
    // has passed. A foreground service at any point is a regression.
    let mut services;
    let restarted_and_stopped = loop {
        let service_log = adb.shell("logcat -d -s RadarScanService:*")?;
        services = adb.shell(&format!("dumpsys activity services {PACKAGE}"))?;
        if service_is_foreground(&services, SERVICE) == Some(true) {
            return Err(format!(
                "RadarScanService is back in the foreground without {REVOKED_PERMISSION}:\n{services}"
            ));
        }
        if service_log.contains(REVOKED_RESTART_LOG) {
            break true;
        }
        if revoked.elapsed() >= RESTART_TIMEOUT {
            break false;
        }
        thread::sleep(Duration::from_secs(2));
    };
    let restart = if restarted_and_stopped {
        "the sticky restart found the permissions revoked and stopped the service"
    } else if service_record(&services, SERVICE).is_some() {
        "the service record is back (an activity relaunch re-bound it) without a scan"
    } else {
        "the platform did not restart the service within the window"
    };
    report.push(format!(
        "pm revoke {REVOKED_PERMISSION}: the platform killed pid {pid}; {restart}; the API is {settled} {:.1}s after the revocation",
        revoked.elapsed().as_secs_f64()
    ));

    println!("== crash log ==");
    let crashes = adb.shell_lenient("logcat -d -b crash -b main -v brief");
    let headers = crash_headers(&crashes, PACKAGE);
    if !headers.is_empty() {
        return Err(format!(
            "the app crashed on the device:\n{}\n{}",
            headers.join("\n"),
            crashes
        ));
    }
    report.push(format!("logcat: no Java or native crash of {PACKAGE}"));
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
    let adb = Adb {
        exe: adb_exe,
        serial: format!("emulator-{CONSOLE_PORT}"),
    };
    let mut report = Report::new();
    // The tool identity and whether the SDK ships netsim (virtual radios):
    // what the next proof step could build on.
    let version = Command::new(&emulator_exe)
        .arg("-version")
        .env("ANDROID_SDK_ROOT", config.sdk_root)
        .env("ANDROID_HOME", config.sdk_root)
        .output()
        .map(|output| {
            // The launcher prints its banner on stderr on some versions.
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .unwrap_or_default();
    report.push(format!(
        "emulator: {}; netsimd {}",
        version
            .lines()
            .find(|line| line.to_ascii_lowercase().contains("version"))
            .unwrap_or("(version unknown)")
            .trim(),
        if config.sdk_root.join("emulator/netsimd").is_file() {
            "shipped"
        } else {
            "absent"
        }
    ));
    let outcome = match launch_emulator(config.sdk_root, &emulator_exe, &avd_home, &log) {
        Err(error) => Err(error),
        Ok(mut emulator) => {
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
                println!("-- report so far --");
                for line in &report {
                    println!("  {line}");
                }
                println!("-- logcat lines about the app (last 120) --");
                let logcat = adb.shell_lenient("logcat -d -v time");
                // Lines naming the app, its classes, or any pid the report
                // recorded for it (`( 1234)` as logcat prints the field).
                let pids: Vec<String> = report
                    .iter()
                    .filter_map(|line| line.rsplit("pid ").next())
                    .filter_map(|tail| tail.split(|c: char| !c.is_ascii_digit()).next())
                    .filter(|digits| !digits.is_empty())
                    .map(|digits| format!("({digits:>5})"))
                    .collect();
                let lines: Vec<&str> = logcat
                    .lines()
                    .filter(|line| {
                        [
                            "bleradar",
                            "RadarScanService",
                            "ApiHttpServer",
                            "BleScanEngine",
                            "UpdateCheckService",
                            "AndroidRuntime",
                        ]
                        .iter()
                        .any(|needle| line.contains(needle))
                            || pids.iter().any(|pid| line.contains(pid.as_str()))
                    })
                    .collect();
                println!("{}", lines[lines.len().saturating_sub(120)..].join("\n"));
                println!("-- emulator log (last 40 lines) --");
                println!("{}", tail(&log, 40));
            }
            println!("== shutting the emulator down ==");
            shutdown(&adb, &mut emulator);
            // What the shutdown left behind (CI's runner once reported an
            // orphan `emulator` process after a green run).
            let ps = Command::new("ps")
                .args(["-eo", "pid,ppid,comm,args"])
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                .unwrap_or_default();
            let lingering = emulator_process_lines(&ps);
            report.push(format!(
                "after the shutdown: {} emulator-side process(es) still alive{}{}",
                lingering.len(),
                if lingering.is_empty() { "" } else { ": " },
                lingering.join(" | ")
            ));
            outcome
        }
    };
    // The AVD is deleted on every exit after its creation; the proof's own
    // outcome is reported first, a cleanup failure after it.
    let cleanup = delete_avd(config.sdk_root, &avdmanager, &avd_home);
    outcome?;
    cleanup?;
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
    fn notification_strings_and_crashes_are_extracted() {
        // The service titles its notification with the app name and puts the
        // state in the text, so both fields are read.
        let dumpsys = "  NotificationRecord(0x1: pkg=com.hse.bleradar user=UserHandle{0} id=1)\n      extras={\n        android.title=String (HSE BLE Radar)\n        android.text=String (BLE Radar is scanning)\n      }\n";
        assert_eq!(
            notification_strings(dumpsys),
            vec!["HSE BLE Radar".to_string(), SCANNING_TEXT.to_string()]
        );
        let crash = "--------- beginning of crash\nE/AndroidRuntime( 1234): FATAL EXCEPTION: main\nE/AndroidRuntime( 1234): Process: com.hse.bleradar, PID: 1234\nE/AndroidRuntime( 1234): java.lang.IllegalStateException: boom\nF/libc    ( 4321): Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0 in tid 4321 (com.hse.bleradar), pid 4321 (com.hse.bleradar)\nF/DEBUG   ( 4400): pid: 4321, tid: 4321, name: com.hse.bleradar  >>> com.hse.bleradar <<<\nF/libc    ( 5555): Fatal signal 6 (SIGABRT), code -1 (SI_QUEUE) in tid 5555 (com.other), pid 5555 (com.other)\n";
        let headers = crash_headers(crash, PACKAGE);
        assert_eq!(headers.len(), 3, "{headers:?}");
        assert_eq!(crash_headers(crash, "com.other").len(), 1);
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

    #[test]
    fn the_update_decision_and_lingering_processes_are_read_from_tool_output() {
        let log = "--------- beginning of main\n\
                   09-13 01:18:45.100  3512  3512 D UpdateCheckService: UpdateCheckService created\n\
                   09-13 01:18:45.200  3512  3512 D UpdateCheckService: Update decision: 0 (available=1)\n\
                   09-13 01:18:45.201  3512  3512 D UpdateCheckService: No safe update available (decision=0)\n";
        assert_eq!(update_decision_logged(log), Some(0));
        assert_eq!(
            update_decision_logged("D UpdateCheckService: Update decision: 3 (available=1)"),
            Some(3)
        );
        assert_eq!(
            update_decision_logged("D UpdateCheckService: created"),
            None
        );
        assert_eq!(update_decision_logged(""), None);

        let ps = "    PID    PPID COMMAND         COMMAND\n\
                  2646       1 adb             adb -L tcp:5037 fork-server server --reply-fd 4\n\
                  3139    3100 emulator        /opt/sdk/emulator/emulator -avd bleradar-proof -port 5554\n\
                  3140    3139 netsimd         netsimd -s 8877\n\
                  3100    2900 xtask           target/debug/xtask verify-android-emulator\n\
                  2900    2800 cargo           cargo xtask verify-android-emulator\n\
                  4001       1 bash            bash -c ls\n";
        let lines = emulator_process_lines(ps);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[0].starts_with("2646"));
        assert!(lines[1].contains("emulator -avd"));
        assert!(lines[2].contains("netsimd"));
        assert!(emulator_process_lines("PID COMMAND\n").is_empty());
    }
}
