use chrono::{Duration, Utc};
use mongodb::{
    bson::{doc, oid::ObjectId},
    Client,
};
use rust_library_system::{
    models::database_schema::*,
    services::circulation_core_service::{CirculationCoreService, DbDesignError},
};
use std::sync::Arc;

async fn setup_test_db(db_name: &str) -> mongodb::Database {
    let uri = std::env::var("MONGODB_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".to_string());
    let client = Client::with_uri_str(&uri)
        .await
        .expect("Failed to connect to MongoDB for testing");
    let db = client.database(db_name);
    db.drop().await.expect("Failed to clean test database");
    db
}

#[tokio::test]
async fn test_database_design_full_workflow() {
    let db = setup_test_db("rust_library_db_design_test").await;
    let service = Arc::new(CirculationCoreService::new(db.clone()));

    // =========================================================================
    // 步驟 1：驗證「原則 A：書籍與副本分離」
    // =========================================================================
    let publisher_id = ObjectId::new();
    let category_id = ObjectId::new();
    let author_id = ObjectId::new();
    let branch_id = ObjectId::new();
    let shelf_id = ObjectId::new();
    let operator_id = ObjectId::new();

    let book = Book {
        id: None,
        isbn: "978-986-434-999-1".to_string(),
        title: "Rust 系統程式設計與資料庫架構".to_string(),
        publisher_id,
        category_id,
        author_ids: vec![author_id],
        created_at: Utc::now(),
    };
    let book_id = db
        .collection::<Book>("books")
        .insert_one(&book)
        .await
        .unwrap()
        .inserted_id
        .as_object_id()
        .unwrap();

    let copy_1 = BookCopy {
        id: None,
        book_id,
        barcode: "BC-2026-0001".to_string(),
        branch_id,
        shelf_id,
        status: CopyStatus::Available,
    };
    let copy_2 = BookCopy {
        id: None,
        book_id,
        barcode: "BC-2026-0002".to_string(),
        branch_id,
        shelf_id,
        status: CopyStatus::Available,
    };

    let copy_1_id = db
        .collection::<BookCopy>("book_copies")
        .insert_one(&copy_1)
        .await
        .unwrap()
        .inserted_id
        .as_object_id()
        .unwrap();
    let copy_2_id = db
        .collection::<BookCopy>("book_copies")
        .insert_one(&copy_2)
        .await
        .unwrap()
        .inserted_id
        .as_object_id()
        .unwrap();

    let copy_count = db
        .collection::<BookCopy>("book_copies")
        .count_documents(doc! { "book_id": book_id })
        .await
        .unwrap();
    assert_eq!(copy_count, 2, "同一書目應關聯兩本獨立館藏副本");

    let member_a_id = db
        .collection::<Member>("members")
        .insert_one(Member {
            id: None,
            user_id: ObjectId::new(),
            card_number: "LIB-M001".to_string(),
            status: MemberStatus::Active,
            max_borrow_limit: 5,
        })
        .await
        .unwrap()
        .inserted_id
        .as_object_id()
        .unwrap();

    let member_b_id = db
        .collection::<Member>("members")
        .insert_one(Member {
            id: None,
            user_id: ObjectId::new(),
            card_number: "LIB-M002".to_string(),
            status: MemberStatus::Active,
            max_borrow_limit: 5,
        })
        .await
        .unwrap()
        .inserted_id
        .as_object_id()
        .unwrap();

    // =========================================================================
    // 步驟 2：驗證「原則 D：避免重複借出（併發原子鎖定）」
    // =========================================================================
    let svc_1 = Arc::clone(&service);
    let svc_2 = Arc::clone(&service);

    let (res_a, res_b) = tokio::join!(
        svc_1.checkout_copy(member_a_id, copy_1_id, operator_id, 14),
        svc_2.checkout_copy(member_b_id, copy_1_id, operator_id, 14)
    );

    let success_count = [res_a.is_ok(), res_b.is_ok()]
        .iter()
        .filter(|&&ok| ok)
        .count();
    assert_eq!(
        success_count, 1,
        "同一本副本在併發借閱下只能成功借出一次"
    );

    let successful_loan = res_a.or(res_b).unwrap();
    let winner_member_id = successful_loan.member_id;
    let loan_id = successful_loan.id.unwrap();

    let loan_docs_count = db
        .collection::<Loan>("loans")
        .count_documents(doc! { "copy_id": copy_1_id })
        .await
        .unwrap();
    assert_eq!(loan_docs_count, 1);

    let copy_2_state = db
        .collection::<BookCopy>("book_copies")
        .find_one(doc! { "_id": copy_2_id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(copy_2_state.status, CopyStatus::Available);

    // =========================================================================
    // 步驟 3：驗證「原則 C：所有金額都要保留交易紀錄（罰款、折抵、繳費、退款）」
    // =========================================================================
    db.collection::<Loan>("loans")
        .update_one(
            doc! { "_id": loan_id },
            doc! { "$set": { "due_date": mongodb::bson::to_bson(&(Utc::now() - Duration::days(5))).unwrap() } },
        )
        .await
        .unwrap();

    let (_returned_loan, fine_opt) = service.return_copy(loan_id, operator_id, 20).await.unwrap();
    let fine = fine_opt.expect("逾期歸還應產生罰款紀錄");
    let fine_id = fine.id.unwrap();
    assert_eq!(fine.amount, 100);
    assert_eq!(fine.status, FineStatus::Unpaid);

    service
        .record_financial_transaction(
            fine_id,
            operator_id,
            PaymentType::Waiver,
            30,
            "首次逾期活動折抵 30 元",
        )
        .await
        .unwrap();

    let fine_after_waiver = db
        .collection::<Fine>("fines")
        .find_one(doc! { "_id": fine_id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fine_after_waiver.status, FineStatus::PartiallyPaid);

    service
        .record_financial_transaction(
            fine_id,
            operator_id,
            PaymentType::Payment,
            80,
            "櫃台現金繳納 80 元",
        )
        .await
        .unwrap();

    let fine_after_payment = db
        .collection::<Fine>("fines")
        .find_one(doc! { "_id": fine_id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fine_after_payment.status, FineStatus::Paid);

    service
        .record_financial_transaction(
            fine_id,
            operator_id,
            PaymentType::Refund,
            10,
            "退還溢繳金額 10 元",
        )
        .await
        .unwrap();

    // 使用原生 Cursor::advance() 驗證 3 筆獨立交易流水
    let mut payment_cursor = db
        .collection::<PaymentTransaction>("payments")
        .find(doc! { "fine_id": fine_id })
        .await
        .unwrap();

    let mut txs = Vec::new();
    while payment_cursor.advance().await.unwrap() {
        txs.push(payment_cursor.deserialize_current().unwrap());
    }
    assert_eq!(txs.len(), 3, "必須完整保留折抵、繳費與退款三筆獨立交易紀錄");
    assert_eq!(txs[0].tx_type, PaymentType::Waiver);
    assert_eq!(txs[1].tx_type, PaymentType::Payment);
    assert_eq!(txs[2].tx_type, PaymentType::Refund);

    // =========================================================================
    // 步驟 4：驗證「原則 B：借閱紀錄不可直接刪除，停權與報廢不影響歷史關聯」
    // =========================================================================
    let del_res = service.attempt_delete_loan(loan_id).await;
    assert!(
        matches!(del_res, Err(DbDesignError::LoanRecordImmutable)),
        "系統應禁止直接刪除借閱紀錄"
    );

    service
        .suspend_member_and_scrap_copy(winner_member_id, copy_1_id, operator_id)
        .await
        .unwrap();

    let preserved_loan = db
        .collection::<Loan>("loans")
        .find_one(doc! { "_id": loan_id })
        .await
        .unwrap()
        .expect("即使讀者停權且書籍報廢，借閱歷史紀錄仍必須存在");

    let suspended_member = db
        .collection::<Member>("members")
        .find_one(doc! { "_id": preserved_loan.member_id })
        .await
        .unwrap()
        .unwrap();
    let scrapped_copy = db
        .collection::<BookCopy>("book_copies")
        .find_one(doc! { "_id": preserved_loan.copy_id })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(suspended_member.status, MemberStatus::Suspended);
    assert_eq!(scrapped_copy.status, CopyStatus::Scrapped);

    db.drop().await.unwrap();
}