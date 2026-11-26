use crate::models::operations::*;
use bson::uuid::Uuid;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

pub struct OperationsService {
    pub books: HashMap<Uuid, BookMetadata>,
    pub copies: HashMap<Uuid, BookCopyItem>,
    pub patrons: HashMap<Uuid, PatronProfile>,
    pub loans: Vec<LoanRecord>,
    pub reservations: Vec<ReservationRecord>,
    pub fines: Vec<FineTransaction>,
    pub fine_rules: FineRuleConfig,
}

impl OperationsService {
    pub fn new(fine_rules: FineRuleConfig) -> Self {
        Self {
            books: HashMap::new(),
            copies: HashMap::new(),
            patrons: HashMap::new(),
            loans: Vec::new(),
            reservations: Vec::new(),
            fines: Vec::new(),
            fine_rules,
        }
    }

    // ==========================================
    // A. 館藏管理功能
    // ==========================================
    /// 透過 ISBN 查詢並自動匯入書目資料
    pub fn import_book_by_isbn(
        &mut self,
        isbn: &str,
        title: &str,
        author: &str,
        publisher: &str,
        category: &str,
        cover_url: Option<String>,
        attachments: Vec<String>,
    ) -> BookMetadata {
        if let Some(existing) = self.books.values().find(|b| b.isbn == isbn) {
            return existing.clone();
        }
        let book = BookMetadata {
            id: Uuid::new(),
            isbn: isbn.to_string(),
            title: title.to_string(),
            author: author.to_string(),
            publisher: publisher.to_string(),
            category: category.to_string(),
            cover_url,
            digital_attachments: attachments,
        };
        self.books.insert(book.id, book.clone());
        book
    }

    pub fn update_book_metadata(
        &mut self,
        book_id: Uuid,
        title: &str,
        category: &str,
    ) -> Result<BookMetadata, String> {
        let book = self.books.get_mut(&book_id).ok_or("找不到指定書目")?;
        book.title = title.to_string();
        book.category = category.to_string();
        Ok(book.clone())
    }

    pub fn search_books(&self, keyword: &str) -> Vec<BookMetadata> {
        let kw = keyword.to_lowercase();
        self.books
            .values()
            .filter(|b| {
                b.title.to_lowercase().contains(&kw)
                    || b.author.to_lowercase().contains(&kw)
                    || b.isbn.contains(&kw)
                    || b.category.to_lowercase().contains(&kw)
            })
            .cloned()
            .collect()
    }

    /// 新增館藏副本並自動產生條碼與列印標籤字串
    pub fn add_copy_with_barcode(
        &mut self,
        book_id: Uuid,
        branch_code: &str,
        shelf_location: &str,
    ) -> Result<(BookCopyItem, String), String> {
        if !self.books.contains_key(&book_id) {
            return Err("書目不存在，無法建立副本".into());
        }
        let seq = self.copies.len() + 1;
        let barcode = format!("LIB-{}-{:06}", branch_code, seq);
        let copy = BookCopyItem {
            id: Uuid::new(),
            book_id,
            barcode: barcode.clone(),
            branch_code: branch_code.to_string(),
            shelf_location: shelf_location.to_string(),
            condition: CopyCondition::Available,
        };
        self.copies.insert(copy.id, copy.clone());
        let printable_label = format!("[BARCODE: {} | LOC: {}]", barcode, shelf_location);
        Ok((copy, printable_label))
    }

    /// 更新副本狀態 (遺失、損壞、報廢)
    pub fn update_copy_condition(
        &mut self,
        copy_id: Uuid,
        condition: CopyCondition,
    ) -> Result<(), String> {
        let copy = self.copies.get_mut(&copy_id).ok_or("找不到館藏副本")?;
        copy.condition = condition;
        Ok(())
    }

    // ==========================================
    // B. 讀者管理功能
    // ==========================================
    pub fn register_patron(
        &mut self,
        name: &str,
        category: PatronCategory,
        tier: MembershipTier,
    ) -> PatronProfile {
        let id = Uuid::new();
        let card_number = format!("P-{:06}", self.patrons.len() + 1001);
        let qr_code_payload = format!("QR://PATRON/{}/{}", id, card_number);
        let profile = PatronProfile {
            id,
            name: name.to_string(),
            card_number,
            qr_code_payload,
            category,
            tier,
            is_suspended: false,
            is_blacklisted: false,
            violation_points: 0,
            violations: Vec::new(),
        };
        self.patrons.insert(id, profile.clone());
        profile
    }

    pub fn record_patron_violation(
        &mut self,
        patron_id: Uuid,
        reason: &str,
        points: u32,
        blacklist: bool,
    ) -> Result<(), String> {
        let patron = self.patrons.get_mut(&patron_id).ok_or("找不到讀者資料")?;
        patron.violation_points += points;
        patron.violations.push(ViolationRecord {
            id: Uuid::new(),
            reason: reason.to_string(),
            points,
            recorded_at: Utc::now(),
        });
        if patron.violation_points >= self.fine_rules.auto_suspend_points {
            patron.is_suspended = true;
        }
        if blacklist {
            patron.is_blacklisted = true;
            patron.is_suspended = true;
        }
        Ok(())
    }

    pub fn get_patron_loan_history(&self, patron_id: Uuid) -> Vec<LoanRecord> {
        self.loans
            .iter()
            .filter(|l| l.patron_id == patron_id)
            .cloned()
            .collect()
    }

    // ==========================================
    // C. 借閱管理功能 (一般、批次、跨館、續借、歸還)
    // ==========================================
    pub fn borrow_copy(
        &mut self,
        patron_id: Uuid,
        barcode: &str,
        pickup_branch: &str,
        now: DateTime<Utc>,
    ) -> Result<LoanRecord, String> {
        let patron = self.patrons.get(&patron_id).ok_or("讀者不存在")?.clone();
        if patron.is_blacklisted {
            return Err("讀者已列入黑名單，禁止借閱".into());
        }
        if patron.is_suspended {
            return Err("讀者目前處於停權狀態，無法借閱".into());
        }

        let (max_books, loan_days) = patron.loan_policy();
        let active_loans = self
            .loans
            .iter()
            .filter(|l| l.patron_id == patron_id && l.returned_at.is_none())
            .count();
        if active_loans >= max_books {
            return Err(format!("已達最大借閱數量上限 ({} 冊)", max_books));
        }

        let copy = self
            .copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or("條碼無效")?;

        if copy.condition != CopyCondition::Available && copy.condition != CopyCondition::Reserved {
            return Err("該冊目前不可借閱".into());
        }

        // 若該書為預約保留狀態，確認是否為第一位取書者
        if copy.condition == CopyCondition::Reserved {
            if let Some(res) = self
                .reservations
                .iter_mut()
                .find(|r| r.book_id == copy.book_id && r.status == ReservationStatus::Ready)
            {
                if res.patron_id != patron_id {
                    return Err("此書為其他讀者預約保留中".into());
                }
                res.status = ReservationStatus::Fulfilled;
            }
        }

        copy.condition = CopyCondition::Borrowed;
        let loan = LoanRecord {
            id: Uuid::new(),
            patron_id,
            copy_id: copy.id,
            book_id: copy.book_id,
            origin_branch: copy.branch_code.clone(),
            pickup_branch: pickup_branch.to_string(),
            borrowed_at: now,
            due_date: now + Duration::days(loan_days),
            returned_at: None,
            renewal_count: 0,
        };
        self.loans.push(loan.clone());
        Ok(loan)
    }

    /// 批次掃描條碼借閱
    pub fn batch_borrow(
        &mut self,
        patron_id: Uuid,
        barcodes: &[&str],
        pickup_branch: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<LoanRecord>, String> {
        let mut results = Vec::new();
        for &code in barcodes {
            let record = self.borrow_copy(patron_id, code, pickup_branch, now)?;
            results.push(record);
        }
        Ok(results)
    }

    /// 續借功能 (若有人預約此書則不可續借)
    pub fn renew_loan(&mut self, loan_id: Uuid) -> Result<LoanRecord, String> {
        let loan = self
            .loans
            .iter_mut()
            .find(|l| l.id == loan_id && l.returned_at.is_none())
            .ok_or("找不到進行中的借閱紀錄")?;

        let has_waiting_reservation = self.reservations.iter().any(|r| {
            r.book_id == loan.book_id && r.status == ReservationStatus::Waiting
        });
        if has_waiting_reservation {
            return Err("本書尚有其他讀者預約候補，無法續借".into());
        }
        if loan.renewal_count >= 2 {
            return Err("已達最大續借次數 (2 次)".into());
        }

        loan.due_date += Duration::days(14);
        loan.renewal_count += 1;
        Ok(loan.clone())
    }

    /// 單本歸還 (自動觸發逾期罰款計算與預約到館通知)
    pub fn return_copy(
        &mut self,
        barcode: &str,
        return_time: DateTime<Utc>,
    ) -> Result<(LoanRecord, Option<FineTransaction>), String> {
        let copy_id = self
            .copies
            .values()
            .find(|c| c.barcode == barcode)
            .map(|c| c.id)
            .ok_or("條碼無效")?;

        let loan = self
            .loans
            .iter_mut()
            .find(|l| l.copy_id == copy_id && l.returned_at.is_none())
            .ok_or("找不到未歸還的借閱紀錄")?;

        loan.returned_at = Some(return_time);
        let loan_snapshot = loan.clone();

        // E. 檢查是否逾期並計算罰款
        let mut generated_fine = None;
        if return_time > loan_snapshot.due_date {
            let overdue_days = (return_time - loan_snapshot.due_date).num_days().max(1);
            let raw_fee = overdue_days as f64 * self.fine_rules.daily_rate;
            let capped_fee = raw_fee.min(self.fine_rules.max_fine_per_loan);

            let fine = FineTransaction {
                id: Uuid::new(),
                patron_id: loan_snapshot.patron_id,
                loan_id: loan_snapshot.id,
                overdue_days,
                original_amount: raw_fee,
                final_amount: capped_fee,
                status: FineStatus::Unpaid,
                note: Some("系統自動計算逾期罰款".into()),
            };
            self.fines.push(fine.clone());
            generated_fine = Some(fine);
        }

        // D. 檢查是否有預約候補，若有則轉為預約保留並發送通知
        if let Some(next_res) = self
            .reservations
            .iter_mut()
            .filter(|r| r.book_id == loan_snapshot.book_id && r.status == ReservationStatus::Waiting)
            .min_by_key(|r| r.queue_position)
        {
            next_res.status = ReservationStatus::Ready;
            next_res.pickup_deadline = Some(return_time + Duration::days(3));
            next_res.notified = true;
            if let Some(copy) = self.copies.get_mut(&copy_id) {
                copy.condition = CopyCondition::Reserved;
            }
        } else if let Some(copy) = self.copies.get_mut(&copy_id) {
            copy.condition = CopyCondition::Available;
        }

        Ok((loan_snapshot, generated_fine))
    }

    /// 批次歸還
    pub fn batch_return(
        &mut self,
        barcodes: &[&str],
        return_time: DateTime<Utc>,
    ) -> Result<Vec<(LoanRecord, Option<FineTransaction>)>, String> {
        let mut results = Vec::new();
        for &code in barcodes {
            results.push(self.return_copy(code, return_time)?);
        }
        Ok(results)
    }

    // ==========================================
    // D. 預約管理功能
    // ==========================================
    pub fn create_reservation(
        &mut self,
        patron_id: Uuid,
        book_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ReservationRecord, String> {
        if !self.patrons.contains_key(&patron_id) {
            return Err("讀者不存在".into());
        }
        let current_queue = self
            .reservations
            .iter()
            .filter(|r| r.book_id == book_id && r.status == ReservationStatus::Waiting)
            .count();

        let res = ReservationRecord {
            id: Uuid::new(),
            patron_id,
            book_id,
            queue_position: current_queue + 1,
            status: ReservationStatus::Waiting,
            created_at: now,
            pickup_deadline: None,
            notified: false,
        };
        self.reservations.push(res.clone());
        Ok(res)
    }

    pub fn cancel_reservation(&mut self, reservation_id: Uuid) -> Result<(), String> {
        let target = self
            .reservations
            .iter_mut()
            .find(|r| r.id == reservation_id)
            .ok_or("找不到預約紀錄")?;
        let book_id = target.book_id;
        target.status = ReservationStatus::Cancelled;

        // 重新整理候補排序
        let mut pos = 1;
        for r in self
            .reservations
            .iter_mut()
            .filter(|r| r.book_id == book_id && r.status == ReservationStatus::Waiting)
        {
            r.queue_position = pos;
            pos += 1;
        }
        Ok(())
    }

    /// 掃描並自動取消超過取書期限的預約
    pub fn process_expired_reservations(&mut self, current_time: DateTime<Utc>) -> usize {
        let mut expired_count = 0;
        let mut freed_books = Vec::new();

        for r in self.reservations.iter_mut() {
            if r.status == ReservationStatus::Ready {
                if let Some(deadline) = r.pickup_deadline {
                    if current_time > deadline {
                        r.status = ReservationStatus::Expired;
                        freed_books.push(r.book_id);
                        expired_count += 1;
                    }
                }
            }
        }

        // 將逾期未取的保留副本釋放回可借閱狀態
        for book_id in freed_books {
            if let Some(copy) = self
                .copies
                .values_mut()
                .find(|c| c.book_id == book_id && c.condition == CopyCondition::Reserved)
            {
                copy.condition = CopyCondition::Available;
            }
        }
        expired_count
    }

    // ==========================================
    // E. 逾期與罰款管理功能 (繳費、減免、退款)
    // ==========================================
    pub fn pay_fine(&mut self, fine_id: Uuid) -> Result<FineTransaction, String> {
        let fine = self
            .fines
            .iter_mut()
            .find(|f| f.id == fine_id)
            .ok_or("找不到罰款單")?;
        fine.status = FineStatus::Paid;
        Ok(fine.clone())
    }

    pub fn waive_fine(&mut self, fine_id: Uuid, reason: &str) -> Result<FineTransaction, String> {
        let fine = self
            .fines
            .iter_mut()
            .find(|f| f.id == fine_id)
            .ok_or("找不到罰款單")?;
        fine.final_amount = 0.0;
        fine.status = FineStatus::Waived;
        fine.note = Some(format!("減免原因: {}", reason));
        Ok(fine.clone())
    }

    pub fn refund_fine(&mut self, fine_id: Uuid, reason: &str) -> Result<FineTransaction, String> {
        let fine = self
            .fines
            .iter_mut()
            .find(|f| f.id == fine_id && f.status == FineStatus::Paid)
            .ok_or("僅已繳費的罰款可執行退款")?;
        fine.status = FineStatus::Refunded;
        fine.note = Some(format!("退款原因: {}", reason));
        Ok(fine.clone())
    }

    // ==========================================
    // F. 統計分析 Dashboard
    // ==========================================
    pub fn generate_dashboard_stats(&self, reference_now: DateTime<Utc>) -> LibraryDashboardStats {
        let today = reference_now.date_naive();

        let total_copies = self
            .copies
            .values()
            .filter(|c| c.condition != CopyCondition::Discarded)
            .count();

        let available_copies = self
            .copies
            .values()
            .filter(|c| c.condition == CopyCondition::Available)
            .count();

        let borrowed_copies = self
            .copies
            .values()
            .filter(|c| c.condition == CopyCondition::Borrowed)
            .count();

        let today_borrows = self
            .loans
            .iter()
            .filter(|l| l.borrowed_at.date_naive() == today)
            .count();

        let today_returns = self
            .loans
            .iter()
            .filter(|l| {
                l.returned_at
                    .map(|rt| rt.date_naive() == today)
                    .unwrap_or(false)
            })
            .count();

        let overdue_unreturned = self
            .loans
            .iter()
            .filter(|l| l.returned_at.is_none() && reference_now > l.due_date)
            .count();

        let waiting_reservations = self
            .reservations
            .iter()
            .filter(|r| r.status == ReservationStatus::Waiting)
            .count();

        // 熱門書籍、分類、讀者與每月趨勢統計
        let mut book_counts: HashMap<Uuid, usize> = HashMap::new();
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        let mut patron_counts: HashMap<Uuid, usize> = HashMap::new();
        let mut month_counts: HashMap<String, usize> = HashMap::new();

        for loan in &self.loans {
            *book_counts.entry(loan.book_id).or_insert(0) += 1;
            *patron_counts.entry(loan.patron_id).or_insert(0) += 1;

            if let Some(book) = self.books.get(&loan.book_id) {
                *category_counts.entry(book.category.clone()).or_insert(0) += 1;
            }
            let ym = loan.borrowed_at.format("%Y-%m").to_string();
            *month_counts.entry(ym).or_insert(0) += 1;
        }

        let mut top_books: Vec<RankedMetric> = book_counts
            .into_iter()
            .filter_map(|(id, count)| {
                self.books.get(&id).map(|b| RankedMetric {
                    label: b.title.clone(),
                    count,
                })
            })
            .collect();
        top_books.sort_by(|a, b| b.count.cmp(&a.count));

        let mut top_categories: Vec<RankedMetric> = category_counts
            .into_iter()
            .map(|(label, count)| RankedMetric { label, count })
            .collect();
        top_categories.sort_by(|a, b| b.count.cmp(&a.count));

        let mut top_patrons: Vec<RankedMetric> = patron_counts
            .into_iter()
            .filter_map(|(id, count)| {
                self.patrons.get(&id).map(|p| RankedMetric {
                    label: p.name.clone(),
                    count,
                })
            })
            .collect();
        top_patrons.sort_by(|a, b| b.count.cmp(&a.count));

        let mut monthly_trends: Vec<(String, usize)> = month_counts.into_iter().collect();
        monthly_trends.sort_by(|a, b| a.0.cmp(&b.0));

        let collection_utilization_rate = if total_copies > 0 {
            ((borrowed_copies as f64 / total_copies as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };

        LibraryDashboardStats {
            total_copies,
            available_copies,
            borrowed_copies,
            today_borrows,
            today_returns,
            overdue_unreturned,
            waiting_reservations,
            top_books,
            top_categories,
            top_patrons,
            monthly_trends,
            collection_utilization_rate,
        }
    }
}