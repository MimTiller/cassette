//! More users: extra real browsers on the run's app, each one a full robot
//! driver with its own sign-in.
//!
//! `idealyst test` drives one browser through its robot relay, and that relay
//! routes every request to the newest tab that dials it. A second user
//! therefore gets its own relay, started here, and its own headless Chrome
//! (its own profile, so its own cookies and storage) loading the same app
//! from the run's server. The page dials its own origin's relay path first,
//! which leads to the run's relay and would take over the suite's browser,
//! so over the DevTools protocol, before the page loads, a script wraps the
//! page's `WebSocket` to send that dial to this relay instead, and pins
//! `window.IDEALYST_ROBOT_RELAY_URL` (the page's fallback) to it as well.
//! (`Network.setBlockedURLs` does not cover WebSockets.) The returned [`App`]
//! then drives that browser exactly like the suite's own, screenshots
//! included (the relay captures the tab over CDP).

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use robot_test::{json, App, Value};

/// One extra signed-out browser on the app. Dropping it closes the browser.
pub struct Browser {
    pub app: App,
    chrome: Child,
    _relay: robot_relay::RelayHandle,
    _profiles: tempfile::TempDir,
    _cdp: std::thread::JoinHandle<()>,
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.chrome.kill();
        let _ = self.chrome.wait();
    }
}

/// Open another browser on the app as `label` (used in its screenshots'
/// file names) and return its driver once the app in it answers.
pub fn open_browser(label: &str) -> Browser {
    let profiles = tempfile::Builder::new().prefix("cassette-robot-").tempdir().expect("profiles dir");
    let relay = robot_relay::start(robot_relay::RelayConfig {
        register: false,
        identity: Some(robot_relay::Identity {
            name: format!("cassette-{label}"),
            bundle_id: None,
            project_root: None,
        }),
        cdp_profiles_dir: Some(profiles.path().to_path_buf()),
        under_test: true,
        ..Default::default()
    })
    .expect("start a relay for the extra browser");

    let profile = profiles.path().join(label);
    let chrome = Command::new(chrome_path())
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            "--window-size=1280,800",
            "--remote-debugging-port=0",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch the extra headless browser");

    let port = devtools_port(&profile);
    let cdp = page_socket(port);
    let relay_url = format!("ws://{}", relay.ws_addr);
    let cdp_thread = drive_page(cdp, &relay_url, &crate::support::base());

    let app = App::connect(relay.tcp_addr).expect("connect to the extra browser's relay");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !relay.apps().iter().any(|a| a.platform.is_some()) {
        assert!(Instant::now() < deadline, "the app in the extra browser `{label}` never dialed its relay");
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut browser = Browser { app, chrome, _relay: relay, _profiles: profiles, _cdp: cdp_thread };
    browser.app.set_timeout(Duration::from_secs(20));
    browser
}

/// The browser `idealyst test` uses (`IDEALYST_CHROME`), else this host's wrapper.
fn chrome_path() -> PathBuf {
    std::env::var_os("IDEALYST_CHROME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root/bin/chrome-robot"))
}

/// The DevTools port Chrome wrote to `<profile>/DevToolsActivePort`.
fn devtools_port(profile: &Path) -> u16 {
    let file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(port) = std::fs::read_to_string(&file).ok().and_then(|s| s.lines().next()?.trim().parse().ok()) {
            return port;
        }
        assert!(Instant::now() < deadline, "the extra browser never opened DevTools ({})", file.display());
        std::thread::sleep(Duration::from_millis(100));
    }
}

type Cdp = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

/// A DevTools connection to the browser's first page.
fn page_socket(port: u16) -> Cdp {
    let deadline = Instant::now() + Duration::from_secs(30);
    let ws_url = loop {
        let targets = http_get(port, "/json/list").ok().and_then(|b| serde_json::from_str::<Value>(&b).ok());
        let page = targets
            .as_ref()
            .and_then(|t| t.as_array())
            .and_then(|t| t.iter().find(|t| t["type"] == "page"))
            .and_then(|t| t["webSocketDebuggerUrl"].as_str())
            .map(str::to_string);
        if let Some(url) = page {
            break url;
        }
        assert!(Instant::now() < deadline, "the extra browser has no page target");
        std::thread::sleep(Duration::from_millis(100));
    };
    tungstenite::connect(ws_url).expect("DevTools websocket").0
}

/// A GET against Chrome's DevTools endpoint, which answers HTTP/1.1 only.
fn http_get(port: u16, path: &str) -> Result<String, Box<ureq::Error>> {
    Ok(ureq::get(&format!("http://127.0.0.1:{port}{path}")).call().map_err(Box::new)?.into_string().unwrap_or_default())
}

/// Point the page at `relay_url`, load the app from `base`, and keep the
/// DevTools session open (its settings last only as long as it does).
fn drive_page(mut cdp: Cdp, relay_url: &str, base: &str) -> std::thread::JoinHandle<()> {
    let pin_relay = format!(
        "Object.defineProperty(window, 'IDEALYST_ROBOT_RELAY_URL', \
         {{ get: () => {relay:?}, set: () => {{}}, configurable: false }}); \
         window.WebSocket = new Proxy(window.WebSocket, {{ construct(target, args) {{ \
           if (String(args[0]).includes('/__idealyst/relay')) args[0] = {relay:?}; \
           return new target(...args); }} }});",
        relay = relay_url
    );
    let mut id = 0;
    let mut send = |cdp: &mut Cdp, method: &str, params: Value| {
        id += 1;
        let msg = json!({ "id": id, "method": method, "params": params }).to_string();
        cdp.send(tungstenite::Message::Text(msg)).expect("DevTools send");
        // Wait for this command's reply, skipping events.
        loop {
            let reply = cdp.read().expect("DevTools read");
            let Ok(v) = serde_json::from_str::<Value>(reply.to_text().unwrap_or("")) else { continue };
            if v["id"] == json!(id) {
                assert!(v.get("error").is_none(), "DevTools {method} failed: {}", v["error"]);
                break;
            }
        }
    };
    send(&mut cdp, "Page.enable", json!({}));
    send(&mut cdp, "Page.addScriptToEvaluateOnNewDocument", json!({ "source": pin_relay }));
    send(&mut cdp, "Page.navigate", json!({ "url": format!("{base}/") }));
    std::thread::spawn(move || while cdp.read().is_ok() {})
}
