use crate::models::book::{Book, BookCopy, CopyStatus};
use crate::models::circulation::{Loan, LoanStatus, Reservation, ReservationStatus};
use crate::models::user::User;
use bson::{oid::ObjectId, DateTime as BsonDateTime};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LibraryError {
    #[error("找不到指定的讀者")]
    UserNotFound,
    #[error("讀者借閱資格無效（帳號停權或有未繳罰金）")]
    IneligibleUser,
    #[error("已達最大借閱數量上限")]
    BorrowLimitExceeded,
    #[error("已達最大預約數量上限")]
    ReserveLimitExceeded,
    #[error("找不到指定的書目或館藏副本")]
    NotFound,
    #[error("該館藏副本目前無法借閱（狀態：{0:?}）")]
    CopyNotAvailable(CopyStatus),
    #[error("該書籍尚有在館副本，請直接借閱無需預約")]
    CopiesAvailableNoNeedToReserve,
    #[error("您已借閱或正在預約此書，不可重複預約")]
    DuplicateReservation,
    #[error("此副本已保留給其他預約讀者")]
    ReservedByAnotherUser,
    #[error("找不到有效的借閱紀錄")]
    ActiveLoanNotFound,
}

#[derive(Default, Clone)]
pub struct LibraryStore {
    pub users: Arc<RwLock<HashMap<ObjectId, User>>>,
    pub books: Arc<RwLock<HashMap<ObjectId, Book>>>,
    pub copies: Arc<RwLock<HashMap<ObjectId, BookCopy>>>,
    pub loans: Arc<RwLock<Vec<Loan>>>,
    pub reservations: Arc<RwLock<Vec<Reservation>>>,
}

pub struct LibraryService {
    pub store: LibraryStore,
    pub loan_days: i64,
    pub daily_fine: u32,
    pub pickup_window_days: i64,
}

impl LibraryService {
    pub fn new(store: LibraryStore) -> Self {
        Self {
            store,
            loan_days: 14,
            daily_fine: 5,
            pickup_window_days: 3,
        }
    }

    // =========================================================================
    // 1. 圖書與館藏管理流程 (書目與副本分離)
    // =========================================================================

    pub fn create_book(
        &self,
        isbn: &str,
        title: &str,
        author: &str,
        publisher: &str,
        category: &str,
        tags: Vec<String>,
    ) -> Book {
        let now = BsonDateTime::now();
        let id = ObjectId::new();
        let book = Book {
            id: Some(id),
            isbn: isbn.to_string(),
            title: title.to_string(),
            author: author.to_string(),
            publisher: publisher.to_string(),
            category: category.to_string(),
            tags,
            total_copies: 0,
            available_copies: 0,
            created_at: now,
            updated_at: now,
        };
        self.store.books.write().unwrap().insert(id, book.clone());
        book
    }

    pub fn add_book_copy(
        &self,
        book_id: ObjectId,
        barcode: &str,
        location: &str,
    ) -> Result<BookCopy, LibraryError> {
        let mut books = self.store.books.write().unwrap();
        let book = books.get_mut(&book_id).ok_or(LibraryError::NotFound)?;

        let copy_id = ObjectId::new();
        let copy = BookCopy {
            id: Some(copy_id),
            book_id,
            isbn: book.isbn.clone(),
            barcode: barcode.to_string(),
            location: location.to_string(),
            status: CopyStatus::Available,
            procurement_id: ObjectId::new(), // 手動編目入館時產生獨立編目批次 ID
            created_at: BsonDateTime::now(),
        };

        book.total_copies += 1;
        book.available_copies += 1;
        book.updated_at = BsonDateTime::now();

        self.store.copies.write().unwrap().insert(copy_id, copy.clone());
        Ok(copy)
    }

    // =========================================================================
    // 2. 借閱與歸還流程
    // =========================================================================

    pub fn borrow_book(
        &self,
        card_number: &str,
        barcode: &str,
        now: DateTime<Utc>,
    ) -> Result<Loan, LibraryError> {
        let user = self
            .store
            .users
            .read()
            .unwrap()
            .values()
            .find(|u| u.card_number == card_number)
            .cloned()
            .ok_or(LibraryError::UserNotFound)?;

        let user_id = user.id.ok_or(LibraryError::UserNotFound)?;

        if !user.is_active || user.unpaid_fines > 0 {
            return Err(LibraryError::IneligibleUser);
        }

        let active_loans_count = self
            .store
            .loans
            .read()
            .unwrap()
            .iter()
            .filter(|l| l.user_id == user_id && l.status == LoanStatus::Active)
            .count();

        if active_loans_count >= user.max_borrow_limit {
            return Err(LibraryError::BorrowLimitExceeded);
        }

        let mut copies = self.store.copies.write().unwrap();
        let copy = copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(LibraryError::NotFound)?;
        let copy_id = copy.id.ok_or(LibraryError::NotFound)?;

        let mut reservations = self.store.reservations.write().unwrap();

        match copy.status {
            CopyStatus::Available => {
                if let Some(book) = self.store.books.write().unwrap().get_mut(&copy.book_id) {
                    book.available_copies = (book.available_copies - 1).max(0);
                }
            }
            CopyStatus::Reserved => {
                let res = reservations
                    .iter_mut()
                    .find(|r| {
                        r.assigned_copy_id == Some(copy_id)
                            && r.status == ReservationStatus::Ready
                    })
                    .ok_or(LibraryError::ReservedByAnotherUser)?;

                if res.user_id != user_id {
                    return Err(LibraryError::ReservedByAnotherUser);
                }
                res.status = ReservationStatus::Fulfilled;
            }
            ref other => return Err(LibraryError::CopyNotAvailable(other.clone())),
        }

        copy.status = CopyStatus::Borrowed;
        let loan = Loan {
            id: Some(ObjectId::new()),
            user_id,
            copy_id,
            book_id: copy.book_id,
            borrowed_at: now,
            due_date: now + Duration::days(self.loan_days),
            returned_at: None,
            fine_amount: 0,
            status: LoanStatus::Active,
        };

        self.store.loans.write().unwrap().push(loan.clone());
        Ok(loan)
    }

    pub fn return_book(
        &self,
        barcode: &str,
        return_time: DateTime<Utc>,
    ) -> Result<(Loan, Option<Reservation>), LibraryError> {
        let mut copies = self.store.copies.write().unwrap();
        let copy = copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(LibraryError::NotFound)?;
        let copy_id = copy.id.ok_or(LibraryError::NotFound)?;

        let mut loans = self.store.loans.write().unwrap();
        let loan = loans
            .iter_mut()
            .find(|l| l.copy_id == copy_id && l.status == LoanStatus::Active)
            .ok_or(LibraryError::ActiveLoanNotFound)?;

        loan.returned_at = Some(return_time);
        if return_time > loan.due_date {
            let overdue_days = (return_time - loan.due_date).num_days().max(1) as u32;
            loan.fine_amount = overdue_days * self.daily_fine;
            loan.status = LoanStatus::Overdue;

            if let Some(user) = self.store.users.write().unwrap().get_mut(&loan.user_id) {
                user.unpaid_fines += loan.fine_amount;
            }
        } else {
            loan.status = LoanStatus::Returned;
        }

        let mut reservations = self.store.reservations.write().unwrap();
        let next_res = Self::assign_next_reservation(
            &mut reservations,
            copy,
            return_time,
            self.pickup_window_days,
        );

        if next_res.is_none() {
            if let Some(book) = self.store.books.write().unwrap().get_mut(&copy.book_id) {
                book.available_copies += 1;
            }
        }

        Ok((loan.clone(), next_res))
    }

    // =========================================================================
    // 3. 預約與候補流程
    // =========================================================================

    pub fn reserve_book(
        &self,
        user_id: ObjectId,
        book_id: ObjectId,
        now: DateTime<Utc>,
    ) -> Result<Reservation, LibraryError> {
        let user = self
            .store
            .users
            .read()
            .unwrap()
            .get(&user_id)
            .cloned()
            .ok_or(LibraryError::UserNotFound)?;

        if !user.is_active || user.unpaid_fines > 0 {
            return Err(LibraryError::IneligibleUser);
        }

        if !self.store.books.read().unwrap().contains_key(&book_id) {
            return Err(LibraryError::NotFound);
        }

        let has_available_copy = self
            .store
            .copies
            .read()
            .unwrap()
            .values()
            .any(|c| c.book_id == book_id && c.status == CopyStatus::Available);

        if has_available_copy {
            return Err(LibraryError::CopiesAvailableNoNeedToReserve);
        }

        let already_borrowed = self
            .store
            .loans
            .read()
            .unwrap()
            .iter()
            .any(|l| l.user_id == user_id && l.book_id == book_id && l.status == LoanStatus::Active);

        if already_borrowed {
            return Err(LibraryError::DuplicateReservation);
        }

        let mut reservations = self.store.reservations.write().unwrap();

        let already_reserved = reservations.iter().any(|r| {
            r.user_id == user_id
                && r.book_id == book_id
                && (r.status == ReservationStatus::Waiting || r.status == ReservationStatus::Ready)
        });

        if already_reserved {
            return Err(LibraryError::DuplicateReservation);
        }

        let active_res_count = reservations
            .iter()
            .filter(|r| {
                r.user_id == user_id
                    && (r.status == ReservationStatus::Waiting
                        || r.status == ReservationStatus::Ready)
            })
            .count();

        if active_res_count >= user.max_reserve_limit {
            return Err(LibraryError::ReserveLimitExceeded);
        }

        let reservation = Reservation {
            id: Some(ObjectId::new()),
            user_id,
            book_id,
            assigned_copy_id: None,
            created_at: now,
            expires_at: None,
            status: ReservationStatus::Waiting,
        };

        reservations.push(reservation.clone());
        Ok(reservation)
    }

    pub fn process_expired_reservations(&self, now: DateTime<Utc>) -> Vec<Reservation> {
        let mut copies = self.store.copies.write().unwrap();
        let mut reservations = self.store.reservations.write().unwrap();
        let mut newly_notified = Vec::new();

        let expired_indices: Vec<usize> = reservations
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.status == ReservationStatus::Ready
                    && r.expires_at.map(|exp| now > exp).unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect();

        for idx in expired_indices {
            reservations[idx].status = ReservationStatus::Expired;
            if let Some(copy_id) = reservations[idx].assigned_copy_id.take() {
                if let Some(copy) = copies.get_mut(&copy_id) {
                    if let Some(next_res) = Self::assign_next_reservation(
                        &mut reservations,
                        copy,
                        now,
                        self.pickup_window_days,
                    ) {
                        newly_notified.push(next_res);
                    } else if let Some(book) = self.store.books.write().unwrap().get_mut(&copy.book_id) {
                        book.available_copies += 1;
                    }
                }
            }
        }

        newly_notified
    }

    pub fn report_copy_damaged(
        &self,
        barcode: &str,
        now: DateTime<Utc>,
    ) -> Result<(), LibraryError> {
        let mut copies = self.store.copies.write().unwrap();
        let target_copy = copies
            .values_mut()
            .find(|c| c.barcode == barcode)
            .ok_or(LibraryError::NotFound)?;

        let damaged_copy_id = target_copy.id.ok_or(LibraryError::NotFound)?;
        let book_id = target_copy.book_id;
        let prev_status = target_copy.status.clone();
        target_copy.status = CopyStatus::Damaged;

        if prev_status == CopyStatus::Available {
            if let Some(book) = self.store.books.write().unwrap().get_mut(&book_id) {
                book.available_copies = (book.available_copies - 1).max(0);
            }
        }

        let mut reservations = self.store.reservations.write().unwrap();

        if let Some(affected_res) = reservations.iter_mut().find(|r| {
            r.assigned_copy_id == Some(damaged_copy_id) && r.status == ReservationStatus::Ready
        }) {
            if let Some(replacement_copy) = copies
                .values_mut()
                .find(|c| c.book_id == book_id && c.status == CopyStatus::Available)
            {
                replacement_copy.status = CopyStatus::Reserved;
                affected_res.assigned_copy_id = replacement_copy.id;
                affected_res.expires_at = Some(now + Duration::days(self.pickup_window_days));

                if let Some(book) = self.store.books.write().unwrap().get_mut(&book_id) {
                    book.available_copies = (book.available_copies - 1).max(0);
                }
            } else {
                affected_res.status = ReservationStatus::Waiting;
                affected_res.assigned_copy_id = None;
                affected_res.expires_at = None;
            }
        }

        let has_any_healthy_copy = copies.values().any(|c| {
            c.book_id == book_id
                && matches!(
                    c.status,
                    CopyStatus::Available | CopyStatus::Borrowed | CopyStatus::Reserved
                )
        });

        if !has_any_healthy_copy {
            for res in reservations.iter_mut().filter(|r| {
                r.book_id == book_id
                    && (r.status == ReservationStatus::Waiting
                        || r.status == ReservationStatus::Ready)
            }) {
                res.status = ReservationStatus::Cancelled;
                res.assigned_copy_id = None;
                res.expires_at = None;
            }
        }

        Ok(())
    }

    fn assign_next_reservation(
        reservations: &mut [Reservation],
        copy: &mut BookCopy,
        now: DateTime<Utc>,
        pickup_window_days: i64,
    ) -> Option<Reservation> {
        let next_idx = reservations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.book_id == copy.book_id && r.status == ReservationStatus::Waiting)
            .min_by_key(|(_, r)| r.created_at)
            .map(|(idx, _)| idx);

        if let Some(idx) = next_idx {
            copy.status = CopyStatus::Reserved;
            reservations[idx].status = ReservationStatus::Ready;
            reservations[idx].assigned_copy_id = copy.id;
            reservations[idx].expires_at = Some(now + Duration::days(pickup_window_days));
            Some(reservations[idx].clone())
        } else {
            copy.status = CopyStatus::Available;
            None
        }
    }
}