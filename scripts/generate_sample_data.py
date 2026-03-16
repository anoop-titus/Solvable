#!/usr/bin/env python3
"""Generate sample SQLite databases for Solvable demo/screenshots."""
import sqlite3
import os
import json
import random
from datetime import datetime, timedelta

OUTPUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sample_data")


def create_learnings_db():
    """Create learnings.db with sample learnings, processed files, and run progress."""
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    path = os.path.join(OUTPUT_DIR, "learnings.db")
    if os.path.exists(path):
        os.remove(path)

    conn = sqlite3.connect(path)
    conn.execute("PRAGMA journal_mode=WAL")

    conn.execute("""CREATE TABLE learnings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        source TEXT NOT NULL,
        agent TEXT NOT NULL,
        folder TEXT,
        file_path TEXT NOT NULL,
        file_name TEXT,
        learning TEXT NOT NULL,
        processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        topic TEXT DEFAULT ''
    )""")
    conn.execute("CREATE UNIQUE INDEX idx_learnings_dedup ON learnings(agent, learning)")

    conn.execute("""CREATE TABLE processed_files (
        file_path TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        file_size INTEGER,
        processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        learning_count INTEGER DEFAULT 0,
        error TEXT
    )""")

    conn.execute("""CREATE TABLE run_progress (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id TEXT NOT NULL,
        source TEXT NOT NULL,
        agent TEXT,
        folder TEXT,
        total_files INTEGER DEFAULT 0,
        processed INTEGER DEFAULT 0,
        skipped INTEGER DEFAULT 0,
        errors INTEGER DEFAULT 0,
        learnings_count INTEGER DEFAULT 0,
        started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        status TEXT DEFAULT 'running',
        pid INTEGER DEFAULT 0
    )""")

    agents = ["medishift-ceo", "heartlab-ceo", "mogul-agent"]
    sources = ["dropbox", "imap", "email", "gdrive"]

    sample_learnings = [
        "HIPAA compliance requires biometric data encryption at rest and in transit",
        "Lease agreement for Unit 4B expires March 31 — renewal decision needed by March 15",
        "Q4 tax liability increased 40% due to accelerated depreciation schedule changes",
        "Clinical trial Phase 2 results show 23% improvement in biomarker sensitivity",
        "Insurance renewal premium increased 18% — competitive quotes recommended",
        "Server migration to ARM instances could reduce cloud costs by 35%",
        "Vendor contract with MedSupply Inc contains auto-renewal clause — 60-day notice required",
        "Board meeting minutes show unanimous approval for Series B funding terms",
        "New FDA guidance requires additional validation for AI diagnostic tools",
        "Property inspection at 789 Elm revealed HVAC system approaching end of life",
        "Employee handbook needs updating for new remote work policy effective April 1",
        "Customer churn analysis shows 12% increase in enterprise segment attrition",
        "Patent application for ML-based diagnostic algorithm filed — provisional status",
        "Annual fire safety inspection due by end of month — previous year had 2 findings",
        "Supplier lead times have increased from 4 to 7 weeks for critical components",
        "Cash flow projection shows 3-month runway at current burn rate without bridge funding",
        "GDPR data processing agreement with EU partner still pending legal review",
        "Building permit for office renovation approved — construction starts May 1",
        "SOC 2 Type II audit findings require remediation of 3 access control gaps",
        "Market analysis shows competitor launched similar product at 20% lower price point",
        "Quarterly investor update needs to highlight 45% YoY revenue growth",
        "Tenant background check flagged inconsistency in employment verification",
        "API rate limiting needs implementation before public launch — current: unlimited",
        "Medical device certification timeline extended by 6 weeks due to reviewer backlog",
        "Accounts receivable aging shows $340K outstanding beyond 90 days",
        "New healthcare regulation requires patient consent workflow redesign",
        "Cybersecurity audit found 4 endpoints with outdated TLS 1.1 configuration",
        "Real estate appraisal for 456 Oak Ave came in 8% below purchase price",
        "Machine learning model accuracy dropped 4% after recent training data update",
        "Contract negotiation with LabCorp stalled on liability indemnification clause",
        "Building maintenance budget 15% over for Q1 — mainly HVAC emergency repairs",
        "New hire onboarding process takes average 12 days — target is 5 days",
        "Investment portfolio rebalancing needed — current allocation 70/30 vs target 60/40",
        "Patient data migration from legacy system 82% complete — 18% stuck on validation",
        "Competitor analysis reveals 3 new entrants in the diagnostic AI market",
        "Parking lot resurfacing quote received: $45K — budget line item is $30K",
        "Clinical validation study enrollment at 67% of target — recruitment push needed",
        "SaaS subscription costs increased 22% YoY — license optimization recommended",
        "Emergency generator last tested 8 months ago — quarterly testing requirement",
        "Debt-to-equity ratio improved from 2.3 to 1.8 after Q4 equity round",
        "Customer support ticket backlog grew to 234 unresolved — SLA at risk",
        "Medicaid reimbursement rate changes effective July 1 — revenue impact analysis needed",
        "WiFi infrastructure upgrade proposal: $85K for full building coverage",
        "Code review backlog shows 47 open PRs older than 2 weeks",
        "Property tax reassessment notice received — appeal deadline is April 30",
        "Lab equipment calibration certificates expiring — 6 instruments due this month",
        "Marketing campaign ROI analysis: $3.40 return per $1 spent on LinkedIn ads",
        "Investor relations portal needs Q1 earnings preview by March 20",
        "Building access card system logs show 3 unauthorized entry attempts last week",
        "AI model training pipeline consuming $4,200/month in GPU costs — optimization possible",
    ]

    now = datetime.now()
    for i, learning in enumerate(sample_learnings):
        agent = agents[i % len(agents)]
        source = sources[i % len(sources)]
        timestamp = (now - timedelta(days=random.randint(0, 7), hours=random.randint(0, 23))).strftime("%Y-%m-%d %H:%M:%S")
        folder = f"/Documents/{agent}" if source == "dropbox" else f"{agent}@company.com"
        conn.execute(
            "INSERT INTO learnings (source, agent, folder, file_path, file_name, learning, processed_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (source, agent, folder, f"{source}/{agent}/doc_{i}.pdf", f"document_{i}.pdf", learning, timestamp)
        )

    # Active run progress
    conn.execute(
        "INSERT INTO run_progress (run_id, source, agent, folder, total_files, processed, status, pid, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("run-001", "dropbox", "medishift-ceo", "/Documents/medishift-ceo", 47, 31, "running", 12345, now.strftime("%Y-%m-%d %H:%M:%S"))
    )
    conn.execute(
        "INSERT INTO run_progress (run_id, source, agent, folder, total_files, processed, status, pid, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("run-002", "imap", "heartlab-ceo", "heartlab@company.com", 89, 89, "watching", 12346, now.strftime("%Y-%m-%d %H:%M:%S"))
    )

    conn.commit()
    conn.close()
    print(f"Created {path} with {len(sample_learnings)} learnings")


def create_research_db():
    """Create research.db with sample issues, solutions, and metadata."""
    path = os.path.join(OUTPUT_DIR, "research.db")
    if os.path.exists(path):
        os.remove(path)

    conn = sqlite3.connect(path)
    conn.execute("PRAGMA journal_mode=WAL")

    conn.execute("""CREATE TABLE issues (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        learning_id INTEGER,
        agent TEXT DEFAULT 'unknown',
        title TEXT,
        description TEXT,
        category TEXT,
        severity TEXT,
        status TEXT DEFAULT 'open',
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        source_location TEXT
    )""")

    conn.execute("""CREATE TABLE solutions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_id INTEGER,
        source_url TEXT,
        source_title TEXT,
        summary TEXT,
        confidence TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(issue_id)
    )""")

    conn.execute("""CREATE TABLE daily_output (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_id INTEGER,
        solution_id INTEGER,
        issue_title TEXT,
        solution_summary TEXT,
        severity TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )""")

    conn.execute("""CREATE TABLE scan_cursor (
        id INTEGER PRIMARY KEY,
        last_learning_id INTEGER DEFAULT 0,
        last_scan_at DATETIME,
        last_digest_at DATETIME,
        last_action_at DATETIME
    )""")

    conn.execute("""CREATE TABLE actions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_id INTEGER UNIQUE,
        solution_id INTEGER,
        assigned_agent TEXT,
        airtable_record_id TEXT,
        airtable_status TEXT DEFAULT 'dispatched',
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )""")

    conn.execute("""CREATE TABLE repairs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_id INTEGER UNIQUE,
        status TEXT DEFAULT 'pending_approval',
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )""")

    issues_data = [
        ("HIPAA compliance gap for biometric data handling", "Patient biometric data stored without encryption", "compliance", "critical", "dispatched"),
        ("Federal tax under-withholding for 2024", "Q4 estimated payments below safe harbor threshold", "financial", "critical", "dispatched"),
        ("Lease expiration approaching for Unit 4B", "30-day renewal window closing — no decision recorded", "operational", "high", "open"),
        ("Clinical validation study behind enrollment target", "67% enrollment vs 100% target with 6 weeks remaining", "operational", "high", "researching"),
        ("SOC 2 access control gaps", "3 findings from Type II audit require remediation", "compliance", "critical", "dispatched"),
        ("API rate limiting not implemented", "Public endpoints have no rate limits — DoS risk", "technical", "critical", "researching"),
        ("Expired TLS 1.1 on 4 endpoints", "Cybersecurity audit flagged outdated encryption", "technical", "high", "dispatched"),
        ("Emergency generator testing overdue", "Last test 8 months ago — quarterly requirement", "compliance", "high", "open"),
        ("Customer support SLA at risk", "234 unresolved tickets — response time degrading", "operational", "high", "dispatched"),
        ("GDPR data processing agreement pending", "EU partner agreement unsigned — processing may be unlawful", "legal", "critical", "dispatched"),
        ("AI model accuracy degradation", "4% accuracy drop after recent training data update", "technical", "medium", "researching"),
        ("Building HVAC system end of life", "Inspection report indicates replacement needed within 12 months", "operational", "medium", "open"),
        ("Vendor auto-renewal clause risk", "MedSupply contract auto-renews in 45 days — 60-day notice required", "legal", "high", "dispatched"),
        ("Cash flow runway concern", "3-month runway at current burn rate without bridge", "financial", "critical", "dispatched"),
        ("Patient consent workflow redesign needed", "New regulation requires updated consent flow by Q3", "compliance", "high", "open"),
        ("Property tax reassessment appeal deadline", "Appeal must be filed by April 30", "financial", "medium", "open"),
        ("Code review backlog growing", "47 open PRs older than 2 weeks — velocity declining", "technical", "medium", "dispatched"),
        ("Lab calibration certificates expiring", "6 instruments due for recertification this month", "compliance", "high", "dispatched"),
        ("Competitor launched at 20% lower price", "Market positioning may need adjustment", "operational", "medium", "open"),
        ("Medical device certification delayed", "Timeline extended 6 weeks — reviewer backlog", "compliance", "high", "researching"),
        ("GPU training costs need optimization", "$4,200/month — potential 40% reduction possible", "financial", "medium", "open"),
        ("Parking lot resurfacing over budget", "$45K quote vs $30K budget — gap approval needed", "financial", "low", "open"),
        ("New hire onboarding too slow", "12 days avg vs 5-day target — productivity impact", "operational", "medium", "dispatched"),
        ("Investment portfolio needs rebalancing", "70/30 allocation vs 60/40 target — risk elevated", "financial", "medium", "open"),
        ("WiFi infrastructure upgrade needed", "Current coverage inadequate for hybrid work model", "technical", "low", "open"),
        ("Accounts receivable aging concern", "$340K outstanding beyond 90 days — collection risk", "financial", "high", "dispatched"),
        ("Insurance premium increase", "18% increase — competitive quotes not yet obtained", "financial", "medium", "open"),
        ("Unauthorized building access attempts", "3 incidents in past week — security review needed", "compliance", "high", "dispatched"),
        ("Legacy data migration stalled", "18% of patient records stuck on validation step", "technical", "high", "dispatched"),
        ("Medicaid reimbursement rate change", "Revenue impact analysis needed before July 1 effective date", "financial", "high", "open"),
    ]

    now = datetime.now()
    for i, (title, desc, cat, sev, status) in enumerate(issues_data):
        ts = (now - timedelta(days=random.randint(0, 14))).strftime("%Y-%m-%d %H:%M:%S")
        conn.execute(
            "INSERT INTO issues (learning_id, title, description, category, severity, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (i + 1, title, desc, cat, sev, status, ts, ts)
        )

    solutions_data = [
        (1, "https://www.hhs.gov/hipaa/", "HHS HIPAA Guidance", "Implement AES-256 encryption for biometric data at rest. Deploy TLS 1.3 for transit. Add audit logging for all biometric data access.", "high"),
        (2, "https://www.irs.gov/estimated-taxes", "IRS Estimated Tax Guide", "File Form 2210 with annualized income method. Make catch-up Q4 estimated payment of $28,400 before January 15.", "high"),
        (4, "https://clinicaltrials.gov/recruitment", "NIH Recruitment Guide", "Expand recruitment to 3 additional clinical sites. Implement patient referral incentive program. Extend enrollment deadline by 4 weeks.", "medium"),
        (5, "https://www.aicpa.org/soc2", "AICPA SOC 2 Guide", "Implement MFA for all privileged access. Deploy automated access review workflow. Add session timeout policies.", "high"),
        (6, "https://owasp.org/rate-limiting", "OWASP Rate Limiting", "Deploy nginx rate limiting: 100 req/min per IP. Add API key-based throttling. Implement circuit breaker pattern.", "high"),
        (7, "https://ssl-config.mozilla.org/", "Mozilla TLS Config", "Upgrade to TLS 1.3 on all endpoints. Disable TLS 1.0/1.1. Deploy automated certificate management.", "high"),
        (9, "https://www.zendesk.com/sla", "SLA Management Guide", "Hire 2 additional support agents. Implement ticket triage automation. Create escalation workflow for P1 tickets.", "medium"),
        (10, "https://gdpr.eu/data-processing", "GDPR Processing Guide", "Execute standard contractual clauses (SCCs). Complete Data Protection Impact Assessment. File with supervisory authority.", "high"),
        (11, "https://arxiv.org/model-drift", "ML Model Drift Paper", "Implement automated drift detection pipeline. Add data quality gates. Schedule weekly model performance reviews.", "medium"),
        (13, "https://law.cornell.edu/contracts", "Contract Law Reference", "Send non-renewal notice immediately via certified mail. Negotiate month-to-month terms as bridge. Start RFP for alternatives.", "high"),
        (14, "https://www.sba.gov/funding", "SBA Funding Guide", "Pursue bridge financing through existing investor network. Reduce burn rate by deferring non-critical hires. Accelerate revenue pipeline.", "high"),
        (15, "https://www.hhs.gov/consent", "Patient Consent Guide", "Redesign digital consent form with granular opt-in/opt-out. Implement consent versioning system. Add audit trail.", "medium"),
        (17, "https://engineering.practices.dev", "Engineering Practices", "Implement PR age SLA (max 5 business days). Add auto-assignment. Deploy PR size analysis to encourage smaller PRs.", "medium"),
        (18, "https://iso17025.com/calibration", "ISO 17025 Guide", "Schedule calibration appointments for all 6 instruments. Document calibration procedures. Implement expiry tracking dashboard.", "high"),
        (20, "https://fda.gov/medical-devices", "FDA Device Guidance", "Prepare additional documentation for reviewer. Engage regulatory consultant for expedited review. Update project timeline.", "medium"),
        (23, "https://hr.best-practices.dev/onboarding", "Onboarding Best Practices", "Automate account provisioning. Create pre-boarding checklist. Implement buddy system. Reduce approvals from 5 to 2.", "medium"),
        (26, "https://www.nacm.org/collections", "Credit & Collections Guide", "Implement automated payment reminders at 30/60/90 days. Escalate accounts over $50K to collections. Offer payment plans.", "high"),
        (28, "https://www.cisa.gov/physical-security", "CISA Physical Security", "Upgrade access card system firmware. Implement failed-attempt lockout. Add security camera coverage to entry points.", "high"),
        (29, "https://hl7.org/fhir/migration", "HL7 FHIR Migration Guide", "Deploy validation rule fixes for 18% stuck records. Run batch re-validation. Implement parallel validation pipeline.", "medium"),
    ]

    for issue_id, url, title, summary, conf in solutions_data:
        ts = (now - timedelta(days=random.randint(0, 10))).strftime("%Y-%m-%d %H:%M:%S")
        conn.execute(
            "INSERT INTO solutions (issue_id, source_url, source_title, summary, confidence, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            (issue_id, url, title, summary, conf, ts)
        )

    conn.execute(
        "INSERT INTO scan_cursor (id, last_learning_id, last_scan_at, last_digest_at, last_action_at) VALUES (1, 50, ?, ?, ?)",
        (now.strftime("%Y-%m-%d %H:%M:%S"), (now - timedelta(hours=2)).strftime("%Y-%m-%d %H:%M:%S"), now.strftime("%Y-%m-%d %H:%M:%S"))
    )

    conn.commit()
    conn.close()
    print(f"Created {path} with {len(issues_data)} issues and {len(solutions_data)} solutions")


def create_mesh_db():
    """Create mesh.db with sample clusters, confluences, and lvl2 analyses."""
    path = os.path.join(OUTPUT_DIR, "mesh.db")
    if os.path.exists(path):
        os.remove(path)

    conn = sqlite3.connect(path)
    conn.execute("PRAGMA journal_mode=WAL")

    # Issue clusters
    conn.execute("""CREATE TABLE issue_clusters (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT, heat_score REAL, issue_count INTEGER,
        member_ids TEXT, severity_breakdown TEXT,
        computed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )""")

    # Solution clusters
    conn.execute("""CREATE TABLE clusters (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT, heat_score REAL, solution_count INTEGER,
        member_files TEXT, severity_breakdown TEXT,
        computed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )""")

    # Confluences
    conn.execute("""CREATE TABLE confluences (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        issue_cluster_id INTEGER, solution_cluster_id INTEGER,
        issue_cluster_name TEXT, solution_cluster_name TEXT,
        topical_similarity REAL, issue_z_score REAL, solution_z_score REAL,
        confluence_score REAL, status TEXT,
        computed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(issue_cluster_id, solution_cluster_id)
    )""")

    # Lvl2 analyses
    conn.execute("""CREATE TABLE lvl2_analyses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        cluster_id INTEGER, cluster_name TEXT,
        output_path TEXT, strategy_summary TEXT, auto_actions TEXT,
        generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )""")

    conn.execute("""CREATE TABLE issue_lvl2_analyses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        cluster_id INTEGER, cluster_name TEXT,
        output_path TEXT, strategy_summary TEXT, auto_actions TEXT,
        generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )""")

    # Embeddings (minimal)
    conn.execute("""CREATE TABLE issue_embeddings (
        id INTEGER PRIMARY KEY, issue_id INTEGER UNIQUE, title TEXT,
        content_hash TEXT, embedding TEXT, embedded_at TIMESTAMP
    )""")
    conn.execute("""CREATE TABLE solution_embeddings (
        id INTEGER PRIMARY KEY, filename TEXT UNIQUE, title TEXT,
        content_hash TEXT, embedding TEXT, embedded_at TIMESTAMP
    )""")
    conn.execute("""CREATE TABLE issue_edges (
        id INTEGER PRIMARY KEY, issue_id_a INTEGER, issue_id_b INTEGER,
        similarity REAL, computed_at TIMESTAMP, UNIQUE(issue_id_a, issue_id_b)
    )""")
    conn.execute("""CREATE TABLE solution_edges (
        id INTEGER PRIMARY KEY, file_a TEXT, file_b TEXT,
        similarity REAL, computed_at TIMESTAMP, UNIQUE(file_a, file_b)
    )""")
    conn.execute("""CREATE TABLE scan_cursor (
        id INTEGER PRIMARY KEY, last_mesh_at DATETIME, last_lvl2_at DATETIME,
        last_issue_mesh_at DATETIME, last_issue_lvl2_at DATETIME,
        last_confluence_at DATETIME, similarity_threshold REAL DEFAULT 0.55,
        issue_sim_threshold REAL DEFAULT 0.4
    )""")
    conn.execute("""CREATE TABLE issue_cluster_centroids (
        cluster_id INTEGER PRIMARY KEY, centroid_embedding TEXT, computed_at TIMESTAMP
    )""")
    conn.execute("""CREATE TABLE solution_cluster_centroids (
        cluster_id INTEGER PRIMARY KEY, centroid_embedding TEXT, computed_at TIMESTAMP
    )""")
    conn.execute("""CREATE TABLE targeting_log (
        id INTEGER PRIMARY KEY, issue_id INTEGER, targeting_score REAL,
        reason TEXT, targeting_status TEXT, decided_at TIMESTAMP
    )""")
    conn.execute("""CREATE TABLE debloat_metrics (
        id INTEGER PRIMARY KEY, cycle_timestamp TIMESTAMP,
        items_added INTEGER DEFAULT 0, items_deleted INTEGER DEFAULT 0, bytes_freed INTEGER DEFAULT 0
    )""")

    # Issue clusters
    issue_clusters = [
        ("Healthcare Compliance & Patient Safety", 2847.5, 8, [1, 5, 8, 15, 18, 20, 28, 4], {"critical": 3, "high": 4, "medium": 1}),
        ("Financial Risk & Cash Management", 1923.1, 7, [2, 14, 16, 21, 22, 24, 26], {"critical": 2, "high": 2, "medium": 3}),
        ("Technical Debt & Security", 1456.8, 6, [6, 7, 11, 17, 25, 29], {"critical": 1, "high": 2, "medium": 3}),
        ("Operational Efficiency", 892.4, 5, [3, 9, 12, 19, 23], {"high": 2, "medium": 3}),
        ("Legal & Regulatory Compliance", 634.2, 4, [10, 13, 27, 30], {"critical": 1, "high": 2, "medium": 1}),
    ]

    for name, heat, count, members, breakdown in issue_clusters:
        conn.execute(
            "INSERT INTO issue_clusters (name, heat_score, issue_count, member_ids, severity_breakdown) VALUES (?, ?, ?, ?, ?)",
            (name, heat, count, json.dumps(members), json.dumps(breakdown))
        )

    # Solution clusters
    sol_clusters = [
        ("Healthcare & Safety Remediation", 1245.3, 7, json.dumps(["hipaa_solution.md", "consent_solution.md", "calibration_solution.md"]), json.dumps({"critical": 0, "high": 5, "medium": 2})),
        ("Financial Strategy & Recovery", 876.9, 5, json.dumps(["tax_solution.md", "cashflow_solution.md", "collections_solution.md"]), json.dumps({"critical": 0, "high": 3, "medium": 2})),
        ("Technical Security & Performance", 654.1, 7, json.dumps(["ratelimit_solution.md", "tls_solution.md", "model_drift_solution.md"]), json.dumps({"critical": 0, "high": 4, "medium": 3})),
    ]

    for name, heat, count, members, breakdown in sol_clusters:
        conn.execute(
            "INSERT INTO clusters (name, heat_score, solution_count, member_files, severity_breakdown) VALUES (?, ?, ?, ?, ?)",
            (name, heat, count, members, breakdown)
        )

    # Confluences
    confluences = [
        (1, 1, "Healthcare Compliance & Patient Safety", "Healthcare & Safety Remediation", 0.92, 3.4, 2.8, 0.89, "met"),
        (2, 2, "Financial Risk & Cash Management", "Financial Strategy & Recovery", 0.87, 2.9, 2.5, 0.82, "met"),
        (3, 3, "Technical Debt & Security", "Technical Security & Performance", 0.84, 2.7, 3.1, 0.78, "met"),
        (4, 1, "Operational Efficiency", "Healthcare & Safety Remediation", 0.41, 1.2, 0.8, 0.35, "unmet"),
        (1, 2, "Healthcare Compliance & Patient Safety", "Financial Strategy & Recovery", 0.38, 1.1, 0.6, 0.31, "unmet"),
        (5, 3, "Legal & Regulatory Compliance", "Technical Security & Performance", 0.34, 0.9, 0.7, 0.28, "unmet"),
        (4, 2, "Operational Efficiency", "Financial Strategy & Recovery", 0.25, 0.6, 0.5, 0.18, "gap"),
        (4, 3, "Operational Efficiency", "Technical Security & Performance", 0.22, 0.5, 0.4, 0.15, "gap"),
        (5, 1, "Legal & Regulatory Compliance", "Healthcare & Safety Remediation", 0.28, 0.7, 0.3, 0.19, "gap"),
        (5, 2, "Legal & Regulatory Compliance", "Financial Strategy & Recovery", 0.21, 0.4, 0.3, 0.14, "gap"),
        (2, 1, "Financial Risk & Cash Management", "Healthcare & Safety Remediation", 0.19, 0.3, 0.2, 0.11, "gap"),
        (1, 3, "Healthcare Compliance & Patient Safety", "Technical Security & Performance", 0.15, 0.2, 0.4, 0.09, "distant"),
        (2, 3, "Financial Risk & Cash Management", "Technical Security & Performance", 0.12, 0.2, 0.3, 0.07, "distant"),
        (3, 1, "Technical Debt & Security", "Healthcare & Safety Remediation", 0.31, 0.8, 0.5, 0.22, "gap"),
        (3, 2, "Technical Debt & Security", "Financial Strategy & Recovery", 0.18, 0.3, 0.2, 0.10, "distant"),
    ]

    for ic_id, sc_id, ic_name, sc_name, sim, iz, sz, score, status in confluences:
        conn.execute(
            "INSERT INTO confluences (issue_cluster_id, solution_cluster_id, issue_cluster_name, solution_cluster_name, topical_similarity, issue_z_score, solution_z_score, confluence_score, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (ic_id, sc_id, ic_name, sc_name, sim, iz, sz, score, status)
        )

    # Lvl2 analyses
    lvl2_data = [
        ("Healthcare Compliance & Patient Safety", "Deploy end-to-end encryption for patient data pipeline", json.dumps(["db_update:encryption_config", "telegram:alert_compliance_team"])),
        ("Financial Strategy & Recovery", "Implement automated tax estimation and payment scheduling", json.dumps(["db_update:tax_schedule", "telegram:alert_finance"])),
        ("Technical Security Hardening", "Upgrade all endpoints to TLS 1.3 with automated cert management", json.dumps(["db_update:tls_config", "telegram:alert_devops"])),
        ("Access Control Remediation", "Deploy MFA and automated access review", json.dumps(["db_update:mfa_policy"])),
        ("Rate Limiting & API Security", "Implement tiered rate limiting with circuit breaker", json.dumps(["db_update:rate_limits", "telegram:alert_security"])),
        ("Regulatory Submission Strategy", "Prepare expedited FDA submission with consultant engagement", json.dumps([])),
        ("Cash Flow Optimization", "Reduce burn rate and accelerate revenue pipeline", json.dumps([])),
        ("Operational Efficiency Improvement", "Automate onboarding and ticket triage workflows", json.dumps([])),
    ]

    for name, summary, actions in lvl2_data:
        conn.execute(
            "INSERT INTO lvl2_analyses (cluster_id, cluster_name, output_path, strategy_summary, auto_actions) VALUES (?, ?, ?, ?, ?)",
            (1, name, f"solutions/lvl2/{name.lower().replace(' ', '_')}_lvl2.md", summary, actions)
        )

    # Issue lvl2 analyses
    issue_lvl2_data = [
        ("Healthcare Compliance Gaps", "Systematic compliance gap analysis across all patient-facing systems"),
        ("Financial Risk Cluster", "Consolidated financial risk assessment with mitigation roadmap"),
        ("Security Vulnerability Cluster", "End-to-end security audit remediation plan"),
        ("Operational Bottleneck Analysis", "Process optimization for support, onboarding, and maintenance workflows"),
    ]

    for name, summary in issue_lvl2_data:
        conn.execute(
            "INSERT INTO issue_lvl2_analyses (cluster_id, cluster_name, output_path, strategy_summary, auto_actions) VALUES (?, ?, ?, ?, ?)",
            (1, name, f"solutions/issue_lvl2/{name.lower().replace(' ', '_')}_issue_lvl2.md", summary, json.dumps([]))
        )

    now = datetime.now()
    conn.execute(
        "INSERT INTO scan_cursor (id, last_mesh_at, last_lvl2_at, last_issue_mesh_at, last_confluence_at) VALUES (1, ?, ?, ?, ?)",
        (now.strftime("%Y-%m-%d %H:%M:%S"),) * 4
    )

    conn.commit()
    conn.close()
    print(f"Created {path} with clusters, confluences, and lvl2 analyses")


if __name__ == "__main__":
    create_learnings_db()
    create_research_db()
    create_mesh_db()
    print(f"\nSample data ready in {OUTPUT_DIR}/")
