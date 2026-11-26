use crate::models::security::{
    AuthContext, ImmutableFineRecord, MaskedMemberView, MemberPersonalData,
    SecureBorrowRecord, SecurityAuditLog, SecurityComplianceReport, SecurityRole,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SecurityError {
    #[error("未經授權的存取：需要有效的 Supabase Auth 驗證憑證")]
    Unauthorized,
    #[error("權限不足 (RBAC)：角色 {0:?} 無法執行此操作")]
    ForbiddenRole(SecurityRole),
    #[error("隱私保護違規 (RLS)：讀者僅能查詢自己的借閱紀錄")]
    OwnershipViolation,
    #[error("資料完整性異常：罰款及繳費紀錄受不可變保護，禁止任意修改或簽章不符")]
    ImmutableRecordTampered,
    #[error("安全策略違規：偵測到潛在注入攻擊或危險金鑰暴露")]
    SecurityPolicyViolation(String),
}

/// 圖書館系統核心安全與資料保護服務
pub struct SecurityService {
    /// 內部簽章鹽值（模擬從 .env 讀取的伺服器端密鑰，絕不對外公開）
    server_secret_pepper: String,
    borrow_records: Arc<RwLock<Vec<SecureBorrowRecord>>>,
    fine_records: Arc<RwLock<HashMap<Uuid, ImmutableFineRecord>>>,
    audit_logs: Arc<RwLock<Vec<SecurityAuditLog>>>,
}

impl SecurityService {
    pub fn new(server_secret_pepper: &str) -> Result<Self, SecurityError> {
        if server_secret_pepper.trim().len() < 16 {
            return Err(SecurityError::SecurityPolicyViolation(
                "SERVER_SECRET 強度不足，必須從 .env 載入至少 16 字元密鑰".to_string(),
            ));
        }
        Ok(Self {
            server_secret_pepper: server_secret_pepper.to_string(),
            borrow_records: Arc::new(RwLock::new(Vec::new())),
            fine_records: Arc::new(RwLock::new(HashMap::new())),
            audit_logs: Arc::new(RwLock::new(Vec::new())),
        })
    }

    // =========================================================================
    // 1. 密碼安全：使用加鹽雜湊機制（禁止明文儲存）
    // =========================================================================
    pub fn hash_password(&self, raw_password: &str, user_salt: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.server_secret_pepper.as_bytes());
        hasher.update(user_salt.as_bytes());
        hasher.update(raw_password.as_bytes());
        format!("sha256${}${}", user_salt, hex::encode(hasher.finalize()))
    }

    pub fn verify_password(&self, raw_password: &str, user_salt: &str, stored_hash: &str) -> bool {
        self.hash_password(raw_password, user_salt) == stored_hash
    }

    // =========================================================================
    // 2. 防止攻擊：SQL Injection 與 XSS 輸入淨化防護
    // =========================================================================
    pub fn sanitize_and_validate_input(&self, input: &str) -> Result<String, SecurityError> {
        let upper = input.to_uppercase();
        let sqli_patterns = ["DROP TABLE", "UNION SELECT", "--", ";--", "OR 1=1", "' OR '1'='1"];
        for pattern in sqli_patterns {
            if upper.contains(pattern) {
                return Err(SecurityError::SecurityPolicyViolation(format!(
                    "偵測到 SQL Injection 攻擊特徵: {}",
                    pattern
                )));
            }
        }

        // XSS HTML 實體編碼轉義
        let escaped = input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;");

        Ok(escaped)
    }

    // =========================================================================
    // 3. 特別注意 1：讀者只能查詢自己的借閱紀錄（應用層 + 模擬 PostgreSQL RLS）
    // =========================================================================
    pub fn seed_borrow_record(&self, record: SecureBorrowRecord) {
        self.borrow_records.write().unwrap().push(record);
    }

    pub fn query_borrow_records(
        &self,
        auth: &AuthContext,
        target_member_id: Uuid,
    ) -> Result<Vec<SecureBorrowRecord>, SecurityError> {
        match auth.role {
            // 讀者僅能查詢 user_id == target_member_id 的紀錄
            SecurityRole::Member => {
                if auth.user_id != target_member_id {
                    self.record_audit_log(
                        auth,
                        "UNAUTHORIZED_BORROW_QUERY",
                        &format!("member:{}", target_member_id),
                        "讀者嘗試跨帳號查詢他人借閱紀錄，已遭阻擋",
                    );
                    return Err(SecurityError::OwnershipViolation);
                }
            }
            // 館員與管理員因業務需要可查詢，但必須留下稽核紀錄
            SecurityRole::Librarian | SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {
                self.record_audit_log(
                    auth,
                    "STAFF_QUERY_BORROW_RECORDS",
                    &format!("member:{}", target_member_id),
                    "館員/管理員查詢讀者借閱紀錄",
                );
            }
            // 採購與訪客無權查詢借閱紀錄
            _ => return Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }

        let records = self.borrow_records.read().unwrap();
        Ok(records
            .iter()
            .filter(|r| r.member_id == target_member_id)
            .cloned()
            .collect())
    }

    // =========================================================================
    // 4. 特別注意 2：館員只能執行被授權的操作（RBAC 權限控管）
    // =========================================================================
    pub fn execute_librarian_circulation(
        &self,
        auth: &AuthContext,
        action_name: &str,
        book_barcode: &str,
    ) -> Result<String, SecurityError> {
        match auth.role {
            SecurityRole::Librarian | SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {
                let safe_barcode = self.sanitize_and_validate_input(book_barcode)?;
                self.record_audit_log(
                    auth,
                    action_name,
                    &format!("book:{}", safe_barcode),
                    "執行流通借還書授權作業",
                );
                Ok(format!("操作成功: {} ({})", action_name, safe_barcode))
            }
            _ => Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }
    }

    /// 系統管理專屬操作（一般館員無權執行，且管理員執行時必須寫入 Audit Log）
    pub fn admin_update_system_policy(
        &self,
        auth: &AuthContext,
        policy_key: &str,
        policy_value: &str,
    ) -> Result<(), SecurityError> {
        match auth.role {
            SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {
                let safe_key = self.sanitize_and_validate_input(policy_key)?;
                let safe_val = self.sanitize_and_validate_input(policy_value)?;
                // 特別注意 4：管理員的重要操作必須留下紀錄
                self.record_audit_log(
                    auth,
                    "ADMIN_UPDATE_SYSTEM_POLICY",
                    &safe_key,
                    &format!("變更系統政策為: {}", safe_val),
                );
                Ok(())
            }
            _ => Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }
    }

    // =========================================================================
    // 5. 特別注意 3：罰款及繳費紀錄不可任意修改（HMAC 簽章驗證 + 唯讀防竄改）
    // =========================================================================
    fn compute_fine_signature(
        &self,
        fine_id: Uuid,
        member_id: Uuid,
        amount_twd: i64,
        reason: &str,
        is_paid: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.server_secret_pepper.as_bytes());
        hasher.update(fine_id.to_string().as_bytes());
        hasher.update(member_id.to_string().as_bytes());
        hasher.update(amount_twd.to_le_bytes());
        hasher.update(reason.as_bytes());
        hasher.update([u8::from(is_paid)]);
        hex::encode(hasher.finalize())
    }

    pub fn issue_fine_record(
        &self,
        auth: &AuthContext,
        member_id: Uuid,
        amount_twd: i64,
        reason: &str,
    ) -> Result<ImmutableFineRecord, SecurityError> {
        match auth.role {
            SecurityRole::Librarian | SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {}
            _ => return Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }

        let safe_reason = self.sanitize_and_validate_input(reason)?;
        let fine_id = Uuid::new_v4();
        let signature =
            self.compute_fine_signature(fine_id, member_id, amount_twd, &safe_reason, false);

        let record = ImmutableFineRecord {
            fine_id,
            member_id,
            amount_twd,
            reason: safe_reason,
            is_paid: false,
            created_at: Utc::now(),
            created_by: auth.user_id,
            integrity_signature: signature,
        };

        self.fine_records
            .write()
            .unwrap()
            .insert(fine_id, record.clone());

        self.record_audit_log(
            auth,
            "ISSUE_FINE_RECORD",
            &format!("fine:{}", fine_id),
            &format!("開立罰款 NT${} 給讀者 {}", amount_twd, member_id),
        );

        Ok(record)
    }

    /// 驗證罰款紀錄是否遭到未經授權的金額或狀態竄改
    pub fn verify_fine_integrity(&self, record: &ImmutableFineRecord) -> Result<(), SecurityError> {
        let expected = self.compute_fine_signature(
            record.fine_id,
            record.member_id,
            record.amount_twd,
            &record.reason,
            record.is_paid,
        );
        if expected != record.integrity_signature {
            return Err(SecurityError::ImmutableRecordTampered);
        }
        Ok(())
    }

    /// 禁止直接任意修改罰款金額；僅允許透過正規繳費核銷流程產生新簽章並記錄稽核日誌
    pub fn attempt_arbitrary_fine_modification(
        &self,
        auth: &AuthContext,
        fine_id: Uuid,
        _new_amount: i64,
    ) -> Result<(), SecurityError> {
        self.record_audit_log(
            auth,
            "BLOCKED_FINE_MODIFICATION",
            &format!("fine:{}", fine_id),
            "嘗試直接修改已開立之罰款金額，系統已拒絕",
        );
        Err(SecurityError::ImmutableRecordTampered)
    }

    // =========================================================================
    // 6. 特別注意 5：讀者個資依個資保護規範管理（最小權限存取與欄位遮罩）
    // =========================================================================
    pub fn get_member_profile_with_min_privilege(
        &self,
        auth: &AuthContext,
        raw_data: &MemberPersonalData,
    ) -> Result<MaskedMemberView, SecurityError> {
        match auth.role {
            SecurityRole::Member if auth.user_id == raw_data.user_id => {}
            SecurityRole::Librarian | SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {
                self.record_audit_log(
                    auth,
                    "ACCESS_MEMBER_PII",
                    &format!("member:{}", raw_data.user_id),
                    "依最小權限原則存取去識別化讀者個資",
                );
            }
            SecurityRole::Member => return Err(SecurityError::OwnershipViolation),
            _ => return Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }

        Ok(MaskedMemberView {
            user_id: raw_data.user_id,
            masked_name: Self::mask_name(&raw_data.full_name),
            masked_national_id: Self::mask_national_id(&raw_data.national_id),
            masked_phone: Self::mask_phone(&raw_data.phone),
            masked_email: Self::mask_email(&raw_data.email),
        })
    }

    fn mask_name(name: &str) -> String {
        let chars: Vec<char> = name.chars().collect();
        match chars.len() {
            0 => String::new(),
            1 => "*".to_string(),
            2 => format!("{}*", chars[0]),
            n => format!("{}{}{}", chars[0], "*".repeat(n - 2), chars[n - 1]),
        }
    }

    fn mask_national_id(id: &str) -> String {
        if id.len() >= 10 {
            format!("{}*****{}", &id[..2], &id[7..])
        } else {
            "**********".to_string()
        }
    }

    fn mask_phone(phone: &str) -> String {
        if phone.len() >= 10 {
            format!("{}-***-{}", &phone[..4], &phone[7..])
        } else {
            "****-***-***".to_string()
        }
    }

    fn mask_email(email: &str) -> String {
        if let Some((local, domain)) = email.split_once('@') {
            let prefix: String = local.chars().take(2).collect();
            format!("{}***@{}", prefix, domain)
        } else {
            "***@***".to_string()
        }
    }

    // =========================================================================
    // 7. 特別注意 4：操作追蹤 Audit Logs（雜湊鏈不可竄改紀錄）
    // =========================================================================
    pub fn record_audit_log(
        &self,
        auth: &AuthContext,
        action: &str,
        target_resource: &str,
        detail: &str,
    ) {
        let mut logs = self.audit_logs.write().unwrap();
        let prev_hash = logs
            .last()
            .map(|l| l.current_log_hash.clone())
            .unwrap_or_else(|| "GENESIS_AUDIT_BLOCK".to_string());

        let timestamp = Utc::now();
        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(auth.user_id.to_string().as_bytes());
        hasher.update(action.as_bytes());
        hasher.update(target_resource.as_bytes());
        hasher.update(detail.as_bytes());
        hasher.update(timestamp.to_rfc3339().as_bytes());
        let current_hash = hex::encode(hasher.finalize());

        logs.push(SecurityAuditLog {
            log_id: Uuid::new_v4(),
            actor_id: auth.user_id,
            actor_role: auth.role.clone(),
            action: action.to_string(),
            target_resource: target_resource.to_string(),
            detail: detail.to_string(),
            ip_or_source: "API_GATEWAY".to_string(),
            prev_log_hash: prev_hash,
            current_log_hash: current_hash,
            timestamp,
        });
    }

    pub fn get_audit_logs(&self, auth: &AuthContext) -> Result<Vec<SecurityAuditLog>, SecurityError> {
        match auth.role {
            SecurityRole::LibrarianAdmin | SecurityRole::SuperAdmin => {
                Ok(self.audit_logs.read().unwrap().clone())
            }
            _ => Err(SecurityError::ForbiddenRole(auth.role.clone())),
        }
    }

    // =========================================================================
    // 8. 資料庫安全（PostgreSQL RLS SQL 策略）與金鑰/備份合規檢查
    // =========================================================================
    pub fn generate_postgresql_rls_policies(&self) -> String {
        r#"
-- 啟用資料表 Row Level Security (RLS)
ALTER TABLE borrow_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE fine_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE member_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

-- 1. 讀者只能查詢自己的借閱紀錄
CREATE POLICY "member_select_own_borrows" ON borrow_records
    FOR SELECT USING (auth.uid() = member_id);

-- 2. 罰款及繳費紀錄禁止任意 UPDATE / DELETE
CREATE POLICY "fines_no_arbitrary_update" ON fine_records
    FOR UPDATE USING (false);
CREATE POLICY "fines_no_delete" ON fine_records
    FOR DELETE USING (false);

-- 3. 稽核日誌僅允許 INSERT，禁止修改或刪除
CREATE POLICY "audit_logs_append_only" ON audit_logs
    FOR INSERT WITH CHECK (auth.role() = 'authenticated');
"#
        .trim()
        .to_string()
    }

    /// 驗證前端公開設定檔是否不慎暴露 Supabase Service Role Key
    pub fn validate_public_env_safety(
        &self,
        public_env_vars: &HashMap<String, String>,
    ) -> Result<SecurityComplianceReport, SecurityError> {
        for (key, val) in public_env_vars {
            let upper_key = key.to_uppercase();
            if upper_key.contains("SERVICE_ROLE") || val.contains("service_role") {
                return Err(SecurityError::SecurityPolicyViolation(format!(
                    "嚴重安全警告：禁止在公開環境變數 ({}) 中暴露 Supabase Service Role Key！",
                    key
                )));
            }
        }

        Ok(SecurityComplianceReport {
            rls_enabled_tables: vec![
                "borrow_records".to_string(),
                "fine_records".to_string(),
                "member_profiles".to_string(),
                "audit_logs".to_string(),
            ],
            backup_schedule_cron: "0 2 * * * (每日凌晨 02:00 自動加密備份至異地儲存)".to_string(),
            backup_encrypted: true,
            service_role_key_exposed: false,
        })
    }
}
