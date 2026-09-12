//! `cargo xtask verify-dashboard-live`: renders the committed web dashboard
//! (`android/app/src/main/assets/dashboard.html`, what `ApiHttpServer` serves
//! at `/`) in a real browser engine against a mock of the server's documented
//! JSON contract, and checks the DOM the page produced.
//!
//! Headless Chromium with `--virtual-time-budget` runs the page's timers and
//! fetches to completion and `--dump-dom` prints the resulting document, so no
//! browser-automation library is needed: the browser is an external tool
//! invoked like the SDK, the JDK and qemu are, and the tooling stays
//! dependency-free. Three scenarios run — a healthy API (every fixture device
//! must be rendered in server order, escaped, and the page must have polled
//! more than once), an API answering `500`, and an API answering `200` with
//! the wrong shape — and the last two must surface the error banner rather
//! than a blank or silently stale page.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The page under test, relative to the repository root; `build-apk` packages
/// the whole `assets/` directory, so this is what the APK serves.
pub const DASHBOARD_ASSET_PATH: &str = "android/app/src/main/assets/dashboard.html";

/// Where the rendered DOM of every scenario and the screenshot are written.
pub const OUTPUT_DIR: &str = "target/dashboard-live";

/// Virtual time the page runs for before the DOM is dumped: the initial poll
/// plus three 1 s polls (the page's `POLL_MS`).
const VIRTUAL_TIME_BUDGET_MS: u32 = 4_000;
/// Wall-clock cap on one browser invocation.
const BROWSER_TIMEOUT: Duration = Duration::from_secs(90);
/// The mock closes a connection that sends no request within this time
/// (Chromium preconnects speculatively).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Executables tried on `PATH`, in order, when `BLERADAR_CHROMIUM` is unset.
const PATH_CANDIDATES: &[&str] = &[
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "chrome",
];

/// `/api/devices` as `ApiHttpServer.devicesJson` writes it (`JsonWriter`
/// renders every double with a fraction, `null` for NaN, `null` for a
/// nameless device), covering every proximity, trend and freshness label,
/// a nameless device without distances (the no-native case), and a name that
/// must be rendered as text, never as markup.
pub const DEVICES_JSON: &str = concat!(
    r#"{"devices":["#,
    r#"{"address":"AA:BB:CC:DD:EE:01","name":"Tag Alpha","distance_m":1.234,"distance_lower_m":0.8,"distance_upper_m":1.9,"rssi_dbm":-61.4,"proximity":"NEAR","trend":"STRONGER","freshness":"LIVE","confidence_percent":87,"last_seen_ago_ms":420},"#,
    r#"{"address":"AA:BB:CC:DD:EE:02","name":"<b>evil</b> Ünïcødé 😀","distance_m":7.5,"distance_lower_m":5.2,"distance_upper_m":11.0,"rssi_dbm":-78.0,"proximity":"MID","trend":"WEAKER","freshness":"RECENT","confidence_percent":52,"last_seen_ago_ms":12345},"#,
    r#"{"address":"AA:BB:CC:DD:EE:03","name":null,"distance_m":null,"distance_lower_m":null,"distance_upper_m":null,"rssi_dbm":-90.2,"proximity":"FAR","trend":"UNKNOWN","freshness":"STALE","confidence_percent":0,"last_seen_ago_ms":75000},"#,
    r#"{"address":"AA:BB:CC:DD:EE:04","name":"Beacon Delta","distance_m":0.4,"distance_lower_m":0.3,"distance_upper_m":0.6,"rssi_dbm":-45.0,"proximity":"IMMEDIATE","trend":"STABLE","freshness":"LIVE","confidence_percent":99,"last_seen_ago_ms":90}"#,
    r#"],"scanning":true,"native_available":true,"timestamp_ms":1757700000000}"#,
);
/// `/api/status` as `ApiHttpServer.statusJson` writes it (uptime 1:02:03).
pub const STATUS_JSON: &str =
    r#"{"scanning":true,"device_count":4,"native_available":true,"uptime_ms":3723000}"#;
/// `/api/updates` as `ApiHttpServer.updatesJson` writes it.
pub const UPDATES_JSON: &str =
    r#"{"last_check_ms":1757600000000,"next_check_ms":1757686400000,"retry_count":2}"#;

/// What the mock answers on `/api/devices`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// The documented contract; the page must render every device.
    Healthy,
    /// `500` with a JSON error body; the page must show the banner.
    ServerError,
    /// `200` with an object that has no `devices` array; the banner again.
    WrongShape,
}

impl Scenario {
    /// Every scenario, in the order the command runs them.
    pub const ALL: [Scenario; 3] = [
        Scenario::Healthy,
        Scenario::ServerError,
        Scenario::WrongShape,
    ];

    /// The scenario's name in output paths and messages.
    pub fn label(self) -> &'static str {
        match self {
            Scenario::Healthy => "healthy",
            Scenario::ServerError => "server-error",
            Scenario::WrongShape => "wrong-shape",
        }
    }
}

/// One HTTP response of the mock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The reason phrase.
    pub reason: &'static str,
    /// The `Content-Type` header value.
    pub content_type: &'static str,
    /// The body bytes.
    pub body: Vec<u8>,
}

const JSON: &str = "application/json";
/// Identical to `ApiHttpServer.HTML`, so the browser sees the same header.
const HTML: &str = "text/html; charset=utf-8";

/// Routes one request exactly as `ApiHttpServer.respond` does (unknown path
/// → `404`, known path with a non-`GET` method → `405`), with `/api/devices`
/// shaped by the scenario.
pub fn route(scenario: Scenario, method: &str, path: &str, dashboard: &[u8]) -> Response {
    let known = matches!(path, "/" | "/api/devices" | "/api/status" | "/api/updates");
    if !known {
        return json(404, "Not Found", r#"{"error":"Not found"}"#);
    }
    if method != "GET" {
        return json(
            405,
            "Method Not Allowed",
            r#"{"error":"Method not allowed"}"#,
        );
    }
    match path {
        "/api/devices" => match scenario {
            Scenario::Healthy => json(200, "OK", DEVICES_JSON),
            Scenario::ServerError => json(500, "Internal Server Error", r#"{"error":"boom"}"#),
            Scenario::WrongShape => json(200, "OK", r#"{"nope":1}"#),
        },
        "/api/status" => json(200, "OK", STATUS_JSON),
        "/api/updates" => json(200, "OK", UPDATES_JSON),
        _ => Response {
            status: 200,
            reason: "OK",
            content_type: HTML,
            body: dashboard.to_vec(),
        },
    }
}

fn json(status: u16, reason: &'static str, body: &str) -> Response {
    Response {
        status,
        reason,
        content_type: JSON,
        body: body.as_bytes().to_vec(),
    }
}

/// Splits an HTTP/1.1 request line into method and path (query dropped), as
/// `ApiHttpServer.handle` does; `None` for a malformed line.
pub fn parse_request_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split(' ');
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parts.next()?;
    if parts.next().is_some() || method.is_empty() || !version.starts_with("HTTP/") {
        return None;
    }
    let path = target.split('?').next().unwrap_or(target);
    Some((method.to_string(), path.to_string()))
}

/// A loopback HTTP server answering the contract for one scenario and
/// recording every request path it received.
pub struct MockApi {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl MockApi {
    /// Binds an ephemeral loopback port and starts answering.
    pub fn start(scenario: Scenario, dashboard: Arc<Vec<u8>>) -> Result<MockApi, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("binding a loopback port for the mock API: {e}"))?;
        let address = listener
            .local_addr()
            .map_err(|e| format!("reading the mock API's address: {e}"))?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let accept_thread = {
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                for connection in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = connection else {
                        continue;
                    };
                    let requests = Arc::clone(&requests);
                    let dashboard = Arc::clone(&dashboard);
                    thread::spawn(move || {
                        serve_connection(stream, scenario, &dashboard, &requests)
                    });
                }
            })
        };
        Ok(MockApi {
            address,
            requests,
            stop,
            accept_thread: Some(accept_thread),
        })
    }

    /// `http://127.0.0.1:<port>/`.
    pub fn url(&self) -> String {
        format!("http://{}/", self.address)
    }

    /// Every request path received so far, in arrival order.
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    /// How many requests each path received.
    pub fn request_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for path in self.requests() {
            *counts.entry(path).or_insert(0) += 1;
        }
        counts
    }

    /// Stops accepting and joins the accept thread.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept so it observes the flag.
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_connection(
    mut stream: TcpStream,
    scenario: Scenario,
    dashboard: &[u8],
    requests: &Mutex<Vec<String>>,
) {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return; // A speculative connection that never sent a request.
    }
    // Drain the headers; no route reads them.
    let mut header = String::new();
    while reader.read_line(&mut header).unwrap_or(0) > 0 && header.trim_end().is_empty().eq(&false)
    {
        header.clear();
    }
    let response = match parse_request_line(request_line.trim_end()) {
        Some((method, path)) => {
            if let Ok(mut log) = requests.lock() {
                log.push(path.clone());
            }
            route(scenario, &method, &path, dashboard)
        }
        None => json(400, "Bad Request", r#"{"error":"Bad request"}"#),
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.reason,
        response.content_type,
        response.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

/// Finds a Chromium/Chrome binary: `BLERADAR_CHROMIUM` (must exist), then
/// the `PATH` candidates, then Playwright's browser cache
/// (`PLAYWRIGHT_BROWSERS_PATH` or `~/.cache/ms-playwright`, newest
/// `chromium-*` first).
pub fn locate_chromium() -> Result<PathBuf, String> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let mut caches = Vec::new();
    if let Some(dir) = env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        caches.push(PathBuf::from(dir));
    }
    if let Some(home) = home {
        caches.push(home.join(".cache/ms-playwright"));
    }
    resolve_chromium(
        env::var_os("BLERADAR_CHROMIUM").as_deref(),
        env::var_os("PATH").as_deref(),
        &caches,
    )
}

/// The pure resolution behind [`locate_chromium`].
pub fn resolve_chromium(
    override_path: Option<&OsStr>,
    search_path: Option<&OsStr>,
    playwright_caches: &[PathBuf],
) -> Result<PathBuf, String> {
    if let Some(value) = override_path {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "BLERADAR_CHROMIUM={} is not a file",
            path.display()
        ));
    }
    if let Some(search_path) = search_path {
        for name in PATH_CANDIDATES {
            if let Some(found) = env::split_paths(search_path)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
            {
                return Ok(found);
            }
        }
    }
    for cache in playwright_caches {
        let Ok(entries) = fs::read_dir(cache) else {
            continue;
        };
        let mut builds: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.starts_with("chromium-"))
            })
            .collect();
        builds.sort();
        if let Some(chrome) = builds
            .iter()
            .rev()
            .map(|build| build.join("chrome-linux/chrome"))
            .find(|chrome| chrome.is_file())
        {
            return Ok(chrome);
        }
    }
    Err(format!(
        "no Chromium/Chrome found: set BLERADAR_CHROMIUM=<binary>, put one of {} on PATH, or install Playwright's chromium",
        PATH_CANDIDATES.join(", ")
    ))
}

/// Runs the browser headless on `url` for [`VIRTUAL_TIME_BUDGET_MS`] of
/// virtual time and returns the serialized DOM (`--dump-dom`), or a
/// screenshot's bytes when `screenshot` is set.
pub fn render(
    chromium: &Path,
    url: &str,
    profile_dir: &Path,
    screenshot: Option<&Path>,
) -> Result<String, String> {
    let mut command = Command::new(chromium);
    command
        .arg("--headless")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--disable-dev-shm-usage")
        .arg("--no-first-run")
        .arg("--disable-extensions")
        .arg("--hide-scrollbars")
        .arg("--window-size=900,900")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--virtual-time-budget={VIRTUAL_TIME_BUDGET_MS}"));
    match screenshot {
        Some(path) => {
            command.arg(format!("--screenshot={}", path.display()));
        }
        None => {
            command.arg("--dump-dom");
        }
    }
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = format!("{command:?}");
    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    let stdout = child.stdout.take().ok_or("browser stdout not captured")?;
    let stderr = child.stderr.take().ok_or("browser stderr not captured")?;
    let stdout_thread = thread::spawn(move || read_all(stdout));
    let stderr_thread = thread::spawn(move || read_all(stderr));
    let deadline = Instant::now() + BROWSER_TIMEOUT;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("waiting for the browser: {e}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "the browser did not finish within {}s: {program}",
                BROWSER_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "{program} exited with {status}; stderr tail:\n{}",
            tail(&String::from_utf8_lossy(&stderr), 20)
        ));
    }
    String::from_utf8(stdout).map_err(|e| format!("the browser printed non-UTF-8 output: {e}"))
}

fn read_all(mut source: impl Read) -> Vec<u8> {
    let mut buffer = Vec::new();
    let _ = source.read_to_end(&mut buffer);
    buffer
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Every marker the healthy page must contain, in the order the four
/// device rows must appear for the address markers (the server's ranked
/// order, which the page must preserve). Text is as Chromium serializes it:
/// markup characters in names come back entity-escaped.
pub const HEALTHY_MARKERS: &[&str] = &[
    r#"data-state="live""#,
    r#">Scanning<"#,
    "native core: loaded",
    ">4 devices<",
    ">1:02:03<",
    "retries 2",
    r#"data-address="AA:BB:CC:DD:EE:01""#,
    r#"data-address="AA:BB:CC:DD:EE:02""#,
    r#"data-address="AA:BB:CC:DD:EE:03""#,
    r#"data-address="AA:BB:CC:DD:EE:04""#,
    "Tag Alpha",
    "&lt;b&gt;evil&lt;/b&gt; Ünïcødé 😀",
    "Unnamed device",
    "Beacon Delta",
    ">1.2 m<",
    "0.8–1.9 m",
    ">7.5 m<",
    ">0.4 m<",
    ">-61 dBm<",
    ">-78 dBm<",
    ">-90 dBm<",
    ">-45 dBm<",
    r#"class="prox-NEAR">NEAR<"#,
    r#"class="prox-MID">MID<"#,
    r#"class="prox-FAR">FAR<"#,
    r#"class="prox-IMMEDIATE">IMMEDIATE<"#,
    ">▲ STRONGER<",
    ">▼ WEAKER<",
    ">— UNKNOWN<",
    ">■ STABLE<",
    r#"data-freshness="LIVE""#,
    r#"data-freshness="RECENT""#,
    r#"data-freshness="STALE""#,
    ">87%<",
    ">52%<",
    ">0%<",
    ">99%<",
    ">0.4 s ago<",
    ">12 s ago<",
    ">1m 15s ago<",
    ">0.1 s ago<",
];

/// Text that must never appear: the device name rendered as markup.
pub const FORBIDDEN_MARKERS: &[&str] = &["<b>evil</b>"];

/// Fewest polls the healthy page must have completed within the budget.
pub const MIN_POLLS: u32 = 2;

/// Checks the DOM one scenario produced; `Ok` carries the poll count.
pub fn check_dom(scenario: Scenario, dom: &str) -> Result<u32, String> {
    let polls = data_polls(dom).ok_or("the page carries no data-polls counter")?;
    match scenario {
        Scenario::Healthy => {
            for marker in HEALTHY_MARKERS {
                if !dom.contains(marker) {
                    return Err(format!("rendered page lacks `{marker}`"));
                }
            }
            for marker in FORBIDDEN_MARKERS {
                if dom.contains(marker) {
                    return Err(format!("rendered page contains `{marker}` as markup"));
                }
            }
            let addresses: Vec<usize> = HEALTHY_MARKERS
                .iter()
                .filter(|marker| marker.starts_with("data-address="))
                .filter_map(|marker| dom.find(marker))
                .collect();
            if addresses.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err("device rows are not in the server's order".to_string());
            }
            if polls < MIN_POLLS {
                return Err(format!(
                    "the page completed {polls} poll(s); at least {MIN_POLLS} expected"
                ));
            }
            Ok(polls)
        }
        Scenario::ServerError | Scenario::WrongShape => {
            if !dom.contains(r#"data-state="error""#) {
                return Err("the page did not enter the error state".to_string());
            }
            let banner = match scenario {
                Scenario::ServerError => "API unreachable: HTTP 500 from /api/devices",
                _ => "API unreachable: /api/devices did not return a devices array",
            };
            if !dom.contains(banner) {
                return Err(format!("rendered page lacks the banner {banner:?}"));
            }
            if polls != 0 {
                return Err(format!(
                    "the page counted {polls} successful poll(s) although every /api/devices answer was invalid"
                ));
            }
            Ok(polls)
        }
    }
}

/// The `data-polls` counter on `<body>`.
pub fn data_polls(dom: &str) -> Option<u32> {
    let start = dom.find(r#"data-polls=""#)? + r#"data-polls=""#.len();
    let rest = &dom[start..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

/// Request counts the healthy run must reach: the page, more than one
/// devices/status poll, and the updates endpoint on its first poll.
pub fn check_requests(counts: &BTreeMap<String, usize>) -> Result<(), String> {
    for (path, minimum) in [
        ("/", 1),
        ("/api/devices", MIN_POLLS as usize),
        ("/api/status", MIN_POLLS as usize),
        ("/api/updates", 1),
    ] {
        let seen = counts.get(path).copied().unwrap_or(0);
        if seen < minimum {
            return Err(format!(
                "the mock received {seen} request(s) for {path}; at least {minimum} expected"
            ));
        }
    }
    Ok(())
}

/// The whole command: locate the browser, read the page, run every scenario.
pub fn run(root: &Path) -> Result<(), String> {
    let chromium = locate_chromium()?;
    let asset_path = root.join(DASHBOARD_ASSET_PATH);
    let dashboard = Arc::new(
        fs::read(&asset_path).map_err(|e| format!("reading {}: {e}", asset_path.display()))?,
    );
    let output_dir = root.join(OUTPUT_DIR);
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir)
        .map_err(|e| format!("creating {}: {e}", output_dir.display()))?;
    println!(
        "browser={} ({} bytes of dashboard from {})",
        chromium.display(),
        dashboard.len(),
        DASHBOARD_ASSET_PATH
    );

    for scenario in Scenario::ALL {
        let mock = MockApi::start(scenario, Arc::clone(&dashboard))?;
        let profile_dir = output_dir.join(format!("profile-{}", scenario.label()));
        let dom = render(&chromium, &mock.url(), &profile_dir, None);
        let counts = mock.request_counts();
        mock.stop();
        let dom = dom.map_err(|e| format!("[{}] {e}", scenario.label()))?;
        let dom_path = output_dir.join(format!("{}.html", scenario.label()));
        fs::write(&dom_path, &dom).map_err(|e| format!("writing {}: {e}", dom_path.display()))?;
        let polls = check_dom(scenario, &dom).map_err(|e| {
            format!(
                "[{}] {e} (rendered DOM kept at {})",
                scenario.label(),
                dom_path.display()
            )
        })?;
        if scenario == Scenario::Healthy {
            check_requests(&counts).map_err(|e| format!("[{}] {e}", scenario.label()))?;
        }
        let requests: Vec<String> = counts
            .iter()
            .map(|(path, count)| format!("{path}×{count}"))
            .collect();
        println!(
            "[{}] ok: polls={polls}, requests: {}",
            scenario.label(),
            requests.join(" ")
        );
    }

    // A screenshot of the healthy page, for human inspection only.
    let mock = MockApi::start(Scenario::Healthy, Arc::clone(&dashboard))?;
    let screenshot = output_dir.join("dashboard.png");
    let result = render(
        &chromium,
        &mock.url(),
        &output_dir.join("profile-screenshot"),
        Some(&screenshot),
    );
    mock.stop();
    result?;
    let size = fs::metadata(&screenshot).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        return Err(format!(
            "the browser produced no screenshot at {}",
            screenshot.display()
        ));
    }
    println!(
        "verify-dashboard-live: {} scenarios rendered by headless Chromium; screenshot {} ({size} bytes)",
        Scenario::ALL.len(),
        screenshot.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document carrying every healthy marker exactly once (the
    /// `data-state` attribute comes from the marker list itself).
    fn healthy_dom() -> String {
        let mut dom = String::from(r#"<html><body data-polls="4" "#);
        for marker in HEALTHY_MARKERS {
            dom.push_str(marker);
            dom.push('\n');
        }
        dom.push_str("</body></html>");
        dom
    }

    #[test]
    fn route_mirrors_api_http_server() {
        let page = b"<html>page</html>";
        let index = route(Scenario::Healthy, "GET", "/", page);
        assert_eq!((index.status, index.content_type), (200, HTML));
        assert_eq!(index.body, page);
        assert_eq!(
            route(Scenario::Healthy, "GET", "/api/devices", page).body,
            DEVICES_JSON.as_bytes()
        );
        assert_eq!(
            route(Scenario::ServerError, "GET", "/api/devices", page).status,
            500
        );
        let wrong = route(Scenario::WrongShape, "GET", "/api/devices", page);
        assert_eq!(wrong.status, 200);
        assert!(!String::from_utf8_lossy(&wrong.body).contains("devices"));
        assert_eq!(
            route(Scenario::Healthy, "GET", "/api/status", page).body,
            STATUS_JSON.as_bytes()
        );
        assert_eq!(
            route(Scenario::Healthy, "GET", "/api/updates", page).body,
            UPDATES_JSON.as_bytes()
        );
        assert_eq!(
            route(Scenario::Healthy, "GET", "/favicon.ico", page).status,
            404
        );
        assert_eq!(
            route(Scenario::Healthy, "POST", "/api/status", page).status,
            405
        );
        assert_eq!(route(Scenario::Healthy, "POST", "/nope", page).status, 404);
    }

    #[test]
    fn request_lines_are_parsed_like_the_app_does() {
        assert_eq!(
            parse_request_line("GET /api/devices?x=1 HTTP/1.1"),
            Some(("GET".to_string(), "/api/devices".to_string()))
        );
        assert_eq!(parse_request_line("GET /"), None);
        assert_eq!(parse_request_line("GET / HTTP/1.1 extra"), None);
        assert_eq!(parse_request_line("GET / SPDY/3"), None);
    }

    #[test]
    fn healthy_dom_check_requires_every_marker_in_order() {
        assert_eq!(check_dom(Scenario::Healthy, &healthy_dom()), Ok(4));
        for marker in HEALTHY_MARKERS {
            let without = healthy_dom().replacen(marker, "", 1);
            let error = check_dom(Scenario::Healthy, &without).unwrap_err();
            assert!(error.contains(marker), "{error}");
        }
        let swapped = healthy_dom().replace(
            "data-address=\"AA:BB:CC:DD:EE:01\"\ndata-address=\"AA:BB:CC:DD:EE:02\"",
            "data-address=\"AA:BB:CC:DD:EE:02\"\ndata-address=\"AA:BB:CC:DD:EE:01\"",
        );
        assert_eq!(
            check_dom(Scenario::Healthy, &swapped).unwrap_err(),
            "device rows are not in the server's order"
        );
        let injected = healthy_dom() + "<b>evil</b>";
        assert!(
            check_dom(Scenario::Healthy, &injected)
                .unwrap_err()
                .contains("as markup")
        );
        let one_poll = healthy_dom().replace(r#"data-polls="4""#, r#"data-polls="1""#);
        assert!(
            check_dom(Scenario::Healthy, &one_poll)
                .unwrap_err()
                .contains("1 poll(s)")
        );
        assert!(
            check_dom(Scenario::Healthy, "<html></html>")
                .unwrap_err()
                .contains("data-polls")
        );
    }

    #[test]
    fn degraded_dom_checks_require_the_banner_and_no_successful_poll() {
        let server_error = r#"<body data-state="error" data-polls="0"><div id="error">API unreachable: HTTP 500 from /api/devices</div></body>"#;
        assert_eq!(check_dom(Scenario::ServerError, server_error), Ok(0));
        assert!(
            check_dom(Scenario::WrongShape, server_error)
                .unwrap_err()
                .contains("banner")
        );
        let wrong_shape = r#"<body data-state="error" data-polls="0"><div id="error">API unreachable: /api/devices did not return a devices array</div></body>"#;
        assert_eq!(check_dom(Scenario::WrongShape, wrong_shape), Ok(0));
        let stale_success = server_error.replace(r#"data-polls="0""#, r#"data-polls="3""#);
        assert!(
            check_dom(Scenario::ServerError, &stale_success)
                .unwrap_err()
                .contains("3 successful poll(s)")
        );
        let live = r#"<body data-state="live" data-polls="0"></body>"#;
        assert_eq!(
            check_dom(Scenario::ServerError, live).unwrap_err(),
            "the page did not enter the error state"
        );
    }

    #[test]
    fn request_count_check_names_the_short_endpoint() {
        let mut counts = BTreeMap::new();
        for (path, count) in [
            ("/", 1),
            ("/api/devices", 4),
            ("/api/status", 4),
            ("/api/updates", 1),
        ] {
            counts.insert(path.to_string(), count);
        }
        assert_eq!(check_requests(&counts), Ok(()));
        counts.insert("/api/updates".to_string(), 0);
        assert!(
            check_requests(&counts)
                .unwrap_err()
                .contains("/api/updates")
        );
    }

    #[test]
    fn mock_api_answers_over_tcp_and_records_requests() {
        let mock = MockApi::start(Scenario::Healthy, Arc::new(b"<html>page</html>".to_vec()))
            .expect("bind loopback");
        let address = mock
            .url()
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string();
        let mut stream = TcpStream::connect(&address).expect("connect");
        stream
            .write_all(b"GET /api/status?verbose=1 HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains("Content-Type: application/json\r\n"));
        assert!(response.ends_with(STATUS_JSON), "{response}");
        let mut stream = TcpStream::connect(&address).expect("connect");
        stream
            .write_all(b"GET / HTTP/1.1\r\n\r\n")
            .expect("write request");
        let mut page = String::new();
        stream.read_to_string(&mut page).expect("read response");
        assert!(page.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(page.ends_with("<html>page</html>"));
        assert_eq!(mock.requests(), vec!["/api/status", "/"]);
        mock.stop();
    }

    #[test]
    fn chromium_resolution_prefers_the_override_then_path_then_playwright_cache() {
        let scratch =
            env::temp_dir().join(format!("xtask-chromium-resolve-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        let bin = scratch.join("bin");
        let cache = scratch.join("cache");
        let build = cache.join("chromium-1194/chrome-linux");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&build).unwrap();
        fs::create_dir_all(cache.join("chromium-1100/chrome-linux")).unwrap();
        fs::write(build.join("chrome"), b"").unwrap();
        fs::write(cache.join("chromium-1100/chrome-linux/chrome"), b"").unwrap();
        let empty_path = OsStr::new("");

        let missing = scratch.join("missing");
        assert!(
            resolve_chromium(Some(missing.as_os_str()), None, &[])
                .unwrap_err()
                .contains("BLERADAR_CHROMIUM")
        );
        let override_bin = build.join("chrome");
        assert_eq!(
            resolve_chromium(Some(override_bin.as_os_str()), None, &[]).unwrap(),
            override_bin
        );

        fs::write(bin.join("google-chrome"), b"").unwrap();
        assert_eq!(
            resolve_chromium(None, Some(bin.as_os_str()), std::slice::from_ref(&cache)).unwrap(),
            bin.join("google-chrome")
        );
        assert_eq!(
            resolve_chromium(None, Some(empty_path), std::slice::from_ref(&cache)).unwrap(),
            build.join("chrome"),
            "the newest Playwright build wins"
        );
        assert!(
            resolve_chromium(None, Some(empty_path), &[])
                .unwrap_err()
                .contains("BLERADAR_CHROMIUM")
        );
        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    fn data_polls_reads_the_body_counter() {
        assert_eq!(
            data_polls(r#"<body data-state="live" data-polls="12">"#),
            Some(12)
        );
        assert_eq!(data_polls(r#"<body data-polls="x">"#), None);
        assert_eq!(data_polls("<body>"), None);
    }

    #[test]
    fn fixtures_are_the_documented_contract() {
        for field in [
            "\"address\"",
            "\"name\"",
            "\"distance_m\"",
            "\"distance_lower_m\"",
            "\"distance_upper_m\"",
            "\"rssi_dbm\"",
            "\"proximity\"",
            "\"trend\"",
            "\"freshness\"",
            "\"confidence_percent\"",
            "\"last_seen_ago_ms\"",
        ] {
            assert_eq!(
                DEVICES_JSON.matches(field).count(),
                4,
                "{field} on every device"
            );
        }
        for field in ["\"scanning\"", "\"native_available\"", "\"timestamp_ms\""] {
            assert_eq!(
                DEVICES_JSON.matches(field).count(),
                1,
                "{field} once at top level"
            );
        }
        for field in [
            "\"scanning\"",
            "\"device_count\"",
            "\"native_available\"",
            "\"uptime_ms\"",
        ] {
            assert!(STATUS_JSON.contains(field), "{field}");
        }
        for field in ["\"last_check_ms\"", "\"next_check_ms\"", "\"retry_count\""] {
            assert!(UPDATES_JSON.contains(field), "{field}");
        }
    }
}
