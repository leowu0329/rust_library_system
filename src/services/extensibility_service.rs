use crate::models::extensibility::*;
use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// 系統擴充模組專屬領域錯誤（與既有 AppError 隔離，避免破壞現有 Handler）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensibilityError {
    NotFound(String),
    ValidationError(String),
}

impl fmt::Display for ExtensibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtensibilityError::NotFound(msg) => write!(f, "查無資料: {}", msg),
            ExtensibilityError::ValidationError(msg) => write!(f, "驗證失敗: {}", msg),
        }
    }
}

impl std::error::Error for ExtensibilityError {}

/// 系統擴充性領域服務（管理多分館、跨館借閱、RFID/QR、電子書、ISBN API、通知、自助借還、AI 推薦）
#[derive(Clone)]
pub struct ExtensibilityService {
    branches: Arc<RwLock<HashMap<Uuid, LibraryBranch>>>,
    smart_tags: Arc<RwLock<HashMap<Uuid, CopySmartTag>>>,
    reader_cards: Arc<RwLock<HashMap<String, ReaderDigitalCard>>>,
    transfers: Arc<RwLock<Vec<InterBranchTransfer>>>,
    ebooks: Arc<RwLock<HashMap<Uuid, EBookResource>>>,
    ebook_loans: Arc<RwLock<Vec<EBookLoanRecord>>>,
    notifications: Arc<RwLock<Vec<NotificationDispatchLog>>>,
    kiosk_logs: Arc<RwLock<Vec<SelfCheckoutLog>>>,
    isbn_catalog_cache: Arc<RwLock<HashMap<String, IsbnBookMetadata>>>,
}

impl Default for ExtensibilityService {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensibilityService {
    pub fn new() -> Self {
        let mut default_isbn_db = HashMap::new();
        default_isbn_db.insert(
            "9789864345120".to_string(),
            IsbnBookMetadata {
                isbn: "9789864345120".to_string(),
                title: "Rust 程式設計實戰與系統架構".to_string(),
                authors: vec!["Steve Klabnik".to_string(), "Carol Nichols".to_string()],
                publisher: "博碩文化".to_string(),
                published_year: 2025,
                category: "資訊工程".to_string(),
                source_provider: "Taiwan-ISBN-Gateway-API".to_string(),
            },
        );
        default_isbn_db.insert(
            "9789573298765".to_string(),
            IsbnBookMetadata {
                isbn: "9789573298765".to_string(),
                title: "福爾摩斯認知思考與推理術".to_string(),
                authors: vec!["Maria Konnikova".to_string()],
                publisher: "遠流出版".to_string(),
                published_year: 2024,
                category: "心理學".to_string(),
                source_provider: "Taiwan-ISBN-Gateway-API".to_string(),
            },
        );

        Self {
            branches: Arc::new(RwLock::new(HashMap::new())),
            smart_tags: Arc::new(RwLock::new(HashMap::new())),
            reader_cards: Arc::new(RwLock::new(HashMap::new())),
            transfers: Arc::new(RwLock::new(Vec::new())),
            ebooks: Arc::new(RwLock::new(HashMap::new())),
            ebook_loans: Arc::new(RwLock::new(Vec::new())),
            notifications: Arc::new(RwLock::new(Vec::new())),
            kiosk_logs: Arc::new(RwLock::new(Vec::new())),
            isbn_catalog_cache: Arc::new(RwLock::new(default_isbn_db)),
        }
    }

    // ========================================================================
    // 1. 多分館管理與軟刪除驗證 (Multi-Branch & Soft Delete)
    // ========================================================================

    pub fn create_branch(
        &self,
        branch_code: &str,
        name: &str,
        address: &str,
        supports_self_checkout: bool,
        operator: &str,
    ) -> Result<LibraryBranch, ExtensibilityError> {
        let mut branches = self.branches.write().unwrap();
        if branches
            .values()
            .any(|b| !b.audit.is_deleted() && b.branch_code == branch_code)
        {
            return Err(ExtensibilityError::ValidationError(format!(
                "分館代碼 {} 已存在",
                branch_code
            )));
        }

        let branch = LibraryBranch {
            audit: AuditMetadata::new(operator),
            branch_code: branch_code.to_string(),
            name: name.to_string(),
            address: address.to_string(),
            is_active: true,
            supports_self_checkout,
        };
        branches.insert(branch.audit.id, branch.clone());
        Ok(branch)
    }

    pub fn soft_delete_branch(
        &self,
        branch_id: Uuid,
        operator: &str,
    ) -> Result<(), ExtensibilityError> {
        let mut branches = self.branches.write().unwrap();
        let branch = branches
            .get_mut(&branch_id)
            .ok_or_else(|| ExtensibilityError::NotFound("找不到指定分館".to_string()))?;

        if branch.audit.is_deleted() {
            return Err(ExtensibilityError::ValidationError(
                "該分館已處於軟刪除狀態".to_string(),
            ));
        }
        branch.is_active = false;
        branch.audit.soft_delete(operator);
        Ok(())
    }

    pub fn list_active_branches(&self) -> Vec<LibraryBranch> {
        let branches = self.branches.read().unwrap();
        branches
            .values()
            .filter(|b| !b.audit.is_deleted() && b.is_active)
            .cloned()
            .collect()
    }

    pub fn get_branch_raw(&self, branch_id: Uuid) -> Option<LibraryBranch> {
        self.branches.read().unwrap().get(&branch_id).cloned()
    }

    // ========================================================================
    // 2. 跨館借閱與調撥 (Inter-Branch Borrowing)
    // ========================================================================

    pub fn request_inter_branch_transfer(
        &self,
        copy_id: Uuid,
        reader_id: &str,
        pickup_branch_id: Uuid,
        operator: &str,
    ) -> Result<InterBranchTransfer, ExtensibilityError> {
        let branches = self.branches.read().unwrap();
        let pickup = branches
            .get(&pickup_branch_id)
            .ok_or_else(|| ExtensibilityError::NotFound("取書分館不存在".to_string()))?;
        if pickup.audit.is_deleted() || !pickup.is_active {
            return Err(ExtensibilityError::ValidationError(
                "取書分館已停用".to_string(),
            ));
        }
        drop(branches);

        let tags = self.smart_tags.read().unwrap();
        let copy = tags
            .get(&copy_id)
            .ok_or_else(|| ExtensibilityError::NotFound("找不到館藏複本".to_string()))?;
        if copy.audit.is_deleted() {
            return Err(ExtensibilityError::ValidationError(
                "館藏複本已下架".to_string(),
            ));
        }
        if copy.is_checked_out {
            return Err(ExtensibilityError::ValidationError(
                "該複本目前借出中，無法申請跨館調撥".to_string(),
            ));
        }
        if copy.branch_id == pickup_branch_id {
            return Err(ExtensibilityError::ValidationError(
                "館藏已位於目標取書分館，無需跨館調撥".to_string(),
            ));
        }
        let source_branch_id = copy.branch_id;
        drop(tags);

        let transfer = InterBranchTransfer {
            audit: ImmutableAuditMetadata::new(operator),
            copy_id,
            reader_id: reader_id.to_string(),
            source_branch_id,
            pickup_branch_id,
            status: TransferStatus::Requested,
        };
        self.transfers.write().unwrap().push(transfer.clone());
        Ok(transfer)
    }

    pub fn complete_inter_branch_arrival(
        &self,
        transfer_id: Uuid,
        operator: &str,
    ) -> Result<InterBranchTransfer, ExtensibilityError> {
        let mut transfers = self.transfers.write().unwrap();
        let transfer = transfers
            .iter_mut()
            .find(|t| t.audit.id == transfer_id)
            .ok_or_else(|| ExtensibilityError::NotFound("找不到跨館調撥單".to_string()))?;

        transfer.status = TransferStatus::ArrivedAtPickupBranch;
        transfer.audit.touch(operator);
        let updated = transfer.clone();
        drop(transfers);

        if let Some(copy) = self.smart_tags.write().unwrap().get_mut(&updated.copy_id) {
            copy.branch_id = updated.pickup_branch_id;
            copy.audit.touch(operator);
        }

        let _ = self.dispatch_multi_channel_notification(
            &updated.reader_id,
            NotificationEventType::InterBranchArrived,
            &format!("您申請的跨館調撥書籍已送達取書分館，調撥單號：{}", updated.audit.id),
            operator,
        );

        Ok(updated)
    }

    pub fn get_transfer_history(&self) -> Vec<InterBranchTransfer> {
        self.transfers.read().unwrap().clone()
    }

    // ========================================================================
    // 3 & 4. RFID 與 QR Code 實體館藏與讀者證管理
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub fn register_copy_smart_tag(
        &self,
        copy_id: Uuid,
        book_title: &str,
        category: &str,
        branch_id: Uuid,
        rfid_epc: &str,
        qr_code_token: &str,
        operator: &str,
    ) -> Result<CopySmartTag, ExtensibilityError> {
        let mut tags = self.smart_tags.write().unwrap();
        if tags
            .values()
            .any(|t| !t.audit.is_deleted() && (t.rfid_epc == rfid_epc || t.qr_code_token == qr_code_token))
        {
            return Err(ExtensibilityError::ValidationError(
                "RFID EPC 或 QR Code Token 重複".to_string(),
            ));
        }

        let tag = CopySmartTag {
            audit: AuditMetadata::new(operator),
            copy_id,
            book_title: book_title.to_string(),
            category: category.to_string(),
            branch_id,
            rfid_epc: rfid_epc.to_string(),
            qr_code_token: qr_code_token.to_string(),
            eas_security_locked: true,
            is_checked_out: false,
            current_holder_id: None,
        };
        tags.insert(copy_id, tag.clone());
        Ok(tag)
    }

    pub fn lookup_copy_by_qr(&self, qr_code_token: &str) -> Result<CopySmartTag, ExtensibilityError> {
        let tags = self.smart_tags.read().unwrap();
        tags.values()
            .find(|t| !t.audit.is_deleted() && t.qr_code_token == qr_code_token)
            .cloned()
            .ok_or_else(|| ExtensibilityError::NotFound("查無對應的 QR Code 館藏".to_string()))
    }

    pub fn register_reader_digital_card(
        &self,
        reader_id: &str,
        reader_name: &str,
        email: Option<&str>,
        line_user_id: Option<&str>,
        preferred_categories: Vec<String>,
        operator: &str,
    ) -> ReaderDigitalCard {
        let card = ReaderDigitalCard {
            audit: AuditMetadata::new(operator),
            reader_id: reader_id.to_string(),
            reader_name: reader_name.to_string(),
            qr_card_code: format!("QR-READER-{}", reader_id),
            email: email.map(|s| s.to_string()),
            line_user_id: line_user_id.map(|s| s.to_string()),
            preferred_categories,
        };
        self.reader_cards
            .write()
            .unwrap()
            .insert(reader_id.to_string(), card.clone());
        card
    }

    pub fn lookup_reader_by_qr(
        &self,
        qr_card_code: &str,
    ) -> Result<ReaderDigitalCard, ExtensibilityError> {
        let cards = self.reader_cards.read().unwrap();
        cards
            .values()
            .find(|c| !c.audit.is_deleted() && c.qr_card_code == qr_card_code)
            .cloned()
            .ok_or_else(|| ExtensibilityError::NotFound("無效的讀者證 QR Code".to_string()))
    }

    /// RFID 手持盤點機批量掃描比對
    pub fn run_rfid_inventory_audit(
        &self,
        branch_id: Uuid,
        scanned_epcs: &[String],
    ) -> RfidInventoryReport {
        let tags = self.smart_tags.read().unwrap();
        let scanned_set: HashSet<&String> = scanned_epcs.iter().collect();

        let expected_epcs: HashSet<String> = tags
            .values()
            .filter(|t| !t.audit.is_deleted() && t.branch_id == branch_id && !t.is_checked_out)
            .map(|t| t.rfid_epc.clone())
            .collect();

        let mut missing_rfid_epcs: Vec<String> = expected_epcs
            .iter()
            .filter(|epc| !scanned_set.contains(epc))
            .cloned()
            .collect();
        missing_rfid_epcs.sort();

        let mut misplaced_rfid_epcs: Vec<String> = scanned_epcs
            .iter()
            .filter(|epc| !expected_epcs.contains(*epc))
            .cloned()
            .collect();
        misplaced_rfid_epcs.sort();

        let scanned_matched_count = expected_epcs
            .iter()
            .filter(|epc| scanned_set.contains(epc))
            .count();

        RfidInventoryReport {
            branch_id,
            expected_on_shelf_count: expected_epcs.len(),
            scanned_matched_count,
            missing_rfid_epcs,
            misplaced_rfid_epcs,
        }
    }

    // ========================================================================
    // 5. 電子書資源管理 (E-Book DRM Licensing & Borrowing)
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub fn add_ebook_resource(
        &self,
        isbn: &str,
        title: &str,
        author: &str,
        category: &str,
        format: EBookFormat,
        total_licenses: u32,
        download_url: &str,
        operator: &str,
    ) -> EBookResource {
        let ebook = EBookResource {
            audit: AuditMetadata::new(operator),
            isbn: isbn.to_string(),
            title: title.to_string(),
            author: author.to_string(),
            category: category.to_string(),
            format,
            total_licenses,
            active_borrows: 0,
            download_url: download_url.to_string(),
        };
        self.ebooks.write().unwrap().insert(ebook.audit.id, ebook.clone());
        ebook
    }

    pub fn borrow_ebook(
        &self,
        ebook_id: Uuid,
        reader_id: &str,
        loan_days: i64,
        operator: &str,
    ) -> Result<EBookLoanRecord, ExtensibilityError> {
        let mut ebooks = self.ebooks.write().unwrap();
        let ebook = ebooks
            .get_mut(&ebook_id)
            .ok_or_else(|| ExtensibilityError::NotFound("找不到指定電子書資源".to_string()))?;

        if ebook.audit.is_deleted() {
            return Err(ExtensibilityError::ValidationError(
                "該電子書資源已下架".to_string(),
            ));
        }
        if ebook.active_borrows >= ebook.total_licenses {
            return Err(ExtensibilityError::ValidationError(
                "電子書同時線上授權席次已滿，請稍後再試或預約".to_string(),
            ));
        }

        ebook.active_borrows += 1;
        ebook.audit.touch(operator);
        drop(ebooks);

        let record = EBookLoanRecord {
            audit: ImmutableAuditMetadata::new(operator),
            ebook_id,
            reader_id: reader_id.to_string(),
            expires_at: Utc::now() + Duration::days(loan_days),
            returned_at: None,
        };
        self.ebook_loans.write().unwrap().push(record.clone());
        Ok(record)
    }

    // ========================================================================
    // 6. ISBN API 自動取得書籍資訊 (ISBN Metadata Auto-Fetch)
    // ========================================================================

    pub fn fetch_metadata_by_isbn(
        &self,
        isbn: &str,
    ) -> Result<IsbnBookMetadata, ExtensibilityError> {
        let normalized_isbn = isbn.replace('-', "").trim().to_string();
        let cache = self.isbn_catalog_cache.read().unwrap();
        cache
            .get(&normalized_isbn)
            .cloned()
            .ok_or_else(|| ExtensibilityError::NotFound(format!("ISBN API 查無書目資料: {}", isbn)))
    }

    // ========================================================================
    // 7 & 8. Email 與 LINE 通知發送 (Email & LINE Notification Gateway)
    // ========================================================================

    pub fn dispatch_multi_channel_notification(
        &self,
        reader_id: &str,
        event_type: NotificationEventType,
        message: &str,
        operator: &str,
    ) -> Result<Vec<NotificationDispatchLog>, ExtensibilityError> {
        let cards = self.reader_cards.read().unwrap();
        let card = cards
            .get(reader_id)
            .ok_or_else(|| ExtensibilityError::NotFound("找不到讀者聯絡資料".to_string()))?
            .clone();
        drop(cards);

        let mut logs = self.notifications.write().unwrap();
        let mut created = Vec::new();

        if let Some(email) = card.email {
            let log = NotificationDispatchLog {
                audit: ImmutableAuditMetadata::new(operator),
                reader_id: reader_id.to_string(),
                channel: NotificationChannel::Email,
                recipient_address: email,
                event_type: event_type.clone(),
                message_body: message.to_string(),
                delivered: true,
            };
            logs.push(log.clone());
            created.push(log);
        }

        if let Some(line_id) = card.line_user_id {
            let log = NotificationDispatchLog {
                audit: ImmutableAuditMetadata::new(operator),
                reader_id: reader_id.to_string(),
                channel: NotificationChannel::Line,
                recipient_address: line_id,
                event_type,
                message_body: message.to_string(),
                delivered: true,
            };
            logs.push(log.clone());
            created.push(log);
        }

        Ok(created)
    }

    pub fn get_notification_logs(&self) -> Vec<NotificationDispatchLog> {
        self.notifications.read().unwrap().clone()
    }

    // ========================================================================
    // 9. 自助借還機工作站 (Self-Checkout & Self-Return Kiosk)
    // ========================================================================

    pub fn kiosk_self_borrow(
        &self,
        branch_id: Uuid,
        reader_qr_code: &str,
        rfid_epc: &str,
    ) -> Result<SelfCheckoutLog, ExtensibilityError> {
        let branches = self.branches.read().unwrap();
        let branch = branches
            .get(&branch_id)
            .ok_or_else(|| ExtensibilityError::NotFound("分館不存在".to_string()))?;
        if !branch.supports_self_checkout || branch.audit.is_deleted() {
            return Err(ExtensibilityError::ValidationError(
                "此分館未啟用自助借還設備".to_string(),
            ));
        }
        drop(branches);

        let reader = self.lookup_reader_by_qr(reader_qr_code)?;

        let mut tags = self.smart_tags.write().unwrap();
        let copy = tags
            .values_mut()
            .find(|t| !t.audit.is_deleted() && t.rfid_epc == rfid_epc)
            .ok_or_else(|| ExtensibilityError::NotFound("RFID 感應失敗：找不到對應館藏".to_string()))?;

        if copy.branch_id != branch_id {
            return Err(ExtensibilityError::ValidationError(
                "書籍所屬分館與自助機台不符".to_string(),
            ));
        }
        if copy.is_checked_out {
            return Err(ExtensibilityError::ValidationError(
                "該書籍已處於借出狀態".to_string(),
            ));
        }

        copy.is_checked_out = true;
        copy.eas_security_locked = false;
        copy.current_holder_id = Some(reader.reader_id.clone());
        copy.audit.touch("KIOSK_TERMINAL");
        let copy_id = copy.copy_id;
        let title = copy.book_title.clone();
        drop(tags);

        let log = SelfCheckoutLog {
            audit: ImmutableAuditMetadata::new("KIOSK_TERMINAL"),
            branch_id,
            reader_id: reader.reader_id.clone(),
            copy_id,
            rfid_epc: rfid_epc.to_string(),
            action: KioskAction::SelfBorrow,
        };
        self.kiosk_logs.write().unwrap().push(log.clone());

        let _ = self.dispatch_multi_channel_notification(
            &reader.reader_id,
            NotificationEventType::SelfCheckoutReceipt,
            &format!("自助借書成功：《{}》", title),
            "KIOSK_TERMINAL",
        );

        Ok(log)
    }

    pub fn kiosk_self_return(
        &self,
        branch_id: Uuid,
        rfid_epc: &str,
    ) -> Result<SelfCheckoutLog, ExtensibilityError> {
        let mut tags = self.smart_tags.write().unwrap();
        let copy = tags
            .values_mut()
            .find(|t| !t.audit.is_deleted() && t.rfid_epc == rfid_epc)
            .ok_or_else(|| ExtensibilityError::NotFound("RFID 感應失敗：找不到對應館藏".to_string()))?;

        if !copy.is_checked_out {
            return Err(ExtensibilityError::ValidationError(
                "該書籍目前在架，無需歸還".to_string(),
            ));
        }

        let reader_id = copy
            .current_holder_id
            .clone()
            .unwrap_or_else(|| "UNKNOWN".to_string());
        copy.is_checked_out = false;
        copy.eas_security_locked = true;
        copy.current_holder_id = None;
        copy.branch_id = branch_id;
        copy.audit.touch("KIOSK_RETURN_BOX");
        let copy_id = copy.copy_id;
        drop(tags);

        let log = SelfCheckoutLog {
            audit: ImmutableAuditMetadata::new("KIOSK_RETURN_BOX"),
            branch_id,
            reader_id,
            copy_id,
            rfid_epc: rfid_epc.to_string(),
            action: KioskAction::SelfReturn,
        };
        self.kiosk_logs.write().unwrap().push(log.clone());
        Ok(log)
    }

    pub fn get_kiosk_logs(&self) -> Vec<SelfCheckoutLog> {
        self.kiosk_logs.read().unwrap().clone()
    }

    // ========================================================================
    // 10. AI 智慧推薦引擎 (AI Smart Book Recommendation)
    // ========================================================================

    pub fn recommend_books_for_reader(&self, reader_id: &str) -> Vec<RecommendedBookItem> {
        let mut category_weights: HashMap<String, f32> = HashMap::new();

        if let Some(card) = self.reader_cards.read().unwrap().get(reader_id) {
            for cat in &card.preferred_categories {
                *category_weights.entry(cat.clone()).or_insert(0.0) += 1.5;
            }
        }

        let tags = self.smart_tags.read().unwrap();
        let mut read_titles: HashSet<String> = HashSet::new();
        for log in self.kiosk_logs.read().unwrap().iter() {
            if log.reader_id == reader_id {
                if let Some(tag) = tags.get(&log.copy_id) {
                    *category_weights.entry(tag.category.clone()).or_insert(0.0) += 2.5;
                    read_titles.insert(tag.book_title.clone());
                }
            }
        }

        let mut candidates: Vec<RecommendedBookItem> = Vec::new();
        let mut seen_titles: HashSet<String> = HashSet::new();

        for tag in tags.values().filter(|t| !t.audit.is_deleted()) {
            if !read_titles.contains(&tag.book_title) && seen_titles.insert(tag.book_title.clone()) {
                let base_score = category_weights.get(&tag.category).copied().unwrap_or(0.2);
                if base_score >= 1.0 {
                    candidates.push(RecommendedBookItem {
                        title: tag.book_title.clone(),
                        category: tag.category.clone(),
                        score: base_score,
                        reason: format!("AI 依據您的「{}」閱讀軌跡與偏好模型推薦", tag.category),
                    });
                }
            }
        }

        for ebook in self.ebooks.read().unwrap().values().filter(|e| !e.audit.is_deleted()) {
            if !read_titles.contains(&ebook.title) && seen_titles.insert(ebook.title.clone()) {
                let base_score = category_weights.get(&ebook.category).copied().unwrap_or(0.2) + 0.3;
                if base_score >= 1.0 {
                    candidates.push(RecommendedBookItem {
                        title: ebook.title.clone(),
                        category: ebook.category.clone(),
                        score: base_score,
                        reason: format!("AI 推薦熱門「{}」電子書（免等待可立即線上閱讀）", ebook.category),
                    });
                }
            }
        }

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates
    }
}