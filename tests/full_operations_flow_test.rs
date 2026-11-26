use chrono::{Duration, TimeZone, Utc};
use rust_library_system::models::operations::*;
use rust_library_system::services::operations_service::OperationsService;

#[test]
fn test_complete_library_operations_modules_a_to_f() {
    // 初始化服務與罰款規則：每日 5 元、單筆上限 100 元、累積 10 點違規自動停權
    let rules = FineRuleConfig {
        daily_rate: 5.0,
        max_fine_per_loan: 100.0,
        auto_suspend_points: 10,
    };
    let mut svc = OperationsService::new(rules);
    let base_time = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();

    // =========================================================================
    // 測試模組 A：館藏管理 (ISBN 匯入、修改查詢、多副本與條碼、館藏報廢)
    // =========================================================================
    let rust_book = svc.import_book_by_isbn(
        "978-1718503106",
        "The Rust Programming Language",
        "Steve Klabnik",
        "No Starch Press",
        "程式設計",
        Some("https://example.com/rust-cover.jpg".into()),
        vec!["https://example.com/rust-sample.pdf".into()],
    );
    let db_book = svc.import_book_by_isbn(
        "978-1449373320",
        "Designing Data-Intensive Applications",
        "Martin Kleppmann",
        "O'Reilly",
        "系統架構",
        None,
        vec![],
    );

    // 修改書目與關鍵字查詢
    svc.update_book_metadata(rust_book.id, "The Rust Programming Language (2nd Ed.)", "程式設計")
        .expect("書目修改應成功");
    let found = svc.search_books("Rust");
    assert_eq!(found.len(), 1);
    assert!(found[0].title.contains("2nd Ed."));

    // 建立多本館藏副本與條碼 (總館 MAIN 與分館 EAST)
    let (copy_r1, label_r1) = svc
        .add_copy_with_barcode(rust_book.id, "MAIN", "A1-01-01")
        .unwrap();
    let (copy_r2, _) = svc
        .add_copy_with_barcode(rust_book.id, "EAST", "B2-03-04")
        .unwrap();
    let (copy_db1, _) = svc
        .add_copy_with_barcode(db_book.id, "MAIN", "A1-02-05")
        .unwrap();
    let (copy_damaged, _) = svc
        .add_copy_with_barcode(db_book.id, "MAIN", "A1-02-06")
        .unwrap();

    assert!(label_r1.contains(&copy_r1.barcode));

    // 標記損壞與報廢
    svc.update_copy_condition(copy_damaged.id, CopyCondition::Damaged)
        .unwrap();
    svc.update_copy_condition(copy_damaged.id, CopyCondition::Discarded)
        .unwrap();

    // =========================================================================
    // 測試模組 B：讀者管理 (註冊、QR Code、分級權限、違規停權與黑名單)
    // =========================================================================
    let student = svc.register_patron("王小明", PatronCategory::Student, MembershipTier::Silver);
    let faculty = svc.register_patron("林教授", PatronCategory::Faculty, MembershipTier::Gold);
    let bad_user = svc.register_patron("陳違規", PatronCategory::Public, MembershipTier::Standard);

    assert!(student.qr_code_payload.contains(&student.card_number));
    assert_eq!(student.loan_policy(), (10, 21)); // 學生 8 + 銀卡 2 = 10 冊，21 天
    assert_eq!(faculty.loan_policy(), (20, 30)); // 教師 15 + 金卡 5 = 20 冊，30 天

    // 測試違規記點觸發自動停權
    svc.record_patron_violation(bad_user.id, "嚴重毀損館藏", 12, false)
        .unwrap();
    assert!(svc.patrons.get(&bad_user.id).unwrap().is_suspended);
    assert!(svc
        .borrow_copy(bad_user.id, &copy_r1.barcode, "MAIN", base_time)
        .is_err());

    // =========================================================================
    // 測試模組 C & D：借閱管理 (批次借閱、跨館借閱、續借) 與 預約候補流程
    // =========================================================================
    // 學生批次借閱 2 本書 (其中 copy_r2 從 EAST 分館跨館至 MAIN 總館取書)
    let batch_loans = svc
        .batch_borrow(
            student.id,
            &[&copy_r1.barcode, &copy_r2.barcode],
            "MAIN",
            base_time,
        )
        .expect("批次借閱應成功");
    assert_eq!(batch_loans.len(), 2);
    assert_eq!(batch_loans[1].origin_branch, "EAST");
    assert_eq!(batch_loans[1].pickup_branch, "MAIN");

    // 個人借閱歷史追蹤
    assert_eq!(svc.get_patron_loan_history(student.id).len(), 2);

    // 教師線上預約 Rust 書籍 (目前 2 本皆借出，進入候補第 1 位)
    let res1 = svc
        .create_reservation(faculty.id, rust_book.id, base_time + Duration::days(1))
        .unwrap();
    assert_eq!(res1.queue_position, 1);

    // 因為已有讀者預約候補，學生嘗試續借應被拒絕
    assert!(svc.renew_loan(batch_loans[0].id).is_err());

    // 學生借閱另一本無預約的系統架構書籍並成功續借
    let db_loan = svc
        .borrow_copy(student.id, &copy_db1.barcode, "MAIN", base_time)
        .unwrap();
    let renewed_db_loan = svc.renew_loan(db_loan.id).expect("無預約時續借應成功");
    assert_eq!(renewed_db_loan.renewal_count, 1);

    // =========================================================================
    // 測試模組 E：逾期與罰款管理 (自動計算天數、上限控制、繳費、減免、退款)
    // =========================================================================
    // 學生於第 50 天歸還 copy_r1 (借期 21 天，逾期 29 天 -> 29 * 5 = 145 元 -> 觸發上限 100 元)
    // 同時因教師有預約候補，copy_r1 歸還後應自動轉為 Reserved 並通知教師
    let return_time = base_time + Duration::days(50);
    let (_, fine_opt) = svc.return_copy(&copy_r1.barcode, return_time).unwrap();

    let fine = fine_opt.expect("逾期歸還應產生罰款單");
    assert_eq!(fine.overdue_days, 29);
    assert_eq!(fine.original_amount, 145.0);
    assert_eq!(fine.final_amount, 100.0); // 驗證罰款上限規則

    // 驗證預約狀態轉為 Ready 且已通知
    let updated_res = svc.reservations.iter().find(|r| r.id == res1.id).unwrap();
    assert_eq!(updated_res.status, ReservationStatus::Ready);
    assert!(updated_res.notified);

    // 繳費與後續退款流程測試
    let paid_fine = svc.pay_fine(fine.id).unwrap();
    assert_eq!(paid_fine.status, FineStatus::Paid);
    let refunded_fine = svc.refund_fine(fine.id, "系統假日順延誤算").unwrap();
    assert_eq!(refunded_fine.status, FineStatus::Refunded);

    // 驗證預約逾期未取自動取消機制 (取書期限為 3 天，於第 55 天掃描應自動取消)
    let expired_count = svc.process_expired_reservations(base_time + Duration::days(55));
    assert_eq!(expired_count, 1);
    assert_eq!(
        svc.copies.get(&copy_r1.id).unwrap().condition,
        CopyCondition::Available
    );

    // =========================================================================
    // 測試模組 F：統計分析 Dashboard (驗證 12 項營運指標)
    // =========================================================================
    let stats_now = base_time + Duration::days(55);
    let dashboard = svc.generate_dashboard_stats(stats_now);

    // 總館藏 4 本扣除 1 本報廢 = 3 本有效館藏
    assert_eq!(dashboard.total_copies, 3);
    // copy_r1 已歸還釋放 (1 本可借)，copy_r2 與 copy_db1 仍借出中 (2 本借出)
    assert_eq!(dashboard.available_copies, 1);
    assert_eq!(dashboard.borrowed_copies, 2);
    // copy_r2 (第21天到期) 與 copy_db1 (續借後第35天到期) 至第55天皆已逾期未還 (共 2 本逾期)
    assert_eq!(dashboard.overdue_unreturned, 2);
    // 熱門書籍排行第一名應為 Rust (共借出 2 冊次)
    assert_eq!(
        dashboard.top_books[0].label,
        "The Rust Programming Language (2nd Ed.)"
    );
    assert_eq!(dashboard.top_books[0].count, 2);
    // 熱門分類第一名為「程式設計」
    assert_eq!(dashboard.top_categories[0].label, "程式設計");
    // 讀者排行第一名為「王小明」
    assert_eq!(dashboard.top_patrons[0].label, "王小明");
    // 2026-11 月份借閱趨勢統計應為 3 筆
    assert_eq!(dashboard.monthly_trends, vec![("2026-11".to_string(), 3)]);
    // 館藏使用率 = 2 / 3 = 66.7%
    assert_eq!(dashboard.collection_utilization_rate, 66.7);
}
