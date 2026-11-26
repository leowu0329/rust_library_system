use crate::models::exception_handling::*;
use bson::uuid::Uuid;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use thiserror::Error;

/// 十大異常情境專用錯誤定義
#[derive(Debug, Error, PartialEq)]
pub enum CirculationException {
    #[error("找不到讀者資料")]
    ReaderNotFound,

    #[error("讀者已停權，禁止借閱")]
    ReaderSuspended,

    #[error("讀者尚有逾期未歸還書籍，依規則限制借閱")]
    ReaderHasOverdueBooks,

    #[error("已達最大借閱冊數上限 ({0} 冊)，禁止新增借閱")]
    BorrowLimitReached(usize),

    #[error("找不到館藏副本")]
    CopyNotFound,

    #[error("書籍已被借出，禁止重複借閱")]
    CopyAlreadyBorrowed,

    #[error("書籍已被其他讀者預約保留，請依候補順序取書")]
    CopyReservedForAnotherReader,

    #[error("該副本目前無法借閱（狀態：{0:?}）")]
    CopyUnavailable(CopyState),

    #[error("書籍已遺失，無法執行借閱")]
    CopyLost,

    #[error("書籍損壞維修中，無法執行借閱")]
    CopyInMaintenance,

    #[error("讀者已借閱或正在預約此書，防止重複建立預約")]
    DuplicateReservation,

    #[error("找不到進行中的借閱紀錄")]
    ActiveBorrowNotFound,

    #[error("歸還交易執行期間發生系統中斷，已回滾所有狀態變更: {0}")]
    TransactionAborted(String),
}

/// Email 發送介面（支援正式 SMTP 實作與測試故障注入）
pub trait EmailSender {
    fn send(&mut self, to: &str, subject: &str, body: &str) -> Result<(), String>;
}

/// 模擬 Email 閘道器，可控制是否模擬網路／SMTP 異常
pub struct MockEmailGateway {
    pub should_fail: bool,
    pub sent_messages: Vec<(String, String)>,
}

impl MockEmailGateway {
    pub fn new(should_fail: bool) -> Self {
        Self {
            should_fail,
            sent_messages: Vec::new(),
        }
    }
}

impl EmailSender for MockEmailGateway {
    fn send(&mut self, to: &str, subject: &str, _body: &str) -> Result<(), String> {
        if self.should_fail {
            Err("SMTP 連線逾時 (504 Gateway Timeout)".into())
        } else {
            self.sent_messages.push((to.to_string(), subject.to_string()));
            Ok(())
        }
    }
}

/// 可原子快照與回滾的流通狀態儲存庫
#[derive(Clone, Default)]
pub struct CirculationState {
    pub readers: HashMap<Uuid, Reader>,
    pub books: HashMap<Uuid, CatalogBook>,
    pub copies: HashMap<Uuid, PhysicalCopy>,
    pub borrows: Vec<BorrowRecord>,
    pub holds: Vec<HoldRecord>,
    pub incident_logs: Vec<CopyIncidentLog>,
    pub email_logs: Vec<EmailNotificationLog>,
}

pub struct ExceptionHandlingService {
    pub state: CirculationState,
    pub default_loan_days: i64,
    pub pickup_hold_days: i64,
    pub simulate_return_crash: bool, // 用於模擬歸還中途系統崩潰
}

impl ExceptionHandlingService {
    pub fn new() -> Self {
        Self {
            state: CirculationState::default(),
            default_loan_days: 14,
            pickup_hold_days: 3,
            simulate_return_crash: false,
        }
    }

    /// 借閱書籍（集中檢核：停權、逾期、上限、借出中、預約保留、遺失、損壞）
    pub fn checkout(
        &mut self,
        reader_id: Uuid,
        barcode: &str,
        now: DateTime<Utc>,
    ) -> Result<BorrowRecord, CirculationException> {
        // 1. 檢查讀者是否存在與停權狀態
        let reader = self
            .state
            .readers
            .get(&reader_id)
            .ok_or(CirculationException::ReaderNotFound)?
            .clone();

        if reader.status == ReaderStatus::Suspended {
            return Err(CirculationException::ReaderSuspended);
        }

        // 2. 檢查讀者是否有逾期未還書籍
        if !reader.allow_borrow_with_overdue {
            let has_overdue = self.state.borrows.iter().any(|b| {
                b.reader_id == reader_id && b.returned_at.is_none() && now > b.due_date
            });
            if has_overdue {
                return Err(CirculationException::ReaderHasOverdueBooks);
            }
        }

        // 3. 檢查是否已達借閱上限
        let active_count = self
            .state
            .borrows
            .iter()
            .filter(|b| b.reader_id == reader_id && b.returned_at.is_none())
            .count();
        if active_count >= reader.max_borrow_limit {
            return Err(CirculationException::BorrowLimitReached(
                reader.max_borrow_limit,
            ));
        }

        // 4. 檢查館藏副本狀態（防止重複借出、攔截遺失／損壞／他人預約）
        let copy = self
            .state
            .copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(CirculationException::CopyNotFound)?;

        match copy.state {
            CopyState::Borrowed => return Err(CirculationException::CopyAlreadyBorrowed),
            CopyState::Lost => return Err(CirculationException::CopyLost),
            CopyState::Maintenance => return Err(CirculationException::CopyInMaintenance),
            CopyState::Reserved => {
                // 僅允許該副本指定的第一順位預約者取書
                let hold = self
                    .state
                    .holds
                    .iter_mut()
                    .find(|h| {
                        h.assigned_copy_id == Some(copy.id) && h.status == HoldStatus::Ready
                    })
                    .ok_or(CirculationException::CopyReservedForAnotherReader)?;

                if hold.reader_id != reader_id {
                    return Err(CirculationException::CopyReservedForAnotherReader);
                }
                hold.status = HoldStatus::Fulfilled;
            }
            CopyState::Available => {}
        }

        // 5. 更新副本狀態並建立借閱紀錄
        copy.state = CopyState::Borrowed;
        let record = BorrowRecord {
            id: Uuid::new(),
            reader_id,
            copy_id: copy.id,
            book_id: copy.book_id,
            borrowed_at: now,
            due_date: now + Duration::days(self.default_loan_days),
            returned_at: None,
        };
        self.state.borrows.push(record.clone());
        Ok(record)
    }

    /// 建立預約（防止同一讀者重複預約或已借閱同一本書）
    pub fn place_hold(
        &mut self,
        reader_id: Uuid,
        book_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<HoldRecord, CirculationException> {
        let reader = self
            .state
            .readers
            .get(&reader_id)
            .ok_or(CirculationException::ReaderNotFound)?;

        if reader.status == ReaderStatus::Suspended {
            return Err(CirculationException::ReaderSuspended);
        }

        // 檢查是否已經借閱同一本書尚未歸還
        let already_borrowed = self.state.borrows.iter().any(|b| {
            b.reader_id == reader_id && b.book_id == book_id && b.returned_at.is_none()
        });
        if already_borrowed {
            return Err(CirculationException::DuplicateReservation);
        }

        // 檢查是否已有進行中的預約（Waiting 或 Ready）
        let already_holding = self.state.holds.iter().any(|h| {
            h.reader_id == reader_id
                && h.book_id == book_id
                && matches!(h.status, HoldStatus::Waiting | HoldStatus::Ready)
        });
        if already_holding {
            return Err(CirculationException::DuplicateReservation);
        }

        let queue_order = self
            .state
            .holds
            .iter()
            .filter(|h| h.book_id == book_id && h.status == HoldStatus::Waiting)
            .count()
            + 1;

        let hold = HoldRecord {
            id: Uuid::new(),
            reader_id,
            book_id,
            assigned_copy_id: None,
            queue_order,
            status: HoldStatus::Waiting,
            created_at: now,
            pickup_deadline: None,
        };
        self.state.holds.push(hold.clone());
        Ok(hold)
    }

    /// 書籍遺失處理：更新館藏為 Lost，結束借閱並寫入永久稽核與賠償紀錄
    pub fn report_lost(
        &mut self,
        barcode: &str,
        reader_id: Option<Uuid>,
        compensation_fee: u32,
        now: DateTime<Utc>,
    ) -> Result<CopyIncidentLog, CirculationException> {
        let copy = self
            .state
            .copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(CirculationException::CopyNotFound)?;

        copy.state = CopyState::Lost;
        copy.note = Some(format!("於 {} 通報遺失", now.format("%Y-%m-%d")));
        let copy_id = copy.id;

        // 若該副本處於借出狀態，標記歸還時間以結算借閱（避免卡死借閱額度，改由賠償程序追蹤）
        if let Some(borrow) = self
            .state
            .borrows
            .iter_mut()
            .find(|b| b.copy_id == copy_id && b.returned_at.is_none())
        {
            borrow.returned_at = Some(now);
        }

        let log = CopyIncidentLog {
            id: Uuid::new(),
            copy_id,
            reader_id,
            incident_type: IncidentType::Lost,
            description: format!("條碼 {} 確認遺失，已保留館藏歷史紀錄並開立賠償", barcode),
            compensation_fee,
            recorded_at: now,
        };
        self.state.incident_logs.push(log.clone());
        Ok(log)
    }

    /// 書籍損壞處理：更新館藏進入 Maintenance 維修／待處理狀態，並保留紀錄
    pub fn report_damaged(
        &mut self,
        barcode: &str,
        reader_id: Option<Uuid>,
        damage_detail: &str,
        repair_fee: u32,
        now: DateTime<Utc>,
    ) -> Result<CopyIncidentLog, CirculationException> {
        let copy = self
            .state
            .copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(CirculationException::CopyNotFound)?;

        copy.state = CopyState::Maintenance;
        copy.note = Some(format!("損壞待修：{}", damage_detail));
        let copy_id = copy.id;

        let log = CopyIncidentLog {
            id: Uuid::new(),
            copy_id,
            reader_id,
            incident_type: IncidentType::Damaged,
            description: damage_detail.to_string(),
            compensation_fee: repair_fee,
            recorded_at: now,
        };
        self.state.incident_logs.push(log.clone());
        Ok(log)
    }

    /// 歸還書籍（支援交易原子性回滾、候補轉保留、以及 Email 通知失敗重試紀錄）
    pub fn return_book_transactional<E: EmailSender>(
        &mut self,
        barcode: &str,
        return_time: DateTime<Utc>,
        email_sender: &mut E,
    ) -> Result<BorrowRecord, CirculationException> {
        // 建立交易前完整快照，確保任何步驟失敗或系統中斷皆可原子回滾
        let snapshot = self.state.clone();

        let tx_result = self.execute_return_steps(barcode, return_time, email_sender);
        match tx_result {
            Ok(record) => Ok(record),
            Err(err) => {
                // 發生異常時還原狀態，保證「借閱紀錄」與「副本狀態」不會出現半套更新
                self.state = snapshot;
                Err(err)
            }
        }
    }

    fn execute_return_steps<E: EmailSender>(
        &mut self,
        barcode: &str,
        return_time: DateTime<Utc>,
        email_sender: &mut E,
    ) -> Result<BorrowRecord, CirculationException> {
        let copy_id = self
            .state
            .copies
            .values()
            .find(|c| c.barcode == barcode)
            .map(|c| c.id)
            .ok_or(CirculationException::CopyNotFound)?;

        // 步驟 1：更新借閱紀錄歸還時間
        let borrow = self
            .state
            .borrows
            .iter_mut()
            .find(|b| b.copy_id == copy_id && b.returned_at.is_none())
            .ok_or(CirculationException::ActiveBorrowNotFound)?;

        borrow.returned_at = Some(return_time);
        let completed_borrow = borrow.clone();

        // 模擬交易執行到一半時發生系統中斷（斷電 / DB 寫入斷線）
        if self.simulate_return_crash {
            return Err(CirculationException::TransactionAborted(
                "寫入館藏狀態前發生 I/O 中斷".into(),
            ));
        }

        // 步驟 2：檢查是否有預約候補者；若有則轉為 Reserved 並觸發通知，否則轉為 Available
        let next_hold_opt = self
            .state
            .holds
            .iter_mut()
            .filter(|h| h.book_id == completed_borrow.book_id && h.status == HoldStatus::Waiting)
            .min_by_key(|h| h.queue_order);

        if let Some(next_hold) = next_hold_opt {
            next_hold.status = HoldStatus::Ready;
            next_hold.assigned_copy_id = Some(copy_id);
            next_hold.pickup_deadline = Some(return_time + Duration::days(self.pickup_hold_days));
            let notify_reader_id = next_hold.reader_id;

            if let Some(copy) = self.state.copies.get_mut(&copy_id) {
                copy.state = CopyState::Reserved;
            }

            // 步驟 3：發送預約到館 Email；若發送失敗，寫入 PendingRetry 佇列而不影響歸還交易本身
            if let Some(reader) = self.state.readers.get(&notify_reader_id).cloned() {
                let book_title = self
                    .state
                    .books
                    .get(&completed_borrow.book_id)
                    .map(|b| b.title.clone())
                    .unwrap_or_else(|| "預約書籍".into());
                let subject = format!("【圖書館通知】您預約的《{}》已到館", book_title);
                let body = format!("請於 {} 前至櫃台取書。", return_time + Duration::days(3));

                self.dispatch_email_with_retry_queue(
                    &reader.email,
                    &subject,
                    &body,
                    return_time,
                    email_sender,
                );
            }
        } else if let Some(copy) = self.state.copies.get_mut(&copy_id) {
            copy.state = CopyState::Available;
        }

        Ok(completed_borrow)
    }

    /// 發送 Email 並於失敗時記錄重試任務
    fn dispatch_email_with_retry_queue<E: EmailSender>(
        &mut self,
        to: &str,
        subject: &str,
        body: &str,
        now: DateTime<Utc>,
        email_sender: &mut E,
    ) {
        match email_sender.send(to, subject, body) {
            Ok(_) => {
                self.state.email_logs.push(EmailNotificationLog {
                    id: Uuid::new(),
                    recipient_email: to.to_string(),
                    subject: subject.to_string(),
                    body: body.to_string(),
                    status: NotificationStatus::Sent,
                    retry_count: 0,
                    max_retries: 3,
                    last_error: None,
                    next_retry_at: None,
                    created_at: now,
                });
            }
            Err(err_msg) => {
                self.state.email_logs.push(EmailNotificationLog {
                    id: Uuid::new(),
                    recipient_email: to.to_string(),
                    subject: subject.to_string(),
                    body: body.to_string(),
                    status: NotificationStatus::PendingRetry,
                    retry_count: 1,
                    max_retries: 3,
                    last_error: Some(err_msg),
                    next_retry_at: Some(now + Duration::minutes(5)),
                    created_at: now,
                });
            }
        }
    }

    /// 背景排程重試失敗的 Email 通知
    pub fn process_email_retry_queue<E: EmailSender>(
        &mut self,
        now: DateTime<Utc>,
        email_sender: &mut E,
    ) -> usize {
        let mut success_count = 0;
        for log in self.state.email_logs.iter_mut() {
            if log.status == NotificationStatus::PendingRetry
                && log.next_retry_at.map(|t| now >= t).unwrap_or(true)
            {
                match email_sender.send(&log.recipient_email, &log.subject, &log.body) {
                    Ok(_) => {
                        log.status = NotificationStatus::Sent;
                        log.last_error = None;
                        log.next_retry_at = None;
                        success_count += 1;
                    }
                    Err(err) => {
                        log.retry_count += 1;
                        log.last_error = Some(err);
                        if log.retry_count >= log.max_retries {
                            log.status = NotificationStatus::DeadLetter;
                            log.next_retry_at = None;
                        } else {
                            log.next_retry_at =
                                Some(now + Duration::minutes(5 * log.retry_count as i64));
                        }
                    }
                }
            }
        }
        success_count
    }
}
