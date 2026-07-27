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
    if let Some(expected) = &state.shared_secret {
        let provided = q.s.as_deref().or_else(|| {
            headers
                .get("x-bay-secret")
                .and_then(|v| v.to_str().ok())
        });
        if provided != Some(expected.as_str()) {
            return Err((StatusCode::UNAUTHORIZED, "bad secret".into()));
        }
    }

    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "CONTENT_EMPTY".into()));
    }

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
                Tier::Inbox,
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
