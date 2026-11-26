use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use rust_library_system::create_ux_ui_app;
use tower::ServiceExt;

async fn send_get(uri: &str) -> (StatusCode, String) {
    let app = create_ux_ui_app();
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn send_post_form(uri: &str, form_body: &str) -> (StatusCode, String) {
    let app = create_ux_ui_app();
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form_body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn test_sidebar_header_main_layout_and_all_35_menu_routes() {
    // 1. 驗證 Dashboard 與 Sidebar + Header + Main Content 核心版型結構
    let (status, html) = send_get("/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("app-sidebar"));
    assert!(html.contains("app-header"));
    assert!(html.contains("app-main"));
    assert!(html.contains("toggleSidebar()")); // 手機/平板 RWD 側邊欄切換

    // 2. 驗證全部 10 大模組、34 個子選單頁面皆可正常渲染
    let all_routes = vec![
        "/admin/books/list",
        "/admin/books/create",
        "/admin/books/categories",
        "/admin/books/authors",
        "/admin/books/publishers",
        "/admin/books/copies",
        "/admin/readers/list",
        "/admin/readers/create",
        "/admin/readers/cards",
        "/admin/readers/history",
        "/admin/circulation/checkout",
        "/admin/circulation/return",
        "/admin/circulation/renew",
        "/admin/circulation/records",
        "/admin/reservations/list",
        "/admin/reservations/waitlist",
        "/admin/reservations/notifications",
        "/admin/overdue/list",
        "/admin/overdue/fines",
        "/admin/overdue/payments",
        "/admin/procurement/requests",
        "/admin/procurement/orders",
        "/admin/procurement/receiving",
        "/admin/inventory/audit",
        "/admin/inventory/lost",
        "/admin/inventory/scrap",
        "/admin/reports/circulation",
        "/admin/reports/collection",
        "/admin/reports/readers",
        "/admin/reports/overdue",
        "/admin/settings/users",
        "/admin/settings/roles",
        "/admin/settings/rules",
        "/admin/settings/notifications",
        "/admin/settings/audit-logs",
    ];

    for route in all_routes {
        let (code, body) = send_get(route).await;
        assert_eq!(code, StatusCode::OK, "路由 {} 應該正常回傳 200 OK", route);
        assert!(body.contains("Library Management System"));
    }
}

#[tokio::test]
async fn test_list_search_sort_pagination_and_status_badges() {
    // 驗證搜尋關鍵字 q=Rust、排序與分頁
    let (status, html) =
        send_get("/admin/books/list?q=Rust&sort_by=code&order=asc&page=1&page_size=2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Rust 程式設計實戰"));
    assert!(!html.contains("非同步系統架構設計")); // 已被搜尋條件過濾
    assert!(html.contains("status-badge-available")); // 一致狀態色標

    // 驗證分頁切換至第 2 頁（無搜尋過濾，每頁 2 筆，共 6 筆 -> 3 頁）
    let (status_p2, html_p2) =
        send_get("/admin/books/list?sort_by=code&order=asc&page=2&page_size=2").await;
    assert_eq!(status_p2, StatusCode::OK);
    assert!(html_p2.contains("第 2 / 3 頁"));
    assert!(html_p2.contains("資料庫效能調校指南"));
}

#[tokio::test]
async fn test_danger_actions_require_double_confirmation() {
    // 驗證刪除（圖書清單）、報廢（館藏副本）、停權（讀者清單）皆綁定 confirmDangerAction 二次確認
    let (_, books_html) = send_get("/admin/books/list").await;
    assert!(books_html.contains("confirmDangerAction('刪除'"));

    let (_, copies_html) = send_get("/admin/books/copies").await;
    assert!(copies_html.contains("confirmDangerAction('報廢'"));

    let (_, readers_html) = send_get("/admin/readers/list").await;
    assert!(readers_html.contains("confirmDangerAction('停權'"));
}

#[tokio::test]
async fn test_form_validation_and_error_feedback() {
    // 1. 驗證新增圖書頁面具備前端驗證屬性
    let (_, form_html) = send_get("/admin/books/create").await;
    assert!(form_html.contains("needs-validation"));
    assert!(form_html.contains("invalid-feedback"));

    // 2. 送出不合法表單（編號過短、標題為空）應回傳錯誤提示
    let (status_err, err_html) =
        send_post_form("/admin/books/create", "code=12&title=&remark=test").await;
    assert_eq!(status_err, StatusCode::OK);
    assert!(err_html.contains("flash-error"));
    assert!(err_html.contains("表單欄位驗證失敗"));

    // 3. 送出合法表單應回傳成功回饋
    let (status_ok, ok_html) = send_post_form(
        "/admin/books/create",
        "code=ISBN-9789861234&title=Rust%E9%80%B2%E9%9A%8E%E5%AF%A6%E5%8B%99&remark=OK",
    )
    .await;
    assert_eq!(status_ok, StatusCode::OK);
    assert!(ok_html.contains("flash-success"));
    assert!(ok_html.contains("資料儲存成功"));
}

#[tokio::test]
async fn test_barcode_scanner_checkout_and_return_workflow() {
    // 1. 借出頁面預設自動對焦條碼輸入框 (autofocus)
    let (_, checkout_html) = send_get("/admin/circulation/checkout").await;
    assert!(checkout_html.contains("id=\"readerBarcode\""));
    assert!(checkout_html.contains("autofocus"));

    // 2. 掃描讀者證與圖書條碼執行借出
    let (status_out, out_html) = send_post_form(
        "/admin/circulation/checkout",
        "reader_barcode=R-2026001&book_barcode=BK-000101",
    )
    .await;
    assert_eq!(status_out, StatusCode::OK);
    assert!(out_html.contains("條碼借出成功"));
    assert!(out_html.contains("R-2026001"));
    assert!(out_html.contains("BK-000101"));

    // 3. 掃描圖書條碼執行歸還
    let (status_ret, ret_html) =
        send_post_form("/admin/circulation/return", "book_barcode=BK-000101").await;
    assert_eq!(status_ret, StatusCode::OK);
    assert!(ret_html.contains("條碼歸還成功"));
    assert!(ret_html.contains("BK-000101"));
}
