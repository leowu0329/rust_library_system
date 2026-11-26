use bson::oid::ObjectId;
use chrono::{Duration, Utc};
use rust_library_system::models::book::CopyStatus;
use rust_library_system::models::circulation::{LoanStatus, ReservationStatus};
use rust_library_system::models::user::{Role, User};
use rust_library_system::services::library_service::{
    LibraryError, LibraryService, LibraryStore,
};

fn setup_test_service() -> (LibraryService, User, User, User) {
    let store = LibraryStore::default();

    let alice_id = ObjectId::new();
    let alice = User {
        id: Some(alice_id),
        card_number: "CARD-001".to_string(),
        name: "Alice".to_string(),
        role: Role::Member,
        is_active: true,
        max_borrow_limit: 2,
        max_reserve_limit: 2,
        unpaid_fines: 0,
    };

    let bob_id = ObjectId::new();
    let bob = User {
        id: Some(bob_id),
        card_number: "CARD-002".to_string(),
        name: "Bob".to_string(),
        role: Role::Member,
        is_active: true,
        max_borrow_limit: 2,
        max_reserve_limit: 2,
        unpaid_fines: 0,
    };

    let charlie_id = ObjectId::new();
    let charlie = User {
        id: Some(charlie_id),
        card_number: "CARD-003".to_string(),
        name: "Charlie".to_string(),
        role: Role::Member,
        is_active: true,
        max_borrow_limit: 2,
        max_reserve_limit: 1,
        unpaid_fines: 0,
    };

    {
        let mut users = store.users.write().unwrap();
        users.insert(alice_id, alice.clone());
        users.insert(bob_id, bob.clone());
        users.insert(charlie_id, charlie.clone());
    }

    (LibraryService::new(store), alice, bob, charlie)
}

#[test]
fn test_complete_library_business_workflows() {
    let (service, _alice, bob, charlie) = setup_test_service();
    let bob_id = bob.id.unwrap();
    let charlie_id = charlie.id.unwrap();
    let t0 = Utc::now();

    // 1. 書目與館藏副本分離設計
    let js_book = service.create_book(
        "978-986-434-000-1",
        "JavaScript 完整學習指南",
        "David Flanagan",
        "歐萊禮",
        "程式設計",
        vec!["JavaScript".into(), "前端開發".into()],
    );
    let js_book_id = js_book.id.unwrap();

    let copy1 = service
        .add_book_copy(js_book_id, "BC-JS-001", "A區-1架")
        .unwrap();
    let copy2 = service
        .add_book_copy(js_book_id, "BC-JS-002", "A區-2架")
        .unwrap();

    assert_ne!(copy1.id, copy2.id);
    assert_eq!(copy1.status, CopyStatus::Available);

    // 2. 借閱流程與預約規則
    let loan1 = service.borrow_book("CARD-001", "BC-JS-001", t0).unwrap();
    assert_eq!(loan1.due_date, t0 + Duration::days(14));

    // 尚有在館副本不可預約
    let err_avail = service.reserve_book(bob_id, js_book_id, t0).unwrap_err();
    assert_eq!(err_avail, LibraryError::CopiesAvailableNoNeedToReserve);

    // 借走最後一本後開放預約
    service.borrow_book("CARD-002", "BC-JS-002", t0).unwrap();

    let res_charlie = service
        .reserve_book(charlie_id, js_book_id, t0 + Duration::hours(1))
        .unwrap();
    assert_eq!(res_charlie.status, ReservationStatus::Waiting);

    // 禁止重複預約
    let err_dup = service
        .reserve_book(charlie_id, js_book_id, t0 + Duration::hours(2))
        .unwrap_err();
    assert_eq!(err_dup, LibraryError::DuplicateReservation);

    // 超過預約數量上限
    let rust_book = service.create_book(
        "978-1718500440",
        "The Rust Programming Language",
        "Steve Klabnik",
        "No Starch Press",
        "系統程式",
        vec!["Rust".into()],
    );
    let rust_book_id = rust_book.id.unwrap();
    service
        .add_book_copy(rust_book_id, "BC-RS-001", "B區-1架")
        .unwrap();
    service.borrow_book("CARD-001", "BC-RS-001", t0).unwrap();

    let err_limit = service
        .reserve_book(charlie_id, rust_book_id, t0 + Duration::hours(3))
        .unwrap_err();
    assert_eq!(err_limit, LibraryError::ReserveLimitExceeded);

    // 3. 歸還書籍、逾期罰金與通知第一順位
    let return_time_alice = t0 + Duration::days(17);
    let (returned_loan1, notified_res) = service
        .return_book("BC-JS-001", return_time_alice)
        .unwrap();

    assert_eq!(returned_loan1.status, LoanStatus::Overdue);
    assert_eq!(returned_loan1.fine_amount, 15);

    let err_fine = service
        .borrow_book("CARD-001", "BC-JS-001", return_time_alice)
        .unwrap_err();
    assert_eq!(err_fine, LibraryError::IneligibleUser);

    let notified = notified_res.expect("應觸發通知第一順位預約者 Charlie");
    assert_eq!(notified.user_id, charlie_id);
    assert_eq!(notified.status, ReservationStatus::Ready);
    assert_eq!(notified.assigned_copy_id, copy1.id);

    // 4. 預約逾期未取，自動遞補下一順位
    let dave_id = ObjectId::new();
    let dave = User {
        id: Some(dave_id),
        card_number: "CARD-004".to_string(),
        name: "Dave".to_string(),
        role: Role::Member,
        is_active: true,
        max_borrow_limit: 2,
        max_reserve_limit: 2,
        unpaid_fines: 0,
    };
    service.store.users.write().unwrap().insert(dave_id, dave);
    service
        .reserve_book(dave_id, js_book_id, return_time_alice + Duration::hours(1))
        .unwrap();

    let check_time = return_time_alice + Duration::days(4);
    let newly_notified = service.process_expired_reservations(check_time);

    assert_eq!(newly_notified.len(), 1);
    assert_eq!(newly_notified[0].user_id, dave_id);
    assert_eq!(newly_notified[0].status, ReservationStatus::Ready);

    // 5. 館藏損壞轉移與取消
    let return_time_bob = t0 + Duration::days(10);
    service.return_book("BC-JS-002", return_time_bob).unwrap();

    service
        .report_copy_damaged("BC-JS-001", check_time + Duration::hours(1))
        .unwrap();

    let dave_res = service
        .store
        .reservations
        .read()
        .unwrap()
        .iter()
        .find(|r| r.user_id == dave_id && r.book_id == js_book_id)
        .cloned()
        .unwrap();

    assert_eq!(dave_res.status, ReservationStatus::Ready);
    assert_eq!(dave_res.assigned_copy_id, copy2.id);

    service
        .report_copy_damaged("BC-JS-002", check_time + Duration::hours(2))
        .unwrap();

    let dave_res_after_all_damaged = service
        .store
        .reservations
        .read()
        .unwrap()
        .iter()
        .find(|r| r.user_id == dave_id && r.book_id == js_book_id)
        .cloned()
        .unwrap();

    assert_eq!(
        dave_res_after_all_damaged.status,
        ReservationStatus::Cancelled
    );
}