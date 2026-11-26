use askama::Template;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Form, Router,
};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AdminListRow {
    pub code: String,
    pub title: String,
    pub subtitle: String,
    pub status_key: String,
    pub status_label: String,
    pub status_css: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    pub q: Option<String>,
    pub sort_by: Option<String>,
    pub order: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ActionFormPayload {
    pub code: Option<String>,
    pub title: Option<String>,
    pub remark: Option<String>,
    pub reader_barcode: Option<String>,
    pub book_barcode: Option<String>,
}

#[derive(Template)]
#[template(path = "backend/admin_workspace.html")]
pub struct AdminWorkspaceTemplate {
    pub active_module: String,
    pub active_page: String,
    pub module_label: String,
    pub page_title: String,
    pub current_path: String,
    pub view_mode: String, // "dashboard" | "list" | "form" | "scanner"
    pub danger_action_label: String,
    pub search_query: String,
    pub sort_by: String,
    pub sort_order: String,
    pub page: usize,
    pub page_size: usize,
    pub total_items: usize,
    pub total_pages: usize,
    pub prev_page: usize,
    pub next_page: usize,
    pub rows: Vec<AdminListRow>,
    pub flash_success: Option<String>,
    pub flash_error: Option<String>,
}

/// 統一狀態顏色規範對映函式
pub fn map_unified_status(status_key: &str) -> (&'static str, &'static str) {
    match status_key {
        "available" => ("可借閱 / 正常", "status-badge-available"),
        "borrowed" => ("借出中 / 進行中", "status-badge-borrowed"),
        "reserved" => ("預約保留 / 待審核", "status-badge-reserved"),
        "overdue" => ("逾期 / 異常停權", "status-badge-overdue"),
        "scrapped" => ("已報廢 / 封存", "status-badge-scrapped"),
        _ => ("處理中", "status-badge-borrowed"),
    }
}

/// 解析後台目錄對應之模組名稱、中文標題、視圖模式與二次確認動作標籤
fn resolve_route_meta(module: &str, subpage: &str) -> Option<(String, String, String, String, String)> {
    // 回傳: (active_page, module_label, page_title, view_mode, danger_action_label)
    let meta = match (module, subpage) {
        ("dashboard", _) => ("dashboard", "總覽", "Dashboard 儀表板", "dashboard", "刪除"),
        // 圖書管理
        ("books", "list") => ("books_list", "圖書管理", "圖書清單", "list", "刪除"),
        ("books", "create") => ("books_create", "圖書管理", "新增圖書", "form", "刪除"),
        ("books", "categories") => ("books_categories", "圖書管理", "圖書分類", "list", "刪除"),
        ("books", "authors") => ("books_authors", "圖書管理", "作者管理", "list", "刪除"),
        ("books", "publishers") => ("books_publishers", "圖書管理", "出版社管理", "list", "刪除"),
        ("books", "copies") => ("books_copies", "圖書管理", "館藏副本", "list", "報廢"),
        // 讀者管理
        ("readers", "list") => ("readers_list", "讀者管理", "讀者清單", "list", "停權"),
        ("readers", "create") => ("readers_create", "讀者管理", "新增讀者", "form", "停權"),
        ("readers", "cards") => ("readers_cards", "讀者管理", "讀者證", "list", "停權"),
        ("readers", "history") => ("readers_history", "讀者管理", "讀者借閱歷史", "list", "刪除"),
        // 借閱管理
        ("circulation", "checkout") => ("circulation_checkout", "借閱管理", "圖書借出", "scanner", "取消"),
        ("circulation", "return") => ("circulation_return", "借閱管理", "圖書歸還", "scanner", "取消"),
        ("circulation", "renew") => ("circulation_renew", "借閱管理", "續借管理", "list", "駁回"),
        ("circulation", "records") => ("circulation_records", "借閱管理", "借閱紀錄", "list", "刪除"),
        // 預約管理
        ("reservations", "list") => ("reservations_list", "預約管理", "預約清單", "list", "取消"),
        ("reservations", "waitlist") => ("reservations_waitlist", "預約管理", "候補清單", "list", "移除"),
        ("reservations", "notifications") => ("reservations_notifications", "預約管理", "預約通知", "list", "刪除"),
        // 逾期管理
        ("overdue", "list") => ("overdue_list", "逾期管理", "逾期清單", "list", "停權"),
        ("overdue", "fines") => ("overdue_fines", "逾期管理", "罰款管理", "list", "刪除"),
        ("overdue", "payments") => ("overdue_payments", "逾期管理", "繳費紀錄", "list", "作廢"),
        // 採購管理
        ("procurement", "requests") => ("procurement_requests", "採購管理", "採購申請", "list", "退回"),
        ("procurement", "orders") => ("procurement_orders", "採購管理", "訂單管理", "list", "取消"),
        ("procurement", "receiving") => ("procurement_receiving", "採購管理", "入庫驗收", "form", "退貨"),
        // 館藏盤點
        ("inventory", "audit") => ("inventory_audit", "館藏盤點", "盤點作業", "list", "刪除"),
        ("inventory", "lost") => ("inventory_lost", "館藏盤點", "遺失管理", "list", "報廢"),
        ("inventory", "scrap") => ("inventory_scrap", "館藏盤點", "報廢管理", "list", "報廢"),
        // 報表分析
        ("reports", "circulation") => ("reports_circulation", "報表分析", "借閱統計", "list", "刪除"),
        ("reports", "collection") => ("reports_collection", "報表分析", "館藏統計", "list", "刪除"),
        ("reports", "readers") => ("reports_readers", "報表分析", "讀者統計", "list", "刪除"),
        ("reports", "overdue") => ("reports_overdue", "報表分析", "逾期統計", "list", "刪除"),
        // 系統設定
        ("settings", "users") => ("settings_users", "系統設定", "使用者管理", "list", "停權"),
        ("settings", "roles") => ("settings_roles", "系統設定", "角色權限", "list", "刪除"),
        ("settings", "rules") => ("settings_rules", "系統設定", "借閱規則", "form", "刪除"),
        ("settings", "notifications") => ("settings_notifications", "系統設定", "通知設定", "form", "刪除"),
        ("settings", "audit-logs") => ("settings_audit_logs", "系統設定", "操作紀錄", "list", "封存"),
        _ => return None,
    };

    Some((
        meta.0.to_string(),
        meta.1.to_string(),
        meta.2.to_string(),
        meta.3.to_string(),
        meta.4.to_string(),
    ))
}

/// 產生示範清單資料並執行搜尋、排序與分頁
fn query_paginated_rows(module: &str, params: &ListQueryParams) -> (Vec<AdminListRow>, usize, usize, usize) {
    let raw_items = vec![
        ("BK-000101", "Rust 程式設計實戰", "資訊科學 / ISBN 978-986-001", "available", "2026-11-20 10:15"),
        ("BK-000102", "非同步系統架構設計", "軟體工程 / ISBN 978-986-002", "borrowed", "2026-11-21 14:30"),
        ("BK-000103", "資料庫效能調校指南", "資料庫 / ISBN 978-986-003", "reserved", "2026-11-22 09:45"),
        ("BK-000104", "分散式共識演算法", "系統架構 / ISBN 978-986-004", "overdue", "2026-11-23 16:20"),
        ("BK-000105", "舊版網路協定手冊", "網路通訊 / ISBN 978-986-005", "scrapped", "2026-11-24 11:05"),
        ("BK-000106", "人機介面與體驗設計", "UX/UI 設計 / ISBN 978-986-006", "available", "2026-11-25 13:50"),
    ];

    let mut rows: Vec<AdminListRow> = raw_items
        .into_iter()
        .map(|(code, title, subtitle, status_key, updated_at)| {
            let (status_label, status_css) = map_unified_status(status_key);
            AdminListRow {
                code: format!("{}-{}", module.to_uppercase(), code),
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                status_key: status_key.to_string(),
                status_label: status_label.to_string(),
                status_css: status_css.to_string(),
                updated_at: updated_at.to_string(),
            }
        })
        .collect();

    // 1. 關鍵字搜尋過濾
    if let Some(ref q) = params.q {
        let keyword = q.trim().to_lowercase();
        if !keyword.is_empty() {
            rows.retain(|r| {
                r.code.to_lowercase().contains(&keyword)
                    || r.title.to_lowercase().contains(&keyword)
                    || r.subtitle.to_lowercase().contains(&keyword)
                    || r.status_label.to_lowercase().contains(&keyword)
            });
        }
    }

    // 2. 多欄位升/降冪排序
    let sort_by = params.sort_by.as_deref().unwrap_or("code");
    let desc = params.order.as_deref() == Some("desc");
    rows.sort_by(|a, b| {
        let ord = match sort_by {
            "title" => a.title.cmp(&b.title),
            "status" => a.status_key.cmp(&b.status_key),
            "updated_at" => a.updated_at.cmp(&b.updated_at),
            _ => a.code.cmp(&b.code),
        };
        if desc { ord.reverse() } else { ord }
    });

    // 3. 分頁計算
    let page_size = params.page_size.unwrap_or(5).max(1);
    let total_items = rows.len();
    let total_pages = total_items.div_ceil(page_size).max(1);
    let current_page = params.page.unwrap_or(1).clamp(1, total_pages);
    let start = (current_page - 1) * page_size;
    let paged_rows = rows.into_iter().skip(start).take(page_size).collect();

    (paged_rows, total_items, total_pages, current_page)
}

fn build_workspace_response(
    module: &str,
    subpage: &str,
    current_path: &str,
    params: &ListQueryParams,
    flash_success: Option<String>,
    flash_error: Option<String>,
) -> impl IntoResponse {
    let Some((active_page, module_label, page_title, view_mode, danger_action_label)) =
        resolve_route_meta(module, subpage)
    else {
        return (StatusCode::NOT_FOUND, Html("<h1>404 - 找不到指定的後台功能頁面</h1>".to_string()));
    };

    let page_size = params.page_size.unwrap_or(5).max(1);
    let (rows, total_items, total_pages, page) = query_paginated_rows(module, params);
    let prev_page = page.saturating_sub(1).max(1);
    let next_page = (page + 1).min(total_pages);

    let tpl = AdminWorkspaceTemplate {
        active_module: module.to_string(),
        active_page,
        module_label,
        page_title,
        current_path: current_path.to_string(),
        view_mode,
        danger_action_label,
        search_query: params.q.clone().unwrap_or_default(),
        sort_by: params.sort_by.clone().unwrap_or_else(|| "code".to_string()),
        sort_order: params.order.clone().unwrap_or_else(|| "asc".to_string()),
        page,
        page_size,
        total_items,
        total_pages,
        prev_page,
        next_page,
        rows,
        flash_success,
        flash_error,
    };

    match tpl.render() {
        Ok(html) => (StatusCode::OK, Html(html)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("模板渲染失敗: {}", e)),
        ),
    }
}

pub async fn dashboard_page(Query(params): Query<ListQueryParams>) -> impl IntoResponse {
    build_workspace_response("dashboard", "index", "/admin/dashboard", &params, None, None)
}

pub async fn module_subpage_get(
    Path((module, subpage)): Path<(String, String)>,
    Query(params): Query<ListQueryParams>,
) -> impl IntoResponse {
    let path = format!("/admin/{}/{}", module, subpage);
    build_workspace_response(&module, &subpage, &path, &params, None, None)
}

pub async fn module_subpage_post(
    Path((module, subpage)): Path<(String, String)>,
    Form(payload): Form<ActionFormPayload>,
) -> impl IntoResponse {
    let path = format!("/admin/{}/{}", module, subpage);
    let params = ListQueryParams::default();

    // 條碼掃描驗證與回饋（圖書借出 / 圖書歸還）
    if module == "circulation" && (subpage == "checkout" || subpage == "return") {
        let book_code = payload.book_barcode.unwrap_or_default().trim().to_string();
        let reader_code = payload.reader_barcode.unwrap_or_default().trim().to_string();

        if book_code.is_empty() || (subpage == "checkout" && reader_code.is_empty()) {
            return build_workspace_response(
                &module,
                &subpage,
                &path,
                &params,
                None,
                Some("條碼驗證失敗：請完整掃描讀者證條碼與圖書副本條碼。".to_string()),
            );
        }

        let action_desc = if subpage == "checkout" {
            format!("條碼借出成功！讀者 [{}] 已借出圖書副本 [{}]。", reader_code, book_code)
        } else {
            format!("條碼歸還成功！圖書副本 [{}] 已完成入館驗收。", book_code)
        };

        return build_workspace_response(&module, &subpage, &path, &params, Some(action_desc), None);
    }

    // 一般表單欄位驗證
    let code = payload.code.unwrap_or_default().trim().to_string();
    let title = payload.title.unwrap_or_default().trim().to_string();
    if code.len() < 4 || title.is_empty() {
        return build_workspace_response(
            &module,
            &subpage,
            &path,
            &params,
            None,
            Some("表單欄位驗證失敗：識別編號至少需 4 個字元且名稱不可為空。".to_string()),
        );
    }

    build_workspace_response(
        &module,
        &subpage,
        &path,
        &params,
        Some(format!("資料儲存成功：[{}] {}", code, title)),
        None,
    )
}

pub fn admin_ui_routes() -> Router {
    Router::new()
        .route("/admin/dashboard", get(dashboard_page))
        .route(
            "/admin/:module/:subpage",
            get(module_subpage_get).post(module_subpage_post),
        )
}
