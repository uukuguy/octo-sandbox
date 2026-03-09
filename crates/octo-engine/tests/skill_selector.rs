use octo_engine::skills::selector::{
    ScoredSkill, SkillAttenuator, SkillBudget, SkillGate, SkillScorer, SkillSelector,
};
use octo_engine::skills::trust::TrustManager;
use octo_types::skill::{SkillDefinition, SkillSourceType, SkillTrigger, TrustLevel};

fn make_skill(name: &str) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: "test skill".into(),
        version: None,
        user_invocable: false,
        allowed_tools: None,
        body: String::new(),
        base_dir: std::path::PathBuf::new(),
        source_path: std::path::PathBuf::new(),
        body_loaded: false,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::Installed,
        triggers: vec![],
        dependencies: vec![],
        tags: vec![],
        denied_tools: None,
        source_type: SkillSourceType::ProjectLocal,
    }
}

// ── Phase 1: Gate ──

#[test]
fn test_gate_passes_all() {
    let gate = SkillGate;
    let s1 = make_skill("alpha");
    let mut s2 = make_skill("beta");
    s2.source_type = SkillSourceType::Registry;
    s2.trust_level = TrustLevel::Unknown;
    let mut s3 = make_skill("gamma");
    s3.always = true;

    assert!(gate.passes(&s1));
    assert!(gate.passes(&s2));
    assert!(gate.passes(&s3));
}

// ── Phase 2: Scorer ──

#[test]
fn test_scorer_always_gets_1000() {
    let scorer = SkillScorer;
    let mut skill = make_skill("deploy");
    skill.always = true;
    assert_eq!(scorer.score(&skill, "anything"), 1000);
}

#[test]
fn test_scorer_slash_command_900() {
    let scorer = SkillScorer;
    let skill = make_skill("deploy");
    assert_eq!(scorer.score(&skill, "/deploy production"), 900);
}

#[test]
fn test_scorer_slash_command_case_insensitive() {
    let scorer = SkillScorer;
    let skill = make_skill("Deploy");
    assert_eq!(scorer.score(&skill, "/deploy production"), 900);
}

#[test]
fn test_scorer_keyword_trigger() {
    let scorer = SkillScorer;
    let mut skill = make_skill("search");
    skill.triggers = vec![
        SkillTrigger::Keyword {
            keyword: "find".into(),
        },
        SkillTrigger::Keyword {
            keyword: "search".into(),
        },
    ];
    // Both keywords match -> 10 + 10 = 20
    assert_eq!(scorer.score(&skill, "find and search files"), 20);
    // One keyword matches -> 10
    assert_eq!(scorer.score(&skill, "find the file"), 10);
}

#[test]
fn test_scorer_command_trigger() {
    let scorer = SkillScorer;
    let mut skill = make_skill("git-helper");
    skill.triggers = vec![SkillTrigger::Command {
        command: "git commit".into(),
    }];
    assert_eq!(scorer.score(&skill, "git commit -m 'msg'"), 800);
}

#[test]
fn test_scorer_file_pattern_trigger() {
    let scorer = SkillScorer;
    let mut skill = make_skill("rust-fmt");
    skill.triggers = vec![SkillTrigger::FilePattern {
        pattern: ".rs".into(),
    }];
    assert_eq!(scorer.score(&skill, "format main.rs"), 20);
}

#[test]
fn test_scorer_no_match_zero() {
    let scorer = SkillScorer;
    let mut skill = make_skill("deploy");
    skill.triggers = vec![SkillTrigger::Keyword {
        keyword: "deploy".into(),
    }];
    assert_eq!(scorer.score(&skill, "hello world"), 0);
}

#[test]
fn test_scorer_command_beats_keyword() {
    let scorer = SkillScorer;
    let mut skill = make_skill("helper");
    skill.triggers = vec![
        SkillTrigger::Command {
            command: "run test".into(),
        },
        SkillTrigger::Keyword {
            keyword: "test".into(),
        },
    ];
    // Command match (800) + keyword match (10) -> max(800, 0) then +10 = 810
    // Actually: command sets score = max(score, 800) = 800, keyword adds 10 -> 810
    assert_eq!(scorer.score(&skill, "run test suite"), 810);
}

// ── Phase 3: Budget ──

#[test]
fn test_budget_selects_within_limit() {
    let budget = SkillBudget::new(100);
    let mut scored = vec![
        ScoredSkill {
            name: "a".into(),
            score: 500,
            estimated_tokens: 60,
        },
        ScoredSkill {
            name: "b".into(),
            score: 400,
            estimated_tokens: 60,
        },
    ];
    let selected = budget.select(&mut scored);
    // Only "a" fits (60 <= 100), "b" would exceed (120 > 100)
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "a");
}

#[test]
fn test_budget_always_included() {
    let budget = SkillBudget::new(50); // Very tight budget
    let mut scored = vec![
        ScoredSkill {
            name: "always-on".into(),
            score: 1000,
            estimated_tokens: 200,
        },
        ScoredSkill {
            name: "normal".into(),
            score: 500,
            estimated_tokens: 30,
        },
    ];
    let selected = budget.select(&mut scored);
    // always-on is included despite exceeding budget; normal would push over
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "always-on");
}

#[test]
fn test_budget_skips_zero_score() {
    let budget = SkillBudget::new(1000);
    let mut scored = vec![
        ScoredSkill {
            name: "matched".into(),
            score: 100,
            estimated_tokens: 10,
        },
        ScoredSkill {
            name: "unmatched".into(),
            score: 0,
            estimated_tokens: 10,
        },
    ];
    let selected = budget.select(&mut scored);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "matched");
}

// ── Phase 4: Attenuator ──

#[test]
fn test_attenuator_trust_levels() {
    let tm = TrustManager::default();
    let attenuator = SkillAttenuator::new(tm);

    let mut trusted_skill = make_skill("local");
    trusted_skill.trust_level = TrustLevel::Trusted;
    trusted_skill.source_type = SkillSourceType::ProjectLocal;
    assert_eq!(
        attenuator.effective_trust(&trusted_skill),
        TrustLevel::Trusted
    );

    let mut registry_skill = make_skill("remote");
    registry_skill.trust_level = TrustLevel::Trusted;
    registry_skill.source_type = SkillSourceType::Registry;
    assert_eq!(
        attenuator.effective_trust(&registry_skill),
        TrustLevel::Unknown
    );

    let mut user_skill = make_skill("user");
    user_skill.trust_level = TrustLevel::Trusted;
    user_skill.source_type = SkillSourceType::UserLocal;
    assert_eq!(
        attenuator.effective_trust(&user_skill),
        TrustLevel::Installed
    );
}

// ── Full Pipeline ──

#[test]
fn test_full_pipeline_select() {
    let selector = SkillSelector::new(5000, TrustManager::default());

    let mut always_skill = make_skill("always-on");
    always_skill.always = true;
    always_skill.trust_level = TrustLevel::Trusted;

    let mut keyword_skill = make_skill("searcher");
    keyword_skill.triggers = vec![SkillTrigger::Keyword {
        keyword: "search".into(),
    }];
    keyword_skill.trust_level = TrustLevel::Installed;

    let mut unmatched_skill = make_skill("deployer");
    unmatched_skill.triggers = vec![SkillTrigger::Keyword {
        keyword: "deploy".into(),
    }];

    let skills = vec![always_skill, keyword_skill, unmatched_skill];
    let result = selector.select(&skills, "search for files");

    // always-on (score 1000) + searcher (score 10) selected; deployer (score 0) excluded
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "always-on");
    assert_eq!(result[0].score, 1000);
    assert_eq!(result[0].trust_level, TrustLevel::Trusted);
    assert_eq!(result[1].name, "searcher");
    assert_eq!(result[1].score, 10);
    assert_eq!(result[1].trust_level, TrustLevel::Installed);
}

#[test]
fn test_pipeline_empty_skills() {
    let selector = SkillSelector::new(5000, TrustManager::default());
    let result = selector.select(&[], "hello");
    assert!(result.is_empty());
}

#[test]
fn test_pipeline_no_matching_message() {
    let selector = SkillSelector::new(5000, TrustManager::default());

    let mut skill = make_skill("deploy");
    skill.triggers = vec![SkillTrigger::Keyword {
        keyword: "deploy".into(),
    }];

    let result = selector.select(&[skill], "hello world");
    assert!(result.is_empty());
}
