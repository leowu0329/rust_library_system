use rust_library_system::models::extensibility::{
    EBookFormat, KioskAction, NotificationChannel, TransferStatus,
};
use rust_library_system::services::extensibility_service::ExtensibilityService;
use uuid::Uuid;

#[test]
fn test_full_system_extensibility_and_audit_lifecycle() {
    let ext = ExtensibilityService::new();

    // ========================================================================
    // 1. 多分館管理 + 完整審計欄位 (id, created_at, updated_at, created_by, updated_by, deleted_at)
    // ========================================================================
    let main_branch = ext
        .create_branch("TN-MAIN", "台南總館", "台南市永康區康橋大道255號", true, "admin_leo")
        .expect("建立總館應成功");
    let east_branch = ext
        .create_branch("TN-EAST", "台南東區分館", "台南市東區崇德路", true, "admin_leo")
        .expect("建立東區分館應成功");
    let temp_branch = ext
        .create_branch("TN-TEMP", "臨時流動站", "台南市安平區", false, "admin_leo")
        .expect("建立臨時分館應成功");

    // 驗證核心審計欄位皆正確填入
    assert_eq!(main_branch.audit.created_by, "admin_leo");
    assert_eq!(main_branch.audit.updated_by, "admin_leo");
    assert!(main_branch.audit.deleted_at.is_none());
    assert!(main_branch.audit.created_at <= main_branch.audit.updated_at);

    // 測試軟刪除：刪除臨時分館後，查詢活躍清單應被過濾，但底層仍保留 deleted_at 紀錄
    ext.soft_delete_branch(temp_branch.audit.id, "supervisor_wu")
        .expect("軟刪除分館應成功");
    assert_eq!(ext.list_active_branches().len(), 2);

    let raw_temp = ext.get_branch_raw(temp_branch.audit.id).unwrap();
    assert!(raw_temp.audit.is_deleted());
    assert_eq!(raw_temp.audit.updated_by, "supervisor_wu");

    // ========================================================================
    // 2. ISBN API 自動取得書籍資訊
    // ========================================================================
    let isbn_meta = ext
        .fetch_metadata_by_isbn("978-986-434-512-0")
        .expect("ISBN API 應自動解析標準書籍資訊");
    assert_eq!(isbn_meta.title, "Rust 程式設計實戰與系統架構");
    assert_eq!(isbn_meta.category, "資訊工程");

    // ========================================================================
    // 3 & 4. RFID 與 QR Code 館藏標籤 & 讀者數位證註冊 + RFID 盤點
    // ========================================================================
    let copy_1_id = Uuid::new_v4();
    let copy_2_id = Uuid::new_v4();
    let copy_3_id = Uuid::new_v4();

    ext.register_copy_smart_tag(
        copy_1_id,
        &isbn_meta.title,
        &isbn_meta.category,
        main_branch.audit.id,
        "RFID-EPC-0001",
        "QR-BOOK-0001",
        "cataloger_01",
    )
    .unwrap();

    ext.register_copy_smart_tag(
        copy_2_id,
        "深入淺出 Rust 記憶體安全",
        "資訊工程",
        main_branch.audit.id,
        "RFID-EPC-0002",
        "QR-BOOK-0002",
        "cataloger_01",
    )
    .unwrap();

    ext.register_copy_smart_tag(
        copy_3_id,
        "分布式系統設計模式",
        "資訊工程",
        east_branch.audit.id,
        "RFID-EPC-0003",
        "QR-BOOK-0003",
        "cataloger_01",
    )
    .unwrap();

    // 驗證 QR Code 查詢館藏
    let qr_queried = ext.lookup_copy_by_qr("QR-BOOK-0001").unwrap();
    assert_eq!(qr_queried.rfid_epc, "RFID-EPC-0001");
    assert!(qr_queried.eas_security_locked);

    // 註冊讀者數位證（綁定 Email 與 LINE User ID）
    let reader_card = ext.register_reader_digital_card(
        "R2026001",
        "吳俊男",
        Some("leowu0329@example.com"),
        Some("U_LINE_99887766"),
        vec!["心理學".to_string()],
        "counter_staff",
    );
    assert_eq!(reader_card.qr_card_code, "QR-READER-R2026001");

    // 執行總館 RFID 盤點（模擬掃描到 EPC-0001，漏掃 EPC-0002，且誤掃到東區分館的 EPC-0003）
    let inventory_report = ext.run_rfid_inventory_audit(
        main_branch.audit.id,
        &["RFID-EPC-0001".to_string(), "RFID-EPC-0003".to_string()],
    );
    assert_eq!(inventory_report.expected_on_shelf_count, 2);
    assert_eq!(inventory_report.scanned_matched_count, 1);
    assert_eq!(inventory_report.missing_rfid_epcs, vec!["RFID-EPC-0002"]);
    assert_eq!(inventory_report.misplaced_rfid_epcs, vec!["RFID-EPC-0003"]);

    // ========================================================================
    // 5. 自助借還機工作站 (QR Code 讀者證 + RFID 書籍感應 + EAS 防盜磁條連動)
    // ========================================================================
    let borrow_log = ext
        .kiosk_self_borrow(main_branch.audit.id, "QR-READER-R2026001", "RFID-EPC-0001")
        .expect("自助借書應成功");
    assert_eq!(borrow_log.action, KioskAction::SelfBorrow);

    // 確認書籍已借出且防盜磁條（EAS）已解除鎖定
    let after_borrow_tag = ext.lookup_copy_by_qr("QR-BOOK-0001").unwrap();
    assert!(after_borrow_tag.is_checked_out);
    assert!(!after_borrow_tag.eas_security_locked);

    // 執行自助還書，確認重新上鎖防盜磁條
    let return_log = ext
        .kiosk_self_return(main_branch.audit.id, "RFID-EPC-0001")
        .expect("自助還書應成功");
    assert_eq!(return_log.action, KioskAction::SelfReturn);
    let after_return_tag = ext.lookup_copy_by_qr("QR-BOOK-0001").unwrap();
    assert!(!after_return_tag.is_checked_out);
    assert!(after_return_tag.eas_security_locked);

    // ========================================================================
    // 6. 跨館借閱與調撥 + Email / LINE 雙管道自動通知
    // ========================================================================
    let transfer = ext
        .request_inter_branch_transfer(
            copy_3_id,
            "R2026001",
            main_branch.audit.id,
            "R2026001",
        )
        .expect("申請從東區分館調撥至總館應成功");
    assert_eq!(transfer.status, TransferStatus::Requested);

    let arrived = ext
        .complete_inter_branch_arrival(transfer.audit.id, "logistics_staff")
        .expect("跨館到館點收應成功");
    assert_eq!(arrived.status, TransferStatus::ArrivedAtPickupBranch);

    // 驗證複本已轉移至總館
    let transferred_copy = ext.lookup_copy_by_qr("QR-BOOK-0003").unwrap();
    assert_eq!(transferred_copy.branch_id, main_branch.audit.id);

    // 驗證 Email 與 LINE 雙管道通知紀錄皆已寫入且不可刪除
    let notif_logs = ext.get_notification_logs();
    assert!(notif_logs.iter().any(|n| n.channel == NotificationChannel::Email && n.delivered));
    assert!(notif_logs.iter().any(|n| n.channel == NotificationChannel::Line && n.delivered));

    // ========================================================================
    // 7. 電子書資源管理 (DRM 同時授權席次控管)
    // ========================================================================
    let ebook = ext.add_ebook_resource(
        "9789860000001",
        "高併發 Rust 非同步架構設計",
        "Leo Wu",
        "資訊工程",
        EBookFormat::Epub,
        1,
        "https://ebook.library.local/download/rust-async.epub",
        "digital_admin",
    );

    let ebook_loan = ext
        .borrow_ebook(ebook.audit.id, "R2026001", 14, "R2026001")
        .expect("第一位讀者借閱電子書應成功");
    assert!(ebook_loan.returned_at.is_none());

    // 第二位讀者嘗試借閱同一本僅有 1 席次的電子書應被 DRM 阻擋
    let second_attempt = ext.borrow_ebook(ebook.audit.id, "R2026002", 14, "R2026002");
    assert!(second_attempt.is_err());

    // ========================================================================
    // 8. AI 智慧推薦引擎（依據閱讀軌跡與偏好權重推薦未讀實體書與電子書）
    // ========================================================================
    let recommendations = ext.recommend_books_for_reader("R2026001");
    assert!(!recommendations.is_empty());
    assert!(recommendations
        .iter()
        .any(|r| r.title == "高併發 Rust 非同步架構設計" || r.title == "深入淺出 Rust 記憶體安全"));
    assert!(!recommendations
        .iter()
        .any(|r| r.title == "Rust 程式設計實戰與系統架構"));

    // ========================================================================
    // 9. 驗證不可變歷史稽核紀錄完整性（自助借還、跨館調撥、通知紀錄皆完整保留）
    // ========================================================================
    assert_eq!(ext.get_kiosk_logs().len(), 2);
    assert_eq!(ext.get_transfer_history().len(), 1);
    assert!(ext.get_notification_logs().len() >= 4);
}