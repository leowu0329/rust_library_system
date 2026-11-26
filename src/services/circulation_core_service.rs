use crate::models::database_schema::*;
use chrono::{Duration, Utc};
use mongodb::{
    bson::{doc, oid::ObjectId, to_bson},
    options::{FindOneAndUpdateOptions, ReturnDocument},
    Collection, Database,
};

#[derive(Debug)]
pub enum DbDesignError {
    CopyNotAvailable,
    MemberSuspended,
    LoanRecordImmutable,
    FineNotFound,
    InvalidAmount,
    DatabaseError(String),
}

impl From<mongodb::error::Error> for DbDesignError {
    fn from(err: mongodb::error::Error) -> Self {
        DbDesignError::DatabaseError(err.to_string())
    }
}

pub struct CirculationCoreService {
    pub db: Database,
}

impl CirculationCoreService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    fn copies(&self) -> Collection<BookCopy> {
        self.db.collection("book_copies")
    }
    fn members(&self) -> Collection<Member> {
        self.db.collection("members")
    }
    fn loans(&self) -> Collection<Loan> {
        self.db.collection("loans")
    }
    fn fines(&self) -> Collection<Fine> {
        self.db.collection("fines")
    }
    fn payments(&self) -> Collection<PaymentTransaction> {
        self.db.collection("payments")
    }
    fn audit_logs(&self) -> Collection<AuditLog> {
        self.db.collection("audit_logs")
    }

    /// 原則 D：避免重複借出（原子條件更新鎖定副本狀態）
    pub async fn checkout_copy(
        &self,
        member_id: ObjectId,
        copy_id: ObjectId,
        operator_id: ObjectId,
        loan_days: i64,
    ) -> Result<Loan, DbDesignError> {
        let member = self
            .members()
            .find_one(doc! { "_id": member_id })
            .await?
            .ok_or(DbDesignError::MemberSuspended)?;

        if member.status != MemberStatus::Active {
            return Err(DbDesignError::MemberSuspended);
        }

        let filter = doc! {
            "_id": copy_id,
            "status": to_bson(&CopyStatus::Available).unwrap()
        };
        let update = doc! {
            "$set": { "status": to_bson(&CopyStatus::Loaned).unwrap() }
        };
        let opts = FindOneAndUpdateOptions::builder()
            .return_document(ReturnDocument::After)
            .build();

        let locked_copy = self
            .copies()
            .find_one_and_update(filter, update)
            .with_options(opts)
            .await?;

        if locked_copy.is_none() {
            return Err(DbDesignError::CopyNotAvailable);
        }

        let now = Utc::now();
        let mut loan = Loan {
            id: None,
            member_id,
            copy_id,
            operator_id,
            borrowed_at: now,
            due_date: now + Duration::days(loan_days),
            returned_at: None,
            status: LoanStatus::Active,
        };

        let insert_res = self.loans().insert_one(&loan).await?;
        loan.id = insert_res.inserted_id.as_object_id();

        self.log_audit(
            operator_id,
            "CHECKOUT_COPY",
            "loans",
            loan.id.unwrap(),
            format!("Member {} borrowed copy {}", member_id, copy_id),
        )
        .await?;

        Ok(loan)
    }

    /// 歸還書籍並自動計算逾期罰款
    pub async fn return_copy(
        &self,
        loan_id: ObjectId,
        _operator_id: ObjectId,
        daily_fine_rate: i64,
    ) -> Result<(Loan, Option<Fine>), DbDesignError> {
        let mut loan = self
            .loans()
            .find_one(doc! { "_id": loan_id })
            .await?
            .ok_or(DbDesignError::DatabaseError("Loan not found".into()))?;

        let now = Utc::now();
        loan.returned_at = Some(now);
        loan.status = LoanStatus::Returned;

        self.loans()
            .update_one(
                doc! { "_id": loan_id },
                doc! {
                    "$set": {
                        "returned_at": to_bson(&loan.returned_at).unwrap(),
                        "status": to_bson(&LoanStatus::Returned).unwrap()
                    }
                },
            )
            .await?;

        self.copies()
            .update_one(
                doc! {
                    "_id": loan.copy_id,
                    "status": { "$ne": to_bson(&CopyStatus::Scrapped).unwrap() }
                },
                doc! { "$set": { "status": to_bson(&CopyStatus::Available).unwrap() } },
            )
            .await?;

        let mut generated_fine = None;
        if now > loan.due_date {
            let overdue_days = (now - loan.due_date).num_days().max(1);
            let fine_amount = overdue_days * daily_fine_rate;
            let mut fine = Fine {
                id: None,
                loan_id,
                member_id: loan.member_id,
                amount: fine_amount,
                status: FineStatus::Unpaid,
                created_at: now,
            };
            let res = self.fines().insert_one(&fine).await?;
            fine.id = res.inserted_id.as_object_id();
            generated_fine = Some(fine);
        }

        Ok((loan, generated_fine))
    }

    /// 原則 B：借閱紀錄不可直接刪除
    pub async fn attempt_delete_loan(&self, _loan_id: ObjectId) -> Result<(), DbDesignError> {
        Err(DbDesignError::LoanRecordImmutable)
    }

    /// 停權讀者或報廢書籍（軟狀態更新，保留與歷史借閱紀錄的關聯）
    pub async fn suspend_member_and_scrap_copy(
        &self,
        member_id: ObjectId,
        copy_id: ObjectId,
        operator_id: ObjectId,
    ) -> Result<(), DbDesignError> {
        self.members()
            .update_one(
                doc! { "_id": member_id },
                doc! { "$set": { "status": to_bson(&MemberStatus::Suspended).unwrap() } },
            )
            .await?;

        self.copies()
            .update_one(
                doc! { "_id": copy_id },
                doc! { "$set": { "status": to_bson(&CopyStatus::Scrapped).unwrap() } },
            )
            .await?;

        self.log_audit(
            operator_id,
            "SUSPEND_AND_SCRAP",
            "book_copies",
            copy_id,
            "Soft-disabled member and copy while preserving loan history".into(),
        )
        .await?;

        Ok(())
    }

    /// 原則 C：所有金額異動皆寫入獨立交易明細（繳費、折抵、退款）
    pub async fn record_financial_transaction(
        &self,
        fine_id: ObjectId,
        operator_id: ObjectId,
        tx_type: PaymentType,
        amount: i64,
        note: &str,
    ) -> Result<PaymentTransaction, DbDesignError> {
        if amount <= 0 {
            return Err(DbDesignError::InvalidAmount);
        }

        let fine = self
            .fines()
            .find_one(doc! { "_id": fine_id })
            .await?
            .ok_or(DbDesignError::FineNotFound)?;

        let mut tx = PaymentTransaction {
            id: None,
            fine_id,
            member_id: fine.member_id,
            operator_id,
            tx_type: tx_type.clone(),
            amount,
            note: note.to_string(),
            created_at: Utc::now(),
        };

        let insert_res = self.payments().insert_one(&tx).await?;
        tx.id = insert_res.inserted_id.as_object_id();

        // 使用 MongoDB 原生 Cursor::advance() 彙算所有歷史交易流水
        let mut cursor = self.payments().find(doc! { "fine_id": fine_id }).await?;
        let mut net_settled: i64 = 0;

        while cursor.advance().await? {
            let item = cursor.deserialize_current()?;
            match item.tx_type {
                PaymentType::Payment | PaymentType::Waiver => net_settled += item.amount,
                PaymentType::Refund => net_settled -= item.amount,
            }
        }

        let new_status = if net_settled >= fine.amount {
            FineStatus::Paid
        } else if net_settled > 0 {
            FineStatus::PartiallyPaid
        } else {
            FineStatus::Unpaid
        };

        self.fines()
            .update_one(
                doc! { "_id": fine_id },
                doc! { "$set": { "status": to_bson(&new_status).unwrap() } },
            )
            .await?;

        Ok(tx)
    }

    async fn log_audit(
        &self,
        actor_id: ObjectId,
        action: &str,
        target_collection: &str,
        target_id: ObjectId,
        detail: String,
    ) -> Result<(), DbDesignError> {
        let log = AuditLog {
            id: None,
            actor_id,
            action: action.to_string(),
            target_collection: target_collection.to_string(),
            target_id,
            detail,
            created_at: Utc::now(),
        };
        self.audit_logs().insert_one(log).await?;
        Ok(())
    }
}