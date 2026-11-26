use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use bson::doc;
use http_body_util::BodyExt;
use mongodb::{options::ClientOptions, Client, Database};
use rust_library_system::{
    create_app,
    models::{
        book::{Book, BookCopy, CopyStatus},
        procurement::{Procurement, ProcurementStatus},
    },
};
use tower::ServiceExt; // 提供 oneshot() 方法

/// 建立獨立的測試用 MongoDB 連線（名稱控制在 38 bytes 以內以符合 Atlas 規範）
async fn setup_test_db() -> Database {
    dotenvy::dotenv().ok();
    let mongo_uri = std::env::var("MONGODB_URI").expect("請先在 .env 設定 MONGODB_URI");
    let client_options = ClientOptions::parse(&mongo_uri)
        .await
        .expect("無效的 MongoDB 連線字串");
    let client = Client::with_options(client_options).expect("建立 MongoDB Client 失敗");

    client
        .database("admin")
        .run_command(doc! { "ping": 1 })
        .await
        .expect("MongoDB Cloud 連線或帳密驗證失敗");

    // 截取 ObjectId 前 8 碼，總長度為 17 bytes（遠低於 Atlas 38 bytes 上限）
    let short_id = &bson::oid::ObjectId::new().to_hex()[..8];
    let test_db_name = format!("lib_test_{}", short_id);
    let db = client.database(&test_db_name);

    db.drop().await.ok();
    db
}

/// 輔助函式：從 HTTP Response Header 擷取 Session Cookie
fn extract_session_cookie(res: &axum::response::Response) -> Option<String> {
    res.headers()
        .get(header::SET_COOKIE)
        .and_then(|val| val.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
}

#[tokio::test]
async fn test_full_procurement_and_rbac_workflow() {
    let db = setup_test_db().await;
    let app = create_app(db.clone());

    // =========================================================================
    // 階段 1：採購人員建立新書採購申請單 (POST /admin/procurement/create)
    // =========================================================================
    let form_body = "isbn=9789864349999&title=Rust%E9%AB%98%E4%BD%B5%E7%99%BC%E5%AF%A6%E6%88%B0&author=%E5%90%B3%E6%9E%B6%E6%A7%8B&publisher=%E6%B7%B1%E6%99%BA%E6%95%B8%E4%BD%8D&quantity=3&unit_price=650";

    let create_req = Request::builder()
        .method("POST")
        .uri("/admin/procurement/create")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form_body))
        .unwrap();

    let create_res = app.clone().oneshot(create_req).await.unwrap();
    let status = create_res.status();
    if status != StatusCode::SEE_OTHER {
        let err_bytes = create_res.into_body().collect().await.unwrap().to_bytes();
        let err_msg = String::from_utf8_lossy(&err_bytes);
        panic!("建立採購單失敗，狀態碼：{}，伺服器錯誤訊息：{}", status, err_msg);
    }

    // 驗證資料庫已寫入 Pending 狀態的採購單
    let proc_coll = db.collection::<Procurement>("procurements");
    let created_order = proc_coll
        .find_one(doc! { "isbn": "9789864349999" })
        .await
        .unwrap()
        .expect("資料庫中應存在剛建立的採購單");

    assert_eq!(created_order.status, ProcurementStatus::Pending);
    assert_eq!(created_order.quantity, 3);
    let order_id = created_order.id.unwrap().to_hex();

    // =========================================================================
    // 階段 2：RBAC 權限驗證 - 採購人員嘗試自行核准採購單 (預期回傳 403 Forbidden)
    // =========================================================================
    let unauthorized_approve_req = Request::builder()
        .method("POST")
        .uri(format!("/admin/procurement/{}/approve", order_id))
        .body(Body::empty())
        .unwrap();

    let unauthorized_res = app.clone().oneshot(unauthorized_approve_req).await.unwrap();
    assert_eq!(
        unauthorized_res.status(),
        StatusCode::FORBIDDEN,
        "採購人員無權審核採購單，系統必須回傳 403 Forbidden"
    );

    // =========================================================================
    // 階段 3：切換為「館長 (librarian_admin)」角色並核准採購單
    // =========================================================================
    let switch_to_admin_req = Request::builder()
        .method("POST")
        .uri("/switch-role")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("role=librarian_admin"))
        .unwrap();

    let switch_res = app.clone().oneshot(switch_to_admin_req).await.unwrap();
    assert_eq!(switch_res.status(), StatusCode::SEE_OTHER);
    let admin_cookie = extract_session_cookie(&switch_res).expect("應取得館長 Session Cookie");

    // 攜帶館長 Cookie 執行核准
    let approve_req = Request::builder()
        .method("POST")
        .uri(format!("/admin/procurement/{}/approve", order_id))
        .header(header::COOKIE, &admin_cookie)
        .body(Body::empty())
        .unwrap();

    let approve_res = app.clone().oneshot(approve_req).await.unwrap();
    assert_eq!(approve_res.status(), StatusCode::SEE_OTHER);

    // 驗證資料庫狀態已變更為 Approved 且記錄審核人
    let approved_order = proc_coll
        .find_one(doc! { "_id": bson::oid::ObjectId::parse_str(&order_id).unwrap() })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(approved_order.status, ProcurementStatus::Approved);
    assert_eq!(approved_order.approver_name.as_deref(), Some("陳館長"));

    // =========================================================================
    // 階段 4：執行新書驗收入庫，驗證書目庫存更新與實體條碼自動生成
    // =========================================================================
    let receive_req = Request::builder()
        .method("POST")
        .uri(format!("/admin/procurement/{}/receive", order_id))
        .header(header::COOKIE, &admin_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("location=%E4%BA%8C%E6%A8%93%E9%9B%BB%E8%85%A6%E8%B3%87%E8%A8%8A%E6%9B%B8%E5%BA%AB"))
        .unwrap();

    let receive_res = app.clone().oneshot(receive_req).await.unwrap();
    assert_eq!(receive_res.status(), StatusCode::SEE_OTHER);

    // 4-1. 檢查採購單最終狀態與生成的條碼列表
    let final_order = proc_coll
        .find_one(doc! { "_id": bson::oid::ObjectId::parse_str(&order_id).unwrap() })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_order.status, ProcurementStatus::Received);
    assert_eq!(final_order.generated_barcodes.len(), 3);
    assert_eq!(final_order.generated_barcodes[0], "LIB-9789864349999-0001");
    assert_eq!(final_order.generated_barcodes[2], "LIB-9789864349999-0003");

    // 4-2. 檢查 books 書目主檔總複本數與可借閱數皆為 3
    let book_coll = db.collection::<Book>("books");
    let book = book_coll
        .find_one(doc! { "isbn": "9789864349999" })
        .await
        .unwrap()
        .expect("驗收後應自動建立 books 書目主檔");
    assert_eq!(book.total_copies, 3);
    assert_eq!(book.available_copies, 3);

    // 4-3. 檢查 book_copies 實體複本集合是否確實產生 3 筆可借閱館藏
    let copy_coll = db.collection::<BookCopy>("book_copies");
    let copy_count = copy_coll
        .count_documents(doc! { "isbn": "9789864349999" })
        .await
        .unwrap();
    assert_eq!(copy_count, 3);

    let first_copy = copy_coll
        .find_one(doc! { "barcode": "LIB-9789864349999-0001" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_copy.status, CopyStatus::Available);
    assert_eq!(first_copy.location, "二樓電腦資訊書庫");

    // =========================================================================
    // 階段 5：驗證前端 HTML 頁面能正確渲染出這筆採購單與條碼
    // =========================================================================
    let page_req = Request::builder()
        .method("GET")
        .uri("/admin/procurement")
        .header(header::COOKIE, &admin_cookie)
        .body(Body::empty())
        .unwrap();

    let page_res = app.oneshot(page_req).await.unwrap();
    assert_eq!(page_res.status(), StatusCode::OK);

    let body_bytes = page_res.into_body().collect().await.unwrap().to_bytes();
    let html_text = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert!(html_text.contains("Rust高併發實戰"), "HTML 頁面應顯示書名");
    assert!(
        html_text.contains("LIB-9789864349999-0001"),
        "HTML 頁面應顯示生成的館藏條碼"
    );

    // 測試完成，自動刪除暫時測試資料庫
    db.drop().await.expect("清理測試資料庫失敗");
}

