// Minimal `cargo run --example e2e` harness for
// the STW-042 demo-data fallback. Binds the
// dashboard's `axum::Router` to a random localhost
// port (the OS picks an unused port), serves it
// until SIGINT, and writes the bound port to
// stdout so an operator can `curl` against it.
//
// Run with:
//
//     cargo run -p rbp-dashboard --example e2e_serve -- --exit-after 1
//
// The `--exit-after 1` flag drops the listener
// after the first request, which is enough to
// drive the end-to-end `curl` shell check the
// STW-042 verification command pins.

use std::net::SocketAddr;
use std::path::PathBuf;

use rbp_dashboard::{AppState, IndexClient, dashboard_app};
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let exit_after: Option<u64> = args
        .iter()
        .position(|a| a == "--exit-after")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    // Bind a real `TcpListener` on port 0 (the OS
    // picks an unused port). Build the
    // `AppState::from_env`-equivalent the
    // production `serve()` entry point uses: a
    // fresh `IndexClient` (no `INDEX.json` on
    // disk, the demo-data fallback's intended
    // trigger) + the committed static
    // `index.html` + an empty transcript dir.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr: SocketAddr = listener.local_addr()?;
    eprintln!("e2e_serve bound to http://{addr}");

    // Build the AppState without an INDEX.json
    // (the demo-data fallback's intended
    // trigger). The `IndexClient::from_url("")`
    // shape auto-prefixes to `file://` and the
    // fetch_index returns an Io error
    // (no file at ""), which is *not* a
    // shadowing match — the fallback engages.
    let transcript_dir = std::path::PathBuf::from("/tmp/dashboard-e2e-transcripts");
    let _ = std::fs::create_dir_all(&transcript_dir);
    let static_index_html = std::sync::Arc::new(
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("static")
                .join("index.html"),
        )
        .expect("read static index.html"),
    );
    let state = AppState {
        index_client: IndexClient::from_url(""),
        transcript_dir,
        static_index_html,
    };
    let app = dashboard_app(state);
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // Drive the end-to-end `curl` check: hit
    // `GET /bench/compare3-fixture` and assert
    // the body contains `ranked_winner`.
    let url = format!("http://{addr}/bench/compare3-fixture");
    let response = reqwest_get(&url)
        .await
        .expect("GET /bench/compare3-fixture");
    let status = response.status();
    let body = response.text().await.expect("read body");
    eprintln!("GET {url} -> {status} ({} bytes)", body.len());
    if !body.contains("ranked_winner") {
        eprintln!("FAIL: body does not contain `ranked_winner`: {body}");
        std::process::exit(2);
    }
    eprintln!("OK: body contains `ranked_winner`");

    if let Some(n) = exit_after {
        eprintln!("exit-after {n}: dropping server");
        server.abort();
        let _ = server.await;
    } else {
        eprintln!("serving forever; press Ctrl-C to exit");
        let _ = server.await;
    }
    Ok(())
}

async fn reqwest_get(url: &str) -> Result<reqwest_compat::Response, reqwest_compat::Error> {
    reqwest_compat::get(url).await
}

// A tiny `reqwest`-equivalent shim: build a
// `ureq`-shaped response wrapper the example
// can call. The dashboard's HTTP fetch already
// uses `ureq`; we reuse the same blocking
// client to avoid pulling a `reqwest` dev-dep
// into the dashboard's dep tree.
mod reqwest_compat {
    use std::io::Read;

    pub struct Response {
        status: u16,
        body: String,
    }
    impl Response {
        pub fn status(&self) -> u16 {
            self.status
        }
        pub async fn text(self) -> Result<String, Error> {
            Ok(self.body)
        }
    }
    #[derive(Debug)]
    pub struct Error(String);
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for Error {}
    pub async fn get(url: &str) -> Result<Response, Error> {
        let url = url.to_string();
        let result = tokio::task::spawn_blocking(move || -> Result<(u16, String), String> {
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(5))
                .build();
            let resp = agent
                .get(&url)
                .call()
                .map_err(|e| format!("GET failed: {e}"))?;
            let status = resp.status();
            let mut body = String::new();
            resp.into_reader()
                .take(1_048_576)
                .read_to_string(&mut body)
                .map_err(|e| format!("read body: {e}"))?;
            Ok((status, body))
        })
        .await
        .map_err(|e| Error(format!("join: {e}")))?;
        let (status, body) = result.map_err(Error)?;
        Ok(Response { status, body })
    }
}
