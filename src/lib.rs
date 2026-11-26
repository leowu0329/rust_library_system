pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

use axum::{
    response::Redirect,
    routing::{get, post},
    Router,
};
use mongodb::Database;
use std::sync::Arc;
use tower_sessions::{MemoryStore, SessionManagerLayer};

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

/// 建立完整的 Axum 應用程式實例（供正式啟動與整合測試共用）
pub fn create_app(db: Database) -> Router {
    let state = Arc::new(AppState { db });
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    Router::new()
        .route("/", get(|| async { Redirect::to("/admin/procurement") }))
        .route("/switch-mode", post(handlers::auth_handler::switch_view_mode))
        .route("/switch-role", post(handlers::auth_handler::switch_role))
        .route(
            "/admin/procurement",
            get(handlers::procurement_handler::list_procurements),
        )
        .route(
            "/admin/procurement/create",
            post(handlers::procurement_handler::create_procurement),
        )
        .route(
            "/admin/procurement/:id/approve",
            post(handlers::procurement_handler::approve_procurement),
        )
        .route(
            "/admin/procurement/:id/reject",
            post(handlers::procurement_handler::reject_procurement),
        )
        .route(
            "/admin/procurement/:id/receive",
            post(handlers::procurement_handler::receive_procurement),
        )
        .layer(session_layer)
        .with_state(state)
}
