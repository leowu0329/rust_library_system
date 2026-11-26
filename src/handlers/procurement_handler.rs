use crate::error::AppError;
use crate::middleware::rbac::{require_roles, CurrentUser};
use crate::models::{
    procurement::{CreateProcurementForm, Procurement, ProcurementStatus, ReceiveProcurementForm},
    user::{Role, ViewMode},
};
use crate::services::procurement_service::ProcurementService;
use crate::AppState;
use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use std::sync::Arc;

#[derive(Template)]
#[template(path = "backend/procurement.html")]
pub struct ProcurementPageTemplate {
    pub user: CurrentUser,
    pub orders: Vec<Procurement>,
}

/// GET /admin/procurement：渲染採購與驗收入庫管理頁面
pub async fn list_procurements(
    user: CurrentUser,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, Response> {
    // 允許具備後台權限者進入瀏覽，或前台展示無權限提示（此處依角色顯示對應按鈕）
    let orders = ProcurementService::list_all(&state.db)
        .await
        .map_err(|e| e.into_response())?;

    Ok(ProcurementPageTemplate { user, orders })
}

/// POST /admin/procurement/create：建立採購單
pub async fn create_procurement(
    user: CurrentUser,
    State(state): State<Arc<AppState>>,
    Form(form): Form<CreateProcurementForm>,
) -> Result<impl IntoResponse, AppError> {
    require_roles(
        &user,
        &[Role::Procurement, Role::LibrarianAdmin, Role::SuperAdmin],
    )
    .map_err(|_| AppError::Forbidden("僅採購人員、館長或超管可建立採購單".to_string()))?;

    ProcurementService::create_order(&state.db, form, &user.username).await?;
    Ok(Redirect::to("/admin/procurement"))
}

/// POST /admin/procurement/:id/approve：館長審核通過
pub async fn approve_procurement(
    user: CurrentUser,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    require_roles(&user, &[Role::LibrarianAdmin, Role::SuperAdmin])
        .map_err(|_| AppError::Forbidden("僅館長或超級管理員可審核採購單".to_string()))?;

    ProcurementService::review_order(&state.db, &id, true, &user.username).await?;
    Ok(Redirect::to("/admin/procurement"))
}

/// POST /admin/procurement/:id/reject：館長駁回採購單
pub async fn reject_procurement(
    user: CurrentUser,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    require_roles(&user, &[Role::LibrarianAdmin, Role::SuperAdmin])
        .map_err(|_| AppError::Forbidden("僅館長或超級管理員可駁回採購單".to_string()))?;

    ProcurementService::review_order(&state.db, &id, false, &user.username).await?;
    Ok(Redirect::to("/admin/procurement"))
}

/// POST /admin/procurement/:id/receive：驗收入庫並自動生成條碼
pub async fn receive_procurement(
    user: CurrentUser,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Form(form): Form<ReceiveProcurementForm>,
) -> Result<impl IntoResponse, AppError> {
    require_roles(
        &user,
        &[Role::Procurement, Role::LibrarianAdmin, Role::SuperAdmin],
    )
    .map_err(|_| AppError::Forbidden("僅採購人員、館長或超管可執行驗收入庫".to_string()))?;

    ProcurementService::receive_and_catalog(&state.db, &id, &form.location).await?;
    Ok(Redirect::to("/admin/procurement"))
}
