use crate::error::AppError;
use crate::models::{
    book::{Book, BookCopy, CopyStatus},
    procurement::{CreateProcurementForm, Procurement, ProcurementStatus},
};
use bson::{doc, oid::ObjectId, DateTime};
use mongodb::{options::FindOptions, Database};	

pub struct ProcurementService;

impl ProcurementService {
    /// 取得所有採購單（依建立時間新到舊排序）
    pub async fn list_all(db: &Database) -> Result<Vec<Procurement>, AppError> {
        let coll = db.collection::<Procurement>("procurements");
        let options = FindOptions::builder().sort(doc! { "created_at": -1 }).build();
        let mut cursor = coll.find(doc! {}).with_options(options).await?;

        let mut list = Vec::new();
        while cursor.advance().await? {
            list.push(cursor.deserialize_current()?);
        }
        Ok(list)
    }

    /// 1. 採購人員建立新採購單
    pub async fn create_order(
        db: &Database,
        form: CreateProcurementForm,
        requester_name: &str,
    ) -> Result<(), AppError> {
        if form.quantity <= 0 {
            return Err(AppError::BadRequest("採購數量必須大於 0".to_string()));
        }

        let coll = db.collection::<Procurement>("procurements");
        let order = Procurement {
            id: None,
            isbn: form.isbn.trim().to_string(),
            title: form.title.trim().to_string(),
            author: form.author.trim().to_string(),
            publisher: form.publisher.trim().to_string(),
            quantity: form.quantity,
            unit_price: form.unit_price,
            status: ProcurementStatus::Pending,
            requester_name: requester_name.to_string(),
            approver_name: None,
            generated_barcodes: Vec::new(),
            created_at: DateTime::now(),
        };

        coll.insert_one(order).await?;
        Ok(())
    }

    /// 2. 館長 / 超管審核採購單（核准或駁回）
    pub async fn review_order(
        db: &Database,
        id_str: &str,
        approve: bool,
        approver_name: &str,
    ) -> Result<(), AppError> {
        let oid = ObjectId::parse_str(id_str).map_err(|_| AppError::InvalidObjectId)?;
        let coll = db.collection::<Procurement>("procurements");

        let new_status = if approve {
            bson::to_bson(&ProcurementStatus::Approved).unwrap()
        } else {
            bson::to_bson(&ProcurementStatus::Rejected).unwrap()
        };

        coll.update_one(
            doc! { "_id": oid, "status": "Pending" },
            doc! {
                "$set": {
                    "status": new_status,
                    "approver_name": approver_name
                }
            },
        )
        .await?;

        Ok(())
    }

    /// 3. 採購人員驗收入庫：自動更新書目總數並批量生成實體複本條碼
    pub async fn receive_and_catalog(
        db: &Database,
        id_str: &str,
        location: &str,
    ) -> Result<(), AppError> {
        let oid = ObjectId::parse_str(id_str).map_err(|_| AppError::InvalidObjectId)?;
        let proc_coll = db.collection::<Procurement>("procurements");
        let book_coll = db.collection::<Book>("books");
        let copy_coll = db.collection::<BookCopy>("book_copies");

        // 查詢並確認狀態為 Approved
        let order = proc_coll
            .find_one(doc! { "_id": oid })
            .await?
            .ok_or_else(|| AppError::BadRequest("找不到指定的採購單".to_string()))?;

        if order.status != ProcurementStatus::Approved {
            return Err(AppError::BadRequest("僅能驗收狀態為「已核准採購」的單據".to_string()));
        }

        let now = DateTime::now();

        // A. 查詢或建立書目主檔 (books)
        let book_id = match book_coll.find_one(doc! { "isbn": &order.isbn }).await? {
            Some(existing_book) => {
                let bid = existing_book.id.unwrap();
                book_coll
                    .update_one(
                        doc! { "_id": bid },
                        doc! {
                            "$inc": {
                                "total_copies": order.quantity,
                                "available_copies": order.quantity
                            },
                            "$set": { "updated_at": now }
                        },
                    )
                    .await?;
                bid
            }
            None => {
                let new_book = Book {
                    id: None,
                    isbn: order.isbn.clone(),
                    title: order.title.clone(),
                    author: order.author.clone(),
                    publisher: order.publisher.clone(),
                    category: "未分類".to_string(),
                    tags: Vec::new(),
                    total_copies: order.quantity,
                    available_copies: order.quantity,
                    created_at: now,
                    updated_at: now,
                };
                let insert_res = book_coll.insert_one(new_book).await?;
                insert_res.inserted_id.as_object_id().unwrap()
            }
        };

        // B. 計算目前該 ISBN 已有多少複本，作為流水號起點
        let existing_count = copy_coll
            .count_documents(doc! { "isbn": &order.isbn })
            .await? as i32;

        // C. 批量產生實體複本 (book_copies) 與條碼
        let mut new_copies = Vec::with_capacity(order.quantity as usize);
        let mut barcodes = Vec::with_capacity(order.quantity as usize);
        let clean_isbn: String = order.isbn.chars().filter(|c| c.is_ascii_alphanumeric()).collect();

        for i in 1..=order.quantity {
            let seq = existing_count + i;
            let barcode = format!("LIB-{}-{:04}", clean_isbn, seq);
            barcodes.push(barcode.clone());

            new_copies.push(BookCopy {
                id: None,
                book_id,
                isbn: order.isbn.clone(),
                barcode,
                location: location.trim().to_string(),
                status: CopyStatus::Available,
                procurement_id: oid,
                created_at: now,
            });
        }

        if !new_copies.is_empty() {
            copy_coll.insert_many(new_copies).await?;
        }

        // D. 更新採購單狀態為 Received 並寫入生成的條碼清單
        proc_coll
            .update_one(
                doc! { "_id": oid },
                doc! {
                    "$set": {
                        "status": bson::to_bson(&ProcurementStatus::Received).unwrap(),
                        "generated_barcodes": barcodes
                    }
                },
            )
            .await?;

        Ok(())
    }
}

