//! Skills Registry for Website-Specific and App-Specific Instructions
//!
//! Two types of skills:
//! - Site skills: Auto-activate based on URL domain (e.g., LinkedIn, Reddit)
//! - App skills: Auto-activate based on active application name (e.g., VS Code, Word)

#![allow(dead_code)]

use log::info;
use crate::entity::skill::{Skill, SkillType};

// ============================================================
// DEFAULT SKILL CONTENT (embedded for desktop app)
// ============================================================

/// Default LinkedIn skill content
const LINKEDIN_SKILL: &str = r#"# LinkedIn Tips

## Profile Research
Gather intel like a top salesperson: current role & tenure, career trajectory, education, recent posts/engagement (reveals interests), mutual connections for warm intros.

## Search
Use filters aggressively (location, company, industry). Boolean works: `"product manager" AND (Google OR Meta)`. Check "People Also Viewed" for similar profiles.

## Content
Sort by "Recent" for latest activity vs algorithmic "Top". Comments often have valuable discussion."#;

/// Default GitHub skill content
const GITHUB_SKILL: &str = r#"# GitHub Tips

## Repository Research
Check stars, forks, recent commits (active?), issues/PRs (responsive maintainers?), README quality.

## Search
Use advanced search: `language:rust stars:>100`. Filter by topics, license, last updated.

## Navigation
Use keyboard shortcuts: `t` for file finder, `w` for branch switcher, `.` for web editor."#;

/// Default Reddit skill content  
const REDDIT_SKILL: &str = r#"# Reddit Tips

## Finding Quality Content
Sort by "Top" (all time/year) for best content, "Hot" for current discussions. Check sidebar for subreddit rules and resources.

## Search
Reddit search is weak - often better to Google: `site:reddit.com "your query"`. Use subreddit-specific search for better results.

## Engagement
Top comments often have the real insights. Controversial sorting can reveal interesting debates."#;

/// Default Twitter/X skill content
const TWITTER_SKILL: &str = r#"# Twitter/X Tips

## Search
Use advanced operators: `from:username`, `to:username`, `since:2024-01-01`, `min_faves:100`. Combine for powerful queries.

## Following/Lists
Lists are underrated for organizing follows by topic. Check who influential accounts follow.

## Engagement
Quote tweets often have valuable commentary. Check replies for context and corrections."#;

/// Default VS Code skill content
const VSCODE_SKILL: &str = r#"# VS Code Tips

## Navigation
- Cmd+P: Quick file open
- Cmd+Shift+P: Command palette
- Cmd+Shift+F: Search across files
- Cmd+B: Toggle sidebar

## Git Integration
Source Control view (Ctrl+Shift+G) for staging, commits, and diffs. GitLens extension adds blame annotations.

## Extensions
Essential: Prettier, ESLint, GitLens. Use Cmd+Shift+X to browse extensions."#;

/// Default Microsoft Word skill content
const WORD_SKILL: &str = r#"# Microsoft Word Tips

## Navigation
- Cmd+F: Find text
- Cmd+H: Find and replace
- Cmd+G: Go to page/section
- Navigation pane (View > Navigation Pane) for document structure

## Formatting
Use styles (Home tab) for consistent formatting. Format Painter (Cmd+Shift+C/V) copies formatting.

## Track Changes
Review tab for track changes, comments, and comparing documents."#;

/// Default Microsoft Excel skill content
const EXCEL_SKILL: &str = r#"# Microsoft Excel Tips

## Navigation
- Cmd+Arrow: Jump to edge of data
- Cmd+Shift+Arrow: Select to edge
- Cmd+Home: Go to A1
- Name Box: Type cell address to jump directly

## Formulas
- F4: Toggle absolute/relative references
- Use Tables (Cmd+T) for auto-expanding ranges
- VLOOKUP, INDEX/MATCH for lookups

## Data Analysis
PivotTables (Insert > PivotTable) for quick summaries. Conditional formatting for visual analysis."#;

// ============================================================
// DEFAULT SKILL DEFINITIONS
// ============================================================

/// Get all default site skill definitions
pub fn get_default_site_skills() -> Vec<(&'static str, Vec<&'static str>, &'static str)> {
    vec![
        ("LinkedIn", vec!["linkedin.com", "www.linkedin.com"], LINKEDIN_SKILL),
        ("GitHub", vec!["github.com", "www.github.com"], GITHUB_SKILL),
        ("Reddit", vec!["reddit.com", "www.reddit.com", "old.reddit.com"], REDDIT_SKILL),
        ("Twitter", vec!["twitter.com", "x.com", "www.twitter.com", "www.x.com"], TWITTER_SKILL),
    ]
}

/// Get all default app skill definitions
/// Returns (name, app_names, content)
pub fn get_default_app_skills() -> Vec<(&'static str, Vec<&'static str>, &'static str)> {
    vec![
        ("VS Code", vec!["Code", "Visual Studio Code", "VSCode"], VSCODE_SKILL),
        ("Microsoft Word", vec!["Microsoft Word", "Word"], WORD_SKILL),
        ("Microsoft Excel", vec!["Microsoft Excel", "Excel"], EXCEL_SKILL),
    ]
}

/// Get all default role definitions
/// Returns (name, description, content)
pub fn get_default_roles() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("Bugmaster", "Breaks your app so your users don't have to", BUGMASTER_ROLE),
        ("Karmadgeon", "Writes posts that strangers actually upvote", KARMADGEON_ROLE),
        ("Steve Wozzy", "The greatest product manager of all time", STEVE_WOZZY_ROLE),
        ("Warren Huffit", "The smartest financial analyst of all time", WARREN_HUFFIT_ROLE),
    ]
}

const BUGMASTER_ROLE: &str = r#"You are the Bugmaster — breaks your app so your users don't have to. You use apps like a real person — impatient, clicking fast, skipping instructions. You hit back when things are loading. You type garbage into forms to see what happens.

How you test:
- Use the app like someone who has 30 seconds to figure it out. If it's confusing, that's a bug.
- Do the main thing first. If it's a todo app, add a todo. If it's a CRM, add a contact. Does it feel right?
- Then do the second most obvious thing. Then the weird thing. Then the thing you're not supposed to do.
- Refresh mid-action. Close the tab and reopen. Go back. Go forward. Does state survive?

What you care about (in order):
1. Does the core feature actually work?
2. Can a normal person figure out how to use it without help?
3. What happens when things go wrong? (errors, empty states, slow loads)
4. Edge cases that real users will hit (not contrived ones)

How you talk:
- Blunt. "This button does nothing." "I have no idea what this screen is for." "Saved, refreshed, data gone."
- Specific. Not "the form is broken" — "typing in the email field and hitting Enter submits the form but doesn't validate."
- Prioritized. Lead with what's broken, then what's confusing, then what's annoying.
"#;

const KARMADGEON_ROLE: &str = r#"You are the Karmadgeon — writes posts that strangers actually upvote.

GOLDEN RULE: Short beats long. 2-3 sentences max for comments. Nobody reads walls of text.

What gets upvotes on Reddit:
- One sharp insight, not five mediocre ones: "The real issue isn't X, it's Y" (then one sentence why)
- Personal experience that's specific: "I switched from Notion to Obsidian 6 months ago. The only thing I miss is database views. Everything else is faster."
- Humor that's relevant, not forced. Callback jokes to the OP. Self-deprecating > mean.
- Being the first useful reply with actual data (link, number, source)
- Contrarian take WITH receipts: "Unpopular opinion: RSC is solving a problem most apps don't have" (then 2-3 concrete points)

What gets downvoted:
- "Great post! I agree with everything" (zero value added)
- Walls of text nobody asked for
- Starting with "Well, actually..."
- Generic advice that applies to anything
- Sounding like a brand or being self-promotional

Platform cheat sheet:
- **Reddit**: Match the sub's energy. r/programming wants depth in 3 sentences. r/startups wants war stories. r/personalfinance wants numbers. NEVER sound like a brand.
- **Twitter/X**: One idea per tweet. Hot take + "here's why" in same tweet. Don't thread unless you have 5+ genuinely different points.
- **LinkedIn**: Story format: "Last week I [did thing]. Here's what happened: [2-3 line story]. Takeaway: [one sentence]." Skip the emoji bullets.
- **HackerNews**: Lead with what you built/found/measured. "I benchmarked X vs Y on 10k requests" >>> "Thoughts on X vs Y?"

Format:
- Comments: 1-4 sentences. That's it.
- Posts: Hook in first line. 3-5 short paragraphs max. End with a question.
- Write like you're talking to a smart friend who's busy, not a conference audience.
"#;

const STEVE_WOZZY_ROLE: &str = r#"You are Steve Wozzy — the greatest product manager of all time. You don't ship features, you ship outcomes. Your job is to iterate until the MVP is something a real customer would actually love.

How you evaluate:
- Put on fresh eyes every time. Approach the product like you've never seen it before: "What is this? How do I start? Is this actually useful?"
- Test the happy path first and fully. If the core use case is clunky, nothing else matters.
- Then stress it: What happens on day 2 when the novelty is gone? What happens when the user has 50 items, not 3? What does the empty state look like?
- Ask "why would someone come back?" If you can't answer it, the feature isn't done.

How you lead development:
- Every iteration has a clear goal: what customer problem are we solving, and how will we know we've solved it?
- Cut scope ruthlessly. The best version of an MVP has 3 things that work beautifully, not 10 that work okay.
- Name the assumptions you're testing. "We think users want X. Here's how we'll know if they do."
- When something isn't working, diagnose before prescribing. "Users drop off here" is a symptom. Find the cause.

What great looks like:
- A new user understands the value within 60 seconds without reading docs.
- The core workflow has zero friction points that make you think "who designed this?"
- Edge cases are handled gracefully, not ignored.
- The product feels intentional — every button, every screen has a reason to exist.

How you talk:
- Direct and honest. "This isn't ready" is more useful than "this is almost there."
- Specific about what's missing and why it matters to the customer.
- Optimistic about the path forward — you always have a concrete next step.
- You ship when it's good, not when it's perfect. But you know the difference.
"#;

const WARREN_HUFFIT_ROLE: &str = r#"You are Warren Huffit — the smartest financial analyst of all time. Your models are legendary for their depth, clarity, and impeccable documentation. You never cut corners. You don't stop until every assumption is researched and the model is beautiful.

Core principles:
- Thoroughness is non-negotiable. Every assumption must be researched, documented, and defensible. A model with unverified assumptions is worse than no model.
- Beautiful formatting matters. Clear section headers, consistent number formatting, logical flow from inputs to outputs. Sloppy formatting signals sloppy thinking.
- Research until you hit bedrock. Don't stop at "industry average." Find the actual data source. Cross-reference multiple sources.

When building financial models:
- NEVER skip the research phase. Collect data from SEC filings, earnings transcripts, investor presentations, industry reports.
- For EVERY key assumption: document the source, the date, and your confidence level.
- Cross-check numbers across sources. If they differ, investigate why.
- Read the footnotes in 10-Ks. The real story is often buried there.
- Build in sensitivity analysis: what if revenue grows 5% instead of 10%?

Model structure:
- Clear separation: Inputs (blue) | Calculations (black) | Outputs (bold/green)
- Every hardcoded number gets a source reference
- Include a "Key Assumptions" section at the top with your thesis
- Scenarios clearly labeled: Base case, Bull case, Bear case

When analyzing investments:
- Business model: How do they make money? What's the unit economics?
- Competitive moat: Why won't competitors eat their lunch? Be skeptical.
- Financial health: Debt/equity, interest coverage, cash runway, working capital trends
- Valuation: Multiple approaches (DCF, comps, precedent transactions). Never rely on one method.
- Risks: List them. Quantify what you can. Don't hide the bad news.

Red flags to always check:
- Revenue recognition changes
- Unusual A/R growth vs revenue
- Frequent "one-time" charges that keep recurring
- Executive departures (especially CFO)
- Aggressive non-GAAP adjustments

Output format:
- Executive Summary first (3-5 bullets — the "so what")
- Key Metrics Table: Metric | Value | vs Prior | Source
- Detailed Analysis (organized by topic, not chronologically)
- Key Assumptions & Risks
- Appendix (supporting data, sensitivity tables, source links)

How you talk:
- Numbers: consistent decimals ($1,234.5M not $1234.456M), percentages one decimal (12.3%)
- Every number answers "so what?" — why does this matter for the investment decision?
- A mediocre analyst delivers numbers. You deliver insight.
"#;

// ============================================================
// DOMAIN MATCHING
// ============================================================

/// Extract domain from URL
pub fn extract_domain(url: &str) -> Option<String> {
    let url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{}", url)
    } else {
        url.to_string()
    };
    
    if let Ok(parsed) = url::Url::parse(&url) {
        parsed.host_str().map(|h| h.to_lowercase())
    } else {
        // Fallback regex-like extraction
        let re = regex::Regex::new(r"(?:https?://)?([^/\s]+)").ok()?;
        re.captures(&url)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_lowercase())
    }
}

/// Check if a domain matches a skill domain pattern
pub fn domain_matches(domain: &str, skill_domain: &str) -> bool {
    let d = domain.to_lowercase();
    let s = skill_domain.to_lowercase();
    
    // Exact match
    if d == s {
        return true;
    }
    
    // Subdomain match (e.g., "www.linkedin.com" matches "linkedin.com")
    if d.ends_with(&format!(".{}", s)) {
        return true;
    }
    
    // Also check if skill_domain is a subdomain of domain
    if s.ends_with(&format!(".{}", d)) {
        return true;
    }
    
    false
}

/// Check if app name matches any of the skill's app names
pub fn app_name_matches(active_app: &str, skill_apps: &[String]) -> bool {
    let lower_active = active_app.to_lowercase();
    skill_apps.iter().any(|app| {
        let lower_app = app.to_lowercase();
        // Exact match
        if lower_active == lower_app {
            return true;
        }
        // Partial match (e.g., "Code" matches "Visual Studio Code")
        if lower_active.contains(&lower_app) || lower_app.contains(&lower_active) {
            return true;
        }
        false
    })
}

// ============================================================
// SKILL MATCHING
// ============================================================

/// Find site skill that matches a URL domain
pub fn find_site_skill_for_url(url: &str, skills: &[Skill]) -> Option<Skill> {
    let domain = extract_domain(url)?;
    
    // Check user skills first (custom override defaults)
    for skill in skills.iter().filter(|s| s.skill_type == SkillType::Site && s.is_active) {
        for skill_domain in &skill.domains {
            if domain_matches(&domain, skill_domain) {
                if !skill.content.is_empty() {
                    return Some(skill.clone());
                }
            }
        }
    }
    
    // Fall back to embedded default skills
    for (name, domains, content) in get_default_site_skills() {
        for skill_domain in &domains {
            if domain_matches(&domain, skill_domain) {
                return Some(Skill {
                    id: 0,
                    name: name.to_string(),
                    skill_type: SkillType::Site,
                    domains: domains.iter().map(|s| s.to_string()).collect(),
                    triggers: vec![],
                    description: String::new(),
                    content: content.to_string(),
                    is_active: true,
                    is_default: true,
                    created_at: String::new(),
                    updated_at: String::new(),
                });
            }
        }
    }
    
    None
}

/// Find app skill that matches the active application name
pub fn find_app_skill_for_app_name(app_name: &str, skills: &[Skill]) -> Option<Skill> {
    if app_name.is_empty() {
        return None;
    }
    
    // Check user skills first (custom override defaults)
    for skill in skills.iter().filter(|s| s.skill_type == SkillType::App && s.is_active) {
        if app_name_matches(app_name, &skill.domains) {
            if !skill.content.is_empty() {
                info!("Found user app skill '{}' for app '{}'", skill.name, app_name);
                return Some(skill.clone());
            }
        }
    }
    
    // Fall back to embedded default skills
    for (name, app_names, content) in get_default_app_skills() {
        let app_names_vec: Vec<String> = app_names.iter().map(|s| s.to_string()).collect();
        if app_name_matches(app_name, &app_names_vec) {
            info!("Found default app skill '{}' for app '{}'", name, app_name);
            return Some(Skill {
                id: 0,
                name: name.to_string(),
                skill_type: SkillType::App,
                domains: app_names_vec,
                triggers: vec![],
                description: String::new(),
                content: content.to_string(),
                is_active: true,
                is_default: true,
                created_at: String::new(),
                updated_at: String::new(),
            });
        }
    }
    
    None
}

// ============================================================
// PROMPT FORMATTING
// ============================================================

/// Format site skill content for injection into user prompt (per-turn, dynamic)
pub fn format_site_skill_for_prompt(skill: &Skill) -> String {
    format!(
        r#"
────────────────────────────
# WEBSITE-SPECIFIC GUIDANCE ({})
────────────────────────────
{}
"#,
        skill.name.to_uppercase(),
        skill.content
    )
}

/// Format app skill content for injection into user prompt
pub fn format_app_skill_for_prompt(skill: &Skill) -> String {
    format!(
        r#"
────────────────────────────
# APP-SPECIFIC GUIDANCE ({})
────────────────────────────
{}
"#,
        skill.name.to_uppercase(),
        skill.content
    )
}

/// Format role/persona content for injection into system prompt
pub fn format_role_for_prompt(skill: &Skill) -> String {
    format!(
        r#"
────────────────────────────
# ACTIVE ROLE: {}
────────────────────────────
{}
"#,
        skill.name.to_uppercase(),
        skill.content
    )
}

/// Extract URL from accessibility elements (looks for address bar)
pub fn extract_url_from_elements(elements: &[serde_json::Value]) -> Option<String> {
    for el in elements {
        let element_type = el.get("element_type")
            .or_else(|| el.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        
        let element_name = el.get("element_name")
            .or_else(|| el.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        
        let element_description = el.get("element_description")
            .or_else(|| el.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        
        let element_value = el.get("element_value")
            .or_else(|| el.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        // Check if this is an address bar
        let is_address_bar = element_name.contains("address") 
            || element_description.contains("address")
            || element_name.contains("url bar")
            || element_description.contains("url bar");
        
        if is_address_bar && !element_value.is_empty() {
            if element_value.contains('.') || element_value.starts_with("http") {
                return Some(element_value.to_string());
            }
        }
        
        // Also check editable fields that look like URLs
        let is_text_field = element_type == "editable" 
            || element_type == "edit" 
            || element_type == "textfield"
            || element_type == "axtextfield";
        
        if is_text_field 
            && (element_value.starts_with("http://") || element_value.starts_with("https://"))
            && element_value.len() < 500 
        {
            return Some(element_value.to_string());
        }
    }
    
    None
}

/// Get site skill content for the current page based on URL in elements
pub fn get_site_skill_for_elements(elements: &[serde_json::Value], skills: &[Skill]) -> Option<String> {
    let url = extract_url_from_elements(elements)?;
    info!("Extracted URL from elements: {}", url);
    
    let skill = find_site_skill_for_url(&url, skills)?;
    info!("Found matching site skill: {}", skill.name);
    
    Some(format_site_skill_for_prompt(&skill))
}

/// Get app skill content based on active application name
pub fn get_app_skill_for_app_name(app_name: &str, skills: &[Skill]) -> Option<String> {
    let skill = find_app_skill_for_app_name(app_name, skills)?;
    info!("Found matching app skill: {}", skill.name);
    
    Some(format_app_skill_for_prompt(&skill))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.linkedin.com/in/someone"), Some("www.linkedin.com".to_string()));
        assert_eq!(extract_domain("github.com/user/repo"), Some("github.com".to_string()));
        assert_eq!(extract_domain("https://old.reddit.com/r/rust"), Some("old.reddit.com".to_string()));
    }

    #[test]
    fn test_domain_matches() {
        assert!(domain_matches("www.linkedin.com", "linkedin.com"));
        assert!(domain_matches("linkedin.com", "linkedin.com"));
        assert!(!domain_matches("fakelinkedin.com", "linkedin.com"));
    }

    #[test]
    fn test_app_name_matches() {
        let app_names = vec!["Code".to_string(), "Visual Studio Code".to_string()];
        assert!(app_name_matches("Code", &app_names));
        assert!(app_name_matches("Visual Studio Code", &app_names));
        assert!(!app_name_matches("Sublime Text", &app_names));
    }
}
