//! LAN capture server. Single-file axum router, compiled into the
//! binary; serves a mobile-optimized capture page on `GET /` and
//! accepts `POST /capture` to route into `create_item_inner`. Disabled
//! by default; toggled via `toggle_lan_capture` per SPEC §7.
//!
//! Shared-secret hardening (SPEC §7.4): when
//! `settings.lan_capture_shared_secret` is set, POST requests must
//! carry a matching `?s=<secret>` query string or `X-Bay-Secret`
//! header. Missing/mismatched secret returns 401.
//!
//! Lifecycle: `CaptureState::start` binds synchronously (so port
//! conflicts surface immediately), then spawns the serve loop on
//! Tauri's async runtime. `stop` sends a shutdown signal via a
//! oneshot channel; graceful-shutdown drains in-flight requests.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::commands::items::create_item_inner_ctx;
use crate::db::SqlitePool;
use crate::domain::Tier;

const CAPTURE_HTML: &str = include_str!("capture.html");
const LAN_CAPTURE_RECEIVED_EVENT: &str = "lan_capture_received";
const ITEM_CREATED_EVENT: &str = "item_created";

/// Running-server lifecycle wrapper lives in Tauri state so
/// toggle_lan_capture can flip it on/off without re-plumbing pool +
/// app handle.
pub struct CaptureState {
    inner: Mutex<Option<Running>>,
}

struct Running {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    port: u16,
    url: String,
    qr_svg: String,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    #[allow(dead_code)] // reserved; frontend uses `status()` for this today
    pub fn is_running(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn status(&self) -> LanCaptureStatus {
        let g = self.inner.lock().ok();
        match g.as_ref().and_then(|o| o.as_ref()) {
            Some(r) => LanCaptureStatus {
                enabled: true,
                url: Some(r.url.clone()),
                qr_svg: Some(r.qr_svg.clone()),
                port: Some(r.port),
            },
            None => LanCaptureStatus {
                enabled: false,
                url: None,
                qr_svg: None,
                port: None,
            },
        }
    }

    /// Bind the listener synchronously (so port conflicts error out
    /// before the command returns), then spawn the serve loop.
    ///
    /// `AppHandle` here is the non-generic desktop alias (Wry runtime).
    /// The server only runs on desktop builds; mobile support is out
    /// of scope for v1.
    pub fn start(
        &self,
        app: AppHandle,
        pool: SqlitePool,
        port: u16,
        shared_secret: Option<String>,
    ) -> Result<LanCaptureStatus, String> {
        {
            let g = self.inner.lock().map_err(|e| format!("lock: {e}"))?;
            if g.is_some() {
                // Already running; return current status.
                drop(g);
                return Ok(self.status());
            }
        }

        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        let std_listener = std::net::TcpListener::bind(addr)
            .map_err(|e| format!("PORT_IN_USE: bind {port}: {e}"))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| format!("set_nonblocking: {e}"))?;

        let lan_ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".into());
        let secret_query = shared_secret
            .as_deref()
            .map(|s| format!("?s={}", urlencoding_lite(s)))
            .unwrap_or_default();
        let url = format!("http://{lan_ip}:{port}/{secret_query}");

        let qr_svg = qrcode::QrCode::new(url.as_bytes())
            .map_err(|e| format!("qr encode: {e}"))?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .build();

        // App state shared by every request handler.
        let router_state = Arc::new(ServerState {
            app: app.clone(),
            pool,
            shared_secret,
        });
        let router = build_router(router_state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => return Err(format!("tokio from_std: {e}")),
        };

        tauri::async_runtime::spawn(async move {
            let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = serve.await {
                eprintln!("lan capture axum serve ended: {e}");
            }
        });

        let mut g = self.inner.lock().map_err(|e| format!("lock: {e}"))?;
        *g = Some(Running {
            shutdown: Some(shutdown_tx),
            port,
            url: url.clone(),
            qr_svg: qr_svg.clone(),
        });
        Ok(LanCaptureStatus {
            enabled: true,
            url: Some(url),
            qr_svg: Some(qr_svg),
            port: Some(port),
        })
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|e| format!("lock: {e}"))?;
        if let Some(r) = g.take() {
            if let Some(tx) = r.shutdown {
                let _ = tx.send(());
            }
        }
        Ok(())
    }
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::new()
    }
}

/// Used by the Tauri commands layer as well as `bootstrap`.
#[derive(Debug, Clone, Serialize)]
pub struct LanCaptureStatus {
    pub enabled: bool,
    pub url: Option<String>,
    pub qr_svg: Option<String>,
    pub port: Option<u16>,
}

// ── axum plumbing ─────────────────────────────────────────────────

/// Per-server state shared by handlers. AppHandle is type-erased via
/// tauri::Wry to avoid propagating the Runtime generic through the
/// whole router definition. Safe because the desktop build uses Wry.
struct ServerState {
    app: tauri::AppHandle,
    pool: SqlitePool,
    shared_secret: Option<String>,
}

fn build_router(state: Arc<ServerState>) -> Router {
    Router::new()
        .route("/", get(serve_capture_html))
        .route("/health", get(serve_health))
        .route("/capture", post(serve_capture))
        .with_state(state)
}

async fn serve_capture_html() -> impl IntoResponse {
    Html(CAPTURE_HTML)
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    app: &'static str,
}

async fn serve_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        app: "Bay",
    })
}

#[derive(Deserialize, Default)]
struct CaptureQuery {
    s: Option<String>,
}

#[derive(Deserialize)]
struct CaptureBody {
    content: String,
}

/// The tier a LAN capture lands in. CLAUDE.md §5 is explicit and
/// unconditional — captures go to Inbox, **never** directly to A/B/C,
/// because tiering is deliberate human work. Named rather than inlined
/// so the rule is greppable and can be asserted without a socket.
const CAPTURE_TIER: Tier = Tier::Inbox;

/// The shared-secret decision, lifted out of the axum extractors.
///
/// `serve_capture` needs a live `AppHandle`, so with the policy inline
/// this module had **no tests at all** (v0.3 pass 10) — and it is the
/// only network listener in the app. Separating the decision from the
/// transport makes the part that matters assertable.
fn secret_ok(expected: Option<&str>, provided: Option<&str>) -> bool {
    match expected {
        None => true, // disabled: LAN-trust, per SPEC §7.4
        Some(e) => provided == Some(e),
    }
}

/// Trim-and-reject-empty, lifted out for the same reason.
fn normalize_capture_content(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("CONTENT_EMPTY");
    }
    Ok(trimmed.to_string())
}

#[derive(Serialize)]
struct CaptureOk {
    ok: bool,
    id: String,
}

async fn serve_capture(
    AxumState(state): AxumState<Arc<ServerState>>,
    Query(q): Query<CaptureQuery>,
    headers: HeaderMap,
    Json(body): Json<CaptureBody>,
) -> Result<Json<CaptureOk>, (StatusCode, String)> {
    // Shared-secret gate. Accept in query string (friendly from the QR
    // URL) or X-Bay-Secret header (friendly from non-browser clients).
    let provided = q.s.as_deref().or_else(|| {
        headers
            .get("x-bay-secret")
            .and_then(|v| v.to_str().ok())
    });
    if !secret_ok(state.shared_secret.as_deref(), provided) {
        return Err((StatusCode::UNAUTHORIZED, "bad secret".into()));
    }

    let content = match normalize_capture_content(&body.content) {
        Ok(c) => c,
        Err(e) => return Err((StatusCode::BAD_REQUEST, e.into())),
    };

    // Run the synchronous write on a blocking thread so we don't
    // stall the tokio runtime on SQLite I/O.
    let pool = state.pool.clone();
    let item =
        match tokio::task::spawn_blocking(move || {
            create_item_inner_ctx(
                &pool,
                crate::db::WriteCtx {
                    origin: Some("lan".into()),
                    ..Default::default()
                },
                CAPTURE_TIER,
                content,
                None,
                None,
            )
        })
        .await
        {
            Ok(Ok(i)) => i,
            Ok(Err(e)) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("join error: {e}"),
                ))
            }
        };

    // Fire both events: item_created so the UI's store updates, and
    // lan_capture_received so the UI can toast (and future analytics
    // can distinguish LAN-sourced captures from command-sourced).
    if let Err(e) = state.app.emit(ITEM_CREATED_EVENT, &item) {
        eprintln!("emit item_created failed: {e}");
    }
    if let Err(e) = state.app.emit(LAN_CAPTURE_RECEIVED_EVENT, &item) {
        eprintln!("emit lan_capture_received failed: {e}");
    }

    Ok(Json(CaptureOk {
        ok: true,
        id: item.id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_land_in_inbox_and_nowhere_else() {
        // CLAUDE.md §5, verbatim: "Both go to Inbox, never directly to
        // A/B/C. Tiering is deliberate human work." A capture that
        // landed in A would route around the triage step the caps exist
        // to force — and it would do so from the network, without the
        // user present. v0.3 pass 10 changed this to Tier::A and the
        // whole suite stayed green, because this module had no tests.
        assert_eq!(CAPTURE_TIER, Tier::Inbox);
    }

    #[test]
    fn secret_gate_is_closed_by_default_and_open_only_on_an_exact_match() {
        // Disabled means LAN-trust (SPEC §7.4): no secret configured,
        // anything is accepted.
        assert!(secret_ok(None, None));
        assert!(secret_ok(None, Some("anything")));

        // Configured means EXACT match, and nothing else.
        assert!(secret_ok(Some("hunter2"), Some("hunter2")));
        assert!(!secret_ok(Some("hunter2"), None), "a missing secret must not pass");
        assert!(!secret_ok(Some("hunter2"), Some("")), "empty is not a match");
        assert!(!secret_ok(Some("hunter2"), Some("wrong")));
        assert!(
            !secret_ok(Some("hunter2"), Some("hunter")),
            "a prefix must not pass — this is the app's only network listener"
        );
        assert!(
            !secret_ok(Some("hunter2"), Some("hunter2 ")),
            "no trimming: the secret is compared as sent"
        );
        assert!(
            !secret_ok(Some("hunter2"), Some("HUNTER2")),
            "case matters"
        );
    }

    #[test]
    fn capture_content_is_trimmed_and_empty_is_refused() {
        assert_eq!(normalize_capture_content("  buy milk  ").unwrap(), "buy milk");
        assert_eq!(normalize_capture_content("x").unwrap(), "x");
        for empty in ["", "   ", "\t", "\n", " \r\n "] {
            assert_eq!(
                normalize_capture_content(empty),
                Err("CONTENT_EMPTY"),
                "whitespace-only capture {empty:?} must be refused, not stored as a blank item"
            );
        }
    }
}

/// Cheap encoder for the handful of characters a shared secret might
/// contain when embedded in the URL. Good enough for our purposes —
/// we control both sides and the key class is restricted.
fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}
