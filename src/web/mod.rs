pub mod admin;
pub mod api;
pub mod pages;
pub mod vm;

use axum::extract::Path;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use crate::ws;

static STATIC_FILES: &[(&str, &str, &str)] = &[
    ("app.css", "text/css; charset=utf-8", include_str!("../../static/app.css")),
    ("common.js", "text/javascript; charset=utf-8", include_str!("../../static/common.js")),
    ("arena.js", "text/javascript; charset=utf-8", include_str!("../../static/arena.js")),
    ("team.js", "text/javascript; charset=utf-8", include_str!("../../static/team.js")),
    ("wait.js", "text/javascript; charset=utf-8", include_str!("../../static/wait.js")),
    ("admin.js", "text/javascript; charset=utf-8", include_str!("../../static/admin.js")),
    ("chat.js", "text/javascript; charset=utf-8", include_str!("../../static/chat.js")),
    ("favicon.svg", "image/svg+xml", include_str!("../../static/favicon.svg")),
];

async fn static_file(Path(name): Path<String>) -> Response {
    match STATIC_FILES.iter().find(|f| f.0 == name) {
        Some((_, ct, body)) => ([(CONTENT_TYPE, *ct), (CACHE_CONTROL, "no-cache")], *body).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // pages
        .route("/", get(pages::landing))
        .route("/wait", get(pages::wait))
        .route("/team", get(pages::team_page))
        .route("/team/frag/{name}", get(pages::team_fragment_handler))
        .route("/arena", get(pages::arena))
        .route("/arena/frag/team/{id}", get(pages::arena_team_fragment))
        .route("/arena/frag/edge/{attacker}", get(pages::arena_edge_fragment))
        .route("/admin", get(admin::dashboard))
        .route("/admin/login", get(admin::login_page))
        .route("/admin/frag/{name}", get(admin::fragment_handler))
        .route("/admin/challenges", get(admin::challenges_page))
        .route("/admin/challenges/{id}", get(admin::challenge_page))
        .route("/admin/export/scores.csv", get(admin::export_scores_csv))
        .route("/admin/export/session.json", get(admin::export_session_json))
        .route("/static/{name}", get(static_file))
        .route("/ws", get(ws::handler))
        // shared / student API
        .route("/api/join", post(api::join))
        .route("/api/logout", post(api::logout))
        .route("/api/snapshot", get(api::snapshot))
        .route("/api/chat/send", post(api::chat_send))
        .route("/api/chat/history", get(api::chat_history))
        .route("/api/team/status", post(api::team_status))
        .route("/api/team/prep/save", post(api::prep_save))
        .route("/api/team/prep/ready", post(api::prep_ready))
        .route("/api/team/report/save", post(api::report_save))
        .route("/api/team/report/submit", post(api::report_submit))
        .route("/api/team/report/delete", post(api::report_delete))
        .route("/api/team/report/revise", post(api::report_revise))
        .route("/api/team/defense/report", post(api::defense_report))
        .route("/api/team/defense/rebuttal", post(api::rebuttal))
        .route("/api/team/patch/save", post(api::patch_save))
        .route("/api/team/patch/submit", post(api::patch_submit))
        // instructor API
        .route("/api/admin/login", post(admin::login))
        .route("/api/admin/session/rename", post(admin::session_rename))
        .route("/api/admin/session/reset", post(admin::session_reset))
        .route("/api/admin/roster/add", post(admin::roster_add))
        .route("/api/admin/roster/set", post(admin::roster_set))
        .route("/api/admin/participant/release", post(admin::release_device))
        .route("/api/admin/teams/random", post(admin::teams_random))
        .route("/api/admin/teams/create", post(admin::teams_create))
        .route("/api/admin/teams/rename", post(admin::teams_rename))
        .route("/api/admin/teams/delete", post(admin::teams_delete))
        .route("/api/admin/teams/member", post(admin::teams_set_member))
        .route("/api/admin/teams/remove-member", post(admin::teams_remove_member))
        .route("/api/admin/teams/confirm", post(admin::teams_confirm))
        .route("/api/admin/teams/discard", post(admin::teams_discard))
        .route("/api/admin/teams/place-late", post(admin::teams_place_late))
        .route("/api/admin/round/create", post(admin::round_create))
        .route("/api/admin/round/pairing", post(admin::round_pairing))
        .route("/api/admin/round/shuffle", post(admin::round_shuffle))
        .route("/api/admin/round/cancel", post(admin::round_cancel))
        .route("/api/admin/round/start", post(admin::round_start))
        .route("/api/admin/round/advance", post(admin::round_advance))
        .route("/api/admin/round/pause", post(admin::round_pause))
        .route("/api/admin/round/resume", post(admin::round_resume))
        .route("/api/admin/round/extend", post(admin::round_extend))
        .route("/api/admin/round/finalize", post(admin::round_finalize))
        .route("/api/admin/round/discussion", post(admin::round_discussion))
        .route("/api/admin/review/report", post(admin::review_report))
        .route("/api/admin/review/patch", post(admin::review_patch))
        .route("/api/admin/score/adjust", post(admin::score_adjust))
        .route("/api/admin/chat/delete", post(admin::chat_delete))
        .route("/api/admin/chat/mute", post(admin::chat_mute))
        .route("/api/admin/chat/lobby-lock", post(admin::chat_lobby_lock))
        .with_state(state)
}
