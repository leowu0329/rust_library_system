use rust_library_system::models::project_management::{DevelopmentPhase, MilestoneStatus};
use rust_library_system::services::project_management_service::{
    ProjectManagementError, ProjectManagementService,
};

#[test]
fn test_lms_roadmap_structure_and_initial_state() {
    let pm = ProjectManagementService::new_lms_roadmap("Smart Library Management System");
    let report = pm.generate_progress_report();

    assert_eq!(report.project_name, "Smart Library Management System");
    assert_eq!(report.overall_completion_percentage, 0.0);
    assert_eq!(
        report.current_active_phase,
        Some(DevelopmentPhase::Phase1RequirementAndPlanning)
    );
    assert!(!report.ready_for_production);

    // 驗證 Phase 1 包含基礎階段的 5 項核心設計文件
    let p1 = pm
        .get_phase(DevelopmentPhase::Phase1RequirementAndPlanning)
        .expect("Phase 1 應該存在");
    assert_eq!(p1.deliverables.len(), 5);
    assert_eq!(p1.status, MilestoneStatus::InProgress);

    // 驗證 Phase 8 包含 5 項測試與部署項目
    let p8 = pm
        .get_phase(DevelopmentPhase::Phase8TestingAndDeployment)
        .expect("Phase 8 應該存在");
    assert_eq!(p8.deliverables.len(), 5);
    assert_eq!(p8.status, MilestoneStatus::NotStarted);
}

#[test]
fn test_phase_gate_enforcement_prevents_skipping_phases() {
    let mut pm = ProjectManagementService::new_lms_roadmap("LMS Gate Enforcement Test");

    // 1. Phase 1 尚未完成時，嘗試直接啟動 Phase 2 必須被阻擋
    let skip_err = pm.start_phase(DevelopmentPhase::Phase2FoundationSetup);
    assert_eq!(
        skip_err,
        Err(ProjectManagementError::PrerequisitePhaseIncomplete {
            current: DevelopmentPhase::Phase2FoundationSetup,
            required: DevelopmentPhase::Phase1RequirementAndPlanning,
        })
    );

    // 2. Phase 1 僅完成部分文件時，嘗試通過 Gate 簽核必須被阻擋並列出未完成項目
    pm.verify_deliverable(
        DevelopmentPhase::Phase1RequirementAndPlanning,
        "P1-1",
        "docs/srs_v1.md",
    )
    .unwrap();
    pm.verify_deliverable(
        DevelopmentPhase::Phase1RequirementAndPlanning,
        "P1-2",
        "docs/rbac_matrix.md",
    )
    .unwrap();

    let gate_err = pm.approve_phase_gate(
        DevelopmentPhase::Phase1RequirementAndPlanning,
        "Lead Architect",
    );
    assert_eq!(
        gate_err,
        Err(ProjectManagementError::UnverifiedDeliverablesRemaining {
            phase: DevelopmentPhase::Phase1RequirementAndPlanning,
            remaining_codes: vec!["P1-3".to_string(), "P1-4".to_string(), "P1-5".to_string()],
        })
    );

    // 3. 在未完成所有階段前，禁止部署至正式環境
    let deploy_err = pm.deploy_to_production("Release Manager");
    assert!(matches!(
        deploy_err,
        Err(ProjectManagementError::ProductionReleaseNotReady(_))
    ));
    assert!(!pm.is_production_deployed());
}

#[test]
fn test_full_8_phases_end_to_end_delivery_and_production_release() {
    let mut pm = ProjectManagementService::new_lms_roadmap("Enterprise Rust LMS");

    // === Phase 1：需求分析與系統規劃 ===
    pm.verify_deliverable(DevelopmentPhase::Phase1RequirementAndPlanning, "P1-1", "docs/srs.md")
        .unwrap();
    pm.verify_deliverable(DevelopmentPhase::Phase1RequirementAndPlanning, "P1-2", "docs/rbac.md")
        .unwrap();
    pm.verify_deliverable(DevelopmentPhase::Phase1RequirementAndPlanning, "P1-3", "docs/arch.png")
        .unwrap();
    pm.verify_deliverable(DevelopmentPhase::Phase1RequirementAndPlanning, "P1-4", "docs/bpmn.png")
        .unwrap();
    pm.verify_deliverable(DevelopmentPhase::Phase1RequirementAndPlanning, "P1-5", "docs/erd.sql")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase1RequirementAndPlanning, "PM Alice")
        .unwrap();

    // === Phase 2：基礎系統建置（登入、註冊、Email 驗證、RBAC） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase2FoundationSetup, "src/auth")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase2FoundationSetup, "Tech Lead Bob")
        .unwrap();

    // === Phase 3：圖書與讀者管理（書目 CRUD、館藏副本、作者出版社分類、讀者 CRUD、條碼） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase3CatalogAndReader, "src/catalog")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase3CatalogAndReader, "Tech Lead Bob")
        .unwrap();

    // === Phase 4：借閱核心功能（借閱、歸還、續借、借閱期限、借閱限制） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase4CirculationCore, "src/circulation")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase4CirculationCore, "Tech Lead Bob")
        .unwrap();

    // === Phase 5：進階業務功能（預約與候補、逾期及罰款、繳費、通知中心） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase5AdvancedOperations, "src/operations")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase5AdvancedOperations, "Tech Lead Bob")
        .unwrap();

    // === Phase 6：館務管理（採購、館藏盤點、遺失及報廢、多分館） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase6LibraryManagement, "src/procurement")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase6LibraryManagement, "Library Director")
        .unwrap();

    // === Phase 7：Dashboard 與報表（借閱統計、館藏分析、讀者分析、Excel / PDF 匯出） ===
    pm.verify_all_deliverables_in_phase(DevelopmentPhase::Phase7DashboardAndReports, "src/reports")
        .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase7DashboardAndReports, "Data Lead")
        .unwrap();

    // 檢查 Phase 1~7 完成後，目前活躍階段推進至 Phase 8
    let mid_report = pm.generate_progress_report();
    assert_eq!(
        mid_report.current_active_phase,
        Some(DevelopmentPhase::Phase8TestingAndDeployment)
    );
    assert!(!mid_report.ready_for_production);

    // === Phase 8：測試與正式部署（單元、整合、權限、效能、安全性測試） ===
    pm.verify_deliverable(
        DevelopmentPhase::Phase8TestingAndDeployment,
        "P8-1",
        "ci/unit_test_report.xml",
    )
    .unwrap();
    pm.verify_deliverable(
        DevelopmentPhase::Phase8TestingAndDeployment,
        "P8-2",
        "ci/integration_test_report.xml",
    )
    .unwrap();
    pm.verify_deliverable(
        DevelopmentPhase::Phase8TestingAndDeployment,
        "P8-3",
        "ci/rbac_security_audit.json",
    )
    .unwrap();
    pm.verify_deliverable(
        DevelopmentPhase::Phase8TestingAndDeployment,
        "P8-4",
        "ci/k6_load_benchmark.json",
    )
    .unwrap();
    pm.verify_deliverable(
        DevelopmentPhase::Phase8TestingAndDeployment,
        "P8-5",
        "ci/owasp_zap_scan.html",
    )
    .unwrap();
    pm.approve_phase_gate(DevelopmentPhase::Phase8TestingAndDeployment, "QA Lead Carol")
        .unwrap();

    // 驗證整體完成度為 100%，且可正式上線部署
    let final_report = pm.generate_progress_report();
    assert_eq!(final_report.overall_completion_percentage, 100.0);
    assert_eq!(final_report.current_active_phase, None);
    assert!(final_report.ready_for_production);

    let deploy_msg = pm
        .deploy_to_production("Release Manager Dave")
        .expect("所有階段通過後應成功部署");
    assert!(pm.is_production_deployed());
    assert!(deploy_msg.contains("Enterprise Rust LMS"));
}
