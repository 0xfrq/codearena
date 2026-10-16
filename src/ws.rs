//! WebSocket endpoint. On connect (and after every structural change, lag or explicit resync)
//! the server sends a fresh viewer-specific snapshot; otherwise it forwards only the incremental
//! events the viewer is entitled to see.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::auth;
use crate::error::AppError;
use crate::events::Viewer;
use crate::state::AppState;
use crate::util::now_ms;
use crate::views;

#[derive(Deserialize)]
pub struct WsQuery {
    #[serde(default)]
    view: String,
}

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WsQuery>,
) -> Response {
    let view = match q.view.as_str() {
        "arena" | "team" | "wait" | "admin" => q.view,
        _ => "arena".to_string(),
    };
    // The page decides which login it acts as (a browser may hold both an instructor and a student cookie).
    let viewer = match auth::viewer_preferring(&state, &headers, auth::Prefer::parse(&view)).await {
        Ok(Some(v)) => v,
        Ok(None) => return AppError::Unauthorized.into_response(),
        Err(e) => return e.into_response(),
    };
    ws.on_upgrade(move |socket| run(socket, state, viewer, view))
}

async fn send_snapshot(socket: &mut futures_util::stream::SplitSink<WebSocket, Message>, state: &AppState, viewer: &Viewer, view: &str) -> bool {
    // Version is read *before* building the snapshot: any event emitted while building has a
    // higher version and will still be applied by the client.
    let v = state.events.version();
    match views::snapshot_for(state, viewer, view).await {
        Ok(data) => {
            let msg = json!({ "t": "snapshot", "v": v, "now": now_ms(), "data": data });
            socket.send(Message::Text(msg.to_string().into())).await.is_ok()
        }
        Err(e) => {
            tracing::error!("snapshot failed: {e}");
            let msg = json!({ "t": "error", "message": e.message() });
            socket.send(Message::Text(msg.to_string().into())).await.is_ok()
        }
    }
}

async fn refresh_viewer(state: &AppState, viewer: &mut Viewer) {
    if let Viewer::Student { nim, team_id } = viewer {
        if let Ok(s) = crate::db::current_session(&state.db).await {
            if let Ok(t) = auth::confirmed_team_of(&state.db, s.id, nim).await {
                *team_id = t;
            }
        }
    }
}

async fn run(socket: WebSocket, state: AppState, mut viewer: Viewer, view: String) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.events.subscribe();
    let nim = match &viewer {
        Viewer::Student { nim, .. } => Some(nim.clone()),
        Viewer::Instructor => None,
    };
    if let Some(n) = &nim {
        if state.presence.connect(n) {
            touch(&state, n).await;
            views::publish_presence(&state, n, true).await;
        }
    }
    let mut alive = send_snapshot(&mut sink, &state, &viewer, &view).await;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    while alive {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    if ev.is_structural() {
                        refresh_viewer(&state, &mut viewer).await;
                        alive = send_snapshot(&mut sink, &state, &viewer, &view).await;
                    } else if ev.audience.includes(&viewer) {
                        let msg = json!({ "t": "ev", "v": ev.v, "kind": ev.kind, "data": ev.data });
                        alive = sink.send(Message::Text(msg.to_string().into())).await.is_ok();
                        if ev.kind == "force_logout" {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    // Missed events: never try to patch state incrementally — resend the snapshot.
                    alive = send_snapshot(&mut sink, &state, &viewer, &view).await;
                }
                Err(RecvError::Closed) => break,
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    let t = t.as_str();
                    if t == "resync" {
                        refresh_viewer(&state, &mut viewer).await;
                        alive = send_snapshot(&mut sink, &state, &viewer, &view).await;
                    } else if t == "ping" {
                        alive = sink.send(Message::Text(json!({ "t": "pong", "now": now_ms() }).to_string().into())).await.is_ok();
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            _ = heartbeat.tick() => {
                alive = sink.send(Message::Text(json!({ "t": "hb", "now": now_ms() }).to_string().into())).await.is_ok();
            }
        }
    }
    if let Some(n) = &nim {
        if state.presence.disconnect(n) {
            touch(&state, n).await;
            views::publish_presence(&state, n, false).await;
        }
    }
}

async fn touch(state: &AppState, nim: &str) {
    if let Ok(s) = crate::db::current_session(&state.db).await {
        let _ = sqlx::query("UPDATE participants SET last_seen = ? WHERE session_id = ? AND nim = ?")
            .bind(now_ms())
            .bind(s.id)
            .bind(nim)
            .execute(&state.db)
            .await;
    }
}
