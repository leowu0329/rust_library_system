use bson::uuid::Uuid;
use chrono::{Duration, TimeZone, Utc};
use rust_library_system::models::exception_handling::*;
use rust_library_system::services::exception_handling_service::*;

/// 輔助函式：建立測試讀者
fn make_reader(
    card: &str,
    name: &str,
    email: &str,
    status: ReaderStatus,
    limit: usize,
    allow_overdue: bool,
) -> Reader {
    Reader {
        id: Uuid::new(),
        card_number: card.to_string(),
        name: name.to_string(),
        email: email.to_string(),
        status,
        max_borrow_limit: limit,
        allow_borrow_with_overdue: allow_overdue,
    }
}

/// 輔助函式：建立測試副本
fn make_copy(book_id: Uuid, barcode: &str, state: CopyState) -> PhysicalCopy {
    PhysicalCopy {
        id: Uuid::new(),
        book_id,
        barcode: barcode.to_string(),
        state,
        note: None,
    }
}

#[test]
fn test_all_ten_exception_and_edge_case_scenarios() {
    let mut svc = ExceptionHandlingService::new();
    let base_time = Utc.with_ymd_and_hms(2026, 11, 26, 10, 0, 0).unwrap();

    // 建立測試書目
    let book_rust = CatalogBook {
        id: Uuid::new(),
        isbn: "978-1718503106".into(),
        title: "Rust 系統程式設計實戰".into(),
    };
    let book_clean = CatalogBook {
        id: Uuid::new(),
        isbn: "978-0132350884".into(),
        title: "Clean Code 無瑕的程式碼".into(),
    };
    svc.state.books.insert(book_rust.id, book_rust.clone());
    svc.state.books.insert(book_clean.id, book_clean.clone());

    // 建立 5 本館藏副本
    let copy_1 = make_copy(book_rust.id, "BK-001", CopyState::Available);
    let copy_2 = make_copy(book_rust.id, "BK-002", CopyState::Available);
    let copy_3 = make_copy(book_clean.id, "BK-003", CopyState::Available);
    let copy_lost = make_copy(book_clean.id, "BK-004", CopyState::Available);
    let copy_dmg = make_copy(book_clean.id, "BK-005", CopyState::Available);

    for c in [&copy_1, &copy_2, &copy_3, &copy_lost, &copy_dmg] {
        svc.state.copies.insert(c.id, c.clone());
    }

    // 建立測試讀者
    let suspended_reader = make_reader(
        "R-001",
        "張停權",
        "suspended@example.com",
        ReaderStatus::Suspended,
        5,
        false,
    );
    let alice = make_reader(
        "R-002",
        "林愛麗",
        "alice@example.com",
        ReaderStatus::Active,
        2, // 借閱上限設為 2 本，方便測試上限情境
        false,
    );
    let bob = make_reader(
        "R-003",
        "王大明",
        "bob@example.com",
        ReaderStatus::Active,
        5,
        false,
    );

    for r in [&suspended_reader, &alice, &bob] {
        svc.state.readers.insert(r.id, r.clone());
    }

    // =========================================================================
    // 情境 1：讀者已停權 -> 禁止借閱
    // =========================================================================
    let res_suspended = svc.checkout(suspended_reader.id, "BK-001", base_time);
    assert_eq!(res_suspended.unwrap_err(), CirculationException::ReaderSuspended);

    // =========================================================================
    // 情境 2：讀者有逾期書籍 -> 依規則限制借閱
    // =========================================================================
    // 愛麗於第 0 天借閱 BK-001（借期 14 天）
    svc.checkout(alice.id, "BK-001", base_time).unwrap();
    // 快轉至第 20 天（BK-001 已逾期 6 天），愛麗嘗試再借 BK-002 應被攔截
    let day_20 = base_time + Duration::days(20);
    let res_overdue_block = svc.checkout(alice.id, "BK-002", day_20);
    assert_eq!(
        res_overdue_block.unwrap_err(),
        CirculationException::ReaderHasOverdueBooks
    );

    // 歸還逾期書籍後，恢復正常借閱資格
    let mut ok_mailer = MockEmailGateway::new(false);
    svc.return_book_transactional("BK-001", day_20, &mut ok_mailer)
        .unwrap();

    // =========================================================================
    // 情境 3：已達借閱上限 -> 禁止新增借閱
    // =========================================================================
    // 愛麗借閱上限為 2 本，先借出 BK-001 與 BK-002
    svc.checkout(alice.id, "BK-001", day_20).unwrap();
    svc.checkout(alice.id, "BK-002", day_20).unwrap();
    // 嘗試借第 3 本 BK-003 應觸發上限錯誤
    let res_limit = svc.checkout(alice.id, "BK-003", day_20);
    assert_eq!(
        res_limit.unwrap_err(),
        CirculationException::BorrowLimitReached(2)
    );

    // =========================================================================
    // 情境 4：書籍已被借出 -> 禁止重複借閱
    // =========================================================================
    let res_already_borrowed = svc.checkout(bob.id, "BK-001", day_20);
    assert_eq!(
        res_already_borrowed.unwrap_err(),
        CirculationException::CopyAlreadyBorrowed
    );

    // =========================================================================
    // 情境 8：讀者重複預約 -> 防止重複建立
    // =========================================================================
    // 大明預約 book_rust（第 1 次成功）
    let bob_hold = svc.place_hold(bob.id, book_rust.id, day_20).unwrap();
    assert_eq!(bob_hold.queue_order, 1);

    // 大明再次預約同一本書（第 2 次應被拒絕）
    let dup_hold = svc.place_hold(bob.id, book_rust.id, day_20);
    assert_eq!(
        dup_hold.unwrap_err(),
        CirculationException::DuplicateReservation
    );

    // 已借閱該書的愛麗也不可重複預約同一本書
    let borrower_hold = svc.place_hold(alice.id, book_rust.id, day_20);
    assert_eq!(
        borrower_hold.unwrap_err(),
        CirculationException::DuplicateReservation
    );

    // =========================================================================
    // 情境 9：歸還時系統中斷 -> 確保交易一致性（狀態完全回滾）
    // =========================================================================
    svc.simulate_return_crash = true; // 注入系統崩潰故障
    let day_22 = base_time + Duration::days(22);
    let crash_res = svc.return_book_transactional("BK-001", day_22, &mut ok_mailer);
    assert!(matches!(
        crash_res.unwrap_err(),
        CirculationException::TransactionAborted(_)
    ));

    // 驗證原子回滾：BK-001 必須仍維持 Borrowed，借閱紀錄 returned_at 仍為 None，預約仍為 Waiting
    assert_eq!(
        svc.state.copies.get(&copy_1.id).unwrap().state,
        CopyState::Borrowed
    );
    let active_bk1 = svc
        .state
        .borrows
        .iter()
        .find(|b| b.copy_id == copy_1.id && b.returned_at.is_none());
    assert!(active_bk1.is_some(), "系統中斷後借閱紀錄不應被部分修改");
    assert_eq!(svc.state.holds[0].status, HoldStatus::Waiting);

    // 解除系統崩潰故障旗標
    svc.simulate_return_crash = false;

    // =========================================================================
    // 情境 5 & 情境 10：書籍已被預約依候補規則處理 + Email 寄送失敗保留重試紀錄
    // =========================================================================
    // 模擬 SMTP 伺服器故障，愛麗正常歸還 BK-001
    let mut failing_mailer = MockEmailGateway::new(true);
    svc.return_book_transactional("BK-001", day_22, &mut failing_mailer)
        .expect("Email 失敗不應阻斷實體歸還交易");

    // 驗證候補規則：BK-001 狀態轉為 Reserved，大明的預約轉為 Ready
    assert_eq!(
        svc.state.copies.get(&copy_1.id).unwrap().state,
        CopyState::Reserved
    );
    assert_eq!(svc.state.holds[0].status, HoldStatus::Ready);
    assert_eq!(svc.state.holds[0].assigned_copy_id, Some(copy_1.id));

    // 若非預約者的愛麗嘗試借走這本 Reserved 書籍，應被拒絕
    let wrong_reader_checkout = svc.checkout(alice.id, "BK-001", day_22);
    assert_eq!(
        wrong_reader_checkout.unwrap_err(),
        CirculationException::CopyReservedForAnotherReader
    );

    // 第一順位預約者大明取書借閱，應成功並將預約標記為 Fulfilled
    svc.checkout(bob.id, "BK-001", day_22).unwrap();
    assert_eq!(svc.state.holds[0].status, HoldStatus::Fulfilled);

    // 驗證 Email 失敗重試紀錄已寫入佇列
    assert_eq!(svc.state.email_logs.len(), 1);
    assert_eq!(
        svc.state.email_logs[0].status,
        NotificationStatus::PendingRetry
    );
    assert_eq!(svc.state.email_logs[0].retry_count, 1);

    // 模擬 10 分鐘後 SMTP 恢復正常，執行背景重試排程
    let mut recovered_mailer = MockEmailGateway::new(false);
    let retried = svc.process_email_retry_queue(day_22 + Duration::minutes(10), &mut recovered_mailer);
    assert_eq!(retried, 1);
    assert_eq!(svc.state.email_logs[0].status, NotificationStatus::Sent);
    assert_eq!(recovered_mailer.sent_messages.len(), 1);

    // =========================================================================
    // 情境 6：書籍遺失 -> 更新館藏狀態並保留紀錄
    // =========================================================================
    let lost_log = svc
        .report_lost("BK-004", Some(bob.id), 650, day_22)
        .unwrap();
    assert_eq!(lost_log.incident_type, IncidentType::Lost);
    assert_eq!(
        svc.state.copies.get(&copy_lost.id).unwrap().state,
        CopyState::Lost
    );
    // 確認館藏紀錄未被刪除，且再次借閱會明確回傳 CopyLost
    assert_eq!(
        svc.checkout(bob.id, "BK-004", day_22).unwrap_err(),
        CirculationException::CopyLost
    );

    // =========================================================================
    // 情境 7：書籍損壞 -> 進入維修或待處理狀態
    // =========================================================================
    let dmg_log = svc
        .report_damaged("BK-005", Some(alice.id), "書背脫膠與內頁水漬", 200, day_22)
        .unwrap();
    assert_eq!(dmg_log.incident_type, IncidentType::Damaged);
    assert_eq!(
        svc.state.copies.get(&copy_dmg.id).unwrap().state,
        CopyState::Maintenance
    );
    // 確認損壞期間禁止借閱
    assert_eq!(
        svc.checkout(bob.id, "BK-005", day_22).unwrap_err(),
        CirculationException::CopyInMaintenance
    );

    // 驗證所有異常事件稽核紀錄皆完整留存
    assert_eq!(svc.state.incident_logs.len(), 2);
}
