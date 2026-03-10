use chrono::Local;

/// Returns the orchestrator system prompt for agent mode planning
/// The orchestrator creates numbered steps for the executor to follow
pub fn get_orchestrator_system_prompt() -> String {
    get_orchestrator_system_prompt_with_role(None)
}

/// Returns the orchestrator system prompt with an optional active role injected
pub fn get_orchestrator_system_prompt_with_role(active_role: Option<&str>) -> String {
    let current_date = Local::now().format("%A, %B %d, %Y").to_string();

    let os_name = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };

    let role_section = match active_role {
        Some(role) if !role.is_empty() => format!(
            "\n────────────────────────────\n# YOUR ACTIVE ROLE\n────────────────────────────\n{}\n",
            role
        ),
        _ => String::new(),
    };

    format!(r#"You are an expert automation planner. Create numbered steps for a UI automation agent to follow.{role_section}

Current Date: {}
OS: {}

# YOUR JOB

Output a numbered list of steps (1. 2. 3. etc.) that accomplish the user's objective.
That's it - just the numbered steps. The executor will follow them.

# STEP WRITING RULES

1. **HIGH-LEVEL STEPS** - The executor is somewhat smart. Don't micro-manage. Say WHAT to do, not HOW.
   - BAD: "Type 'Quarter' in A1, press Tab, type 'Revenue' in B1, press Tab..."
   - GOOD: "Enter the collected data into Excel with columns: Quarter, Revenue, Profit"
2. Include exact URLs when known (e.g., "Navigate to finance.yahoo.com")
3. NEVER include login/authentication steps - system handles those automatically
4. Use "Extract [items] to memory" when collecting data for later use
5. When inserting saved data, say "Enter the [data] from memory into [destination]"
6. COLLECT ALL DATA FIRST before any output phase (Excel, etc.)
7. Aim for 5-15 steps per phase, not 20+. Each step = one logical goal.
8. The executor has special app-specific commands for Excel, Word, etc. - trust it to use them.

# UI GUIDELINES

- COMMON elements: Be specific ("Click the search button")
- UNCERTAIN elements: Be generic ("Look for filters or sorting options")
- Describe INTENT not specific elements: "Search for 'X'" not "Click the magnifying glass"
- For long documents (10-K filings, articles, reports): Say "Request full text to read the document" - this is faster than scrolling page by page
- **Occam's razor**: For public documents (10-K, earnings reports, SEC filings), Google it first to find a direct link rather than navigating complex site menus
  - BAD: "Go to investor.google.com, click Financials, click SEC Filings, find 10-K..."
  - GOOD: "Google 'Google 10-K 2024 SEC filing' and open the direct link"

# SIMPLE LOOKUP RULE

Apply Occam's Razor aggressively for these task types:
- **Factual questions** ("who is X", "what is the highest rated Y", "when did Z happen"):
  → Single short phase, 2-3 steps MAX. Google it, read the first result, extract the answer.
  ✗ BAD (6 steps): Navigate to fide.com → find ratings page → click top 100 → extract...
  ✓ GOOD (2 steps): 1. Google "highest rated chess player 2026"  2. Extract the player's name and rating to memory
- **"Best X in Y" queries** ("best hotel in Boston", "best restaurant in NYC"):
  → Single short phase, 3-4 steps MAX. Google it, read the snippets, extract top 2-3 options.
  ✓ GOOD (2 steps): 1. Google "best hotels in Boston"  2. Extract top 3 with ratings and key details to memory
- Try Google first. Only navigate to specialized sites (FIDE, ESPN, stock exchanges) if the user asks or the info isn't in search results.
- If the answer appears on a Google results page or the first link, do NOT build a multi-phase research workflow.

# MEMORY & DATA COLLECTION

- "Extract [items] to memory" = save data for later
- "Type [items] from memory" = use saved data
- Memory persists across phases
- Define what to collect upfront (schema)

**HIGH-QUALITY OUTPUT PRINCIPLE**
- Quality over quantity — a few great results beats many mediocre ones
- Default quantities (unless user specifies):
  • "top news articles" → 3-5 from reputable sources
  • "research on X" → 3-5 most authoritative sources
  • "find products" → top 3-5 best matches
  • "collect contacts" → 5-10 most relevant
  • "find listings" (Airbnb, hotels, jobs) → whatever the user specified; default to 5
- More is not better. Stop when the request is clearly satisfied.

**DATA COLLECTION STYLE**
- Describe WHAT KIND of data to collect, with a few examples — don't enumerate every field as a mandatory checklist
- The executor is smart enough to identify relevant data on a page
- ✓ GOOD: "Extract key property details (pricing, size, amenities, ratings) to memory"
- ✗ BAD: "Extract property name, nightly rate, total price, cleaning fee, bedrooms, bathrooms, max guests, hot tub, ski-in/ski-out, parking, wifi, rating, review count to memory"
- For multi-item tasks (10 products, 20 listings): prioritize COMPLETING ALL ITEMS over extracting every field from each one

# PERIODIC PROGRESS REVIEWS

You will periodically be asked to review execution progress (every ~30 steps).
For these automatic checkpoints (reason will say "Automatic checkpoint at step N"):
- If execution is making steady progress → Respond with the SAME phase, keeping current steps (let executor continue)
- If execution is going in circles (repeating actions, no new data collected, stuck on same page) → Give a REVISED phase with a completely different approach that avoids what was already tried
- If the objective is fully complete → COMPLETE
- If the objective is clearly impossible after extensive effort → COMPLETE with explanation of what was achieved and what couldn't be done

⚠️ CRITICAL: When you see repeated failed attempts (e.g., 10+ tries to click the same button, multiple attempts to open an app), DO NOT let execution continue with the same approach. Issue a REVISED phase that works around the blocker.

# OUTPUT FORMAT (STRICT)

Your output MUST start with one of these two prefixes:

1. **PHASE <N>:** followed by numbered steps — when there's more work to do:
   PHASE 1: Research Google financials
   1. Navigate to finance.yahoo.com, search GOOGL
   2. Extract revenue, net income, EPS to memory
   3. Search for latest 10-K filing

2. **COMPLETE:** followed by a summary — when the task is done:
   COMPLETE: All stock prices collected and saved to Excel successfully.

⚠️ CRITICAL: Your response MUST start with either "PHASE" or "COMPLETE". No other format is accepted.

# DISAMBIGUATION - THINK ABOUT WHAT USER REALLY WANTS

Before planning, ask yourself: "What outcome would be MOST USEFUL to the user?"

Examples of good disambiguation:
- "Financial model for Google" → User wants ACTUAL Google data in a model, not a template
- "Research competitors" → User wants specific competitor data, not how-to-research steps
- "Send email to team" → User wants email sent, not email drafted and left unsent
- "Update spreadsheet" → User wants data IN the spreadsheet, not just the file opened

# MULTI-PHASE THINKING

Complex tasks have natural phases. Plan the FIRST phase only - you'll plan the next after it completes.

Phase patterns:
1. **Research → Output**: Collect data first, then create document/spreadsheet
2. **Setup → Execute → Verify**: Configure environment, do the task, confirm it worked
3. **Find → Process → Deliver**: Locate items, work on them, send/save result

# TERMINAL & CODING TASKS (macOS)

The executor has DIRECT TERMINAL ACCESS via TERMINAL_RUN command — no need to open Terminal.app.

## Local Repo Awareness
- ALL code/projects are stored in ~/Linefox by default
- ALWAYS check ~/Linefox FIRST before cloning from GitHub: "List projects in ~/Linefox"
- If repo exists locally, check its state before changes: "Check git status and branch in ~/Linefox/<project>"
- NEVER clone a repo that already exists in ~/Linefox
- When user says "this repo" or "my project" without a name, list ~/Linefox first to identify it

## Terminal Commands
- For git, npm, pip, cargo, shell commands: Say "Run 'git clone <url>'" or "Run 'npm install'"
- For AI coding (Claude CLI): Use for coding tasks ONLY (not research/analysis). Use --continue for follow-ups (permissions are granted automatically)
- Commands timeout after 5 minutes — avoid slow commands like `find ~ -name '*'`

## Coding Task Phase Patterns
1. **Understand → Implement → Verify**: Check local repo state, make changes, run tests/verify
2. **Clone → Implement → Commit**: Clone repo (only if not local), implement feature, commit changes
3. **Research → Code → Test**: Research approach online, write code via Claude CLI, run tests

# EXAMPLES

## Example: "Financial model for Google"
First orchestrator response:
PHASE 1: Research Google financials
1. Navigate to finance.yahoo.com, search GOOGL, extract: Revenue, Net Income, EPS, P/E, segments (Search, Cloud, YouTube) to memory
2. Search for Google's latest 10-K or quarterly earnings report, request full text to read the document
3. Extract from report: revenue growth %, operating margin, segment breakdown, forward guidance to memory

After phase 1 completes, orchestrator returns:
PHASE 2: Build Excel model
1. Open Excel - create "Google_Financial_Model.xlsx"
2. Sheet 1 "Income Statement": Build P&L with rows for Revenue by Segment, COGS, Gross Profit, OpEx, Operating Income, Net Income
3. Sheet 2 "Assumptions": Create assumptions section with Revenue Growth % by segment, Margin assumptions, Tax rate
4. Link formulas so changing assumptions flows through to projections
5. Add 3-year projection columns using the growth assumptions
6. Format professionally: headers bold, currency formatting, % for rates

After phase 2 completes:
COMPLETE: Financial model created in Google_Financial_Model.xlsx with income statement, assumptions, and 3-year projections.

## Example: "Find 5 restaurants in Austin and email them to John"
PHASE 1: Research Austin restaurants
1. Search Google for "best restaurants Austin 2024"
2. Visit Yelp or similar and extract 5 restaurants to memory (name, cuisine, rating, address)

After phase 1:
PHASE 2: Email recommendations to John
1. Open Gmail and compose new email to John
2. Write email body with the restaurant recommendations from memory
3. Send the email

After phase 2:
COMPLETE: Emailed 5 Austin restaurant recommendations to John.

## Example: "Add dark mode to my-app"
PHASE 1: Understand local repo
1. List projects in ~/Linefox to find my-app
2. Check git status and current branch in ~/Linefox/my-app
3. Run Claude CLI to analyze the codebase and plan dark mode implementation

After phase 1:
PHASE 2: Implement dark mode
1. Run Claude CLI to implement dark mode in ~/Linefox/my-app
2. Continue Claude session to add tests for the dark mode toggle
3. Run the test suite to verify changes work
4. Commit changes with descriptive message

After phase 2:
COMPLETE: Dark mode implemented in my-app with tests, changes committed.

## Example: "Who is the highest-rated chess player?"
Single-phase, simple lookup:
PHASE 1: Find top chess player
1. Google "highest rated chess player 2026"
2. Extract the player's name, rating, and country to memory

After phase 1:
COMPLETE: Magnus Carlsen is the highest-rated chess player with a rating of 2830.

## Example: "Best hotel in Boston"
Single-phase, simple lookup:
PHASE 1: Find top hotels in Boston
1. Google "best hotels in Boston 2026"
2. Extract top 3 hotels with ratings, price range, and location to memory

After phase 1:
COMPLETE: Found 3 top-rated Boston hotels with ratings and details.

## Example: "Get today's CNN headlines"
Single-phase task:
PHASE 1: Extract CNN headlines
1. Navigate to cnn.com
2. Extract top 5 headlines with summaries to memory

After phase 1:
COMPLETE: Collected 5 CNN headlines with summaries.

# KEY PRINCIPLES

1. **Content-specific, UI-generic** - Be specific about WHAT data/content, but trust executor with HOW to enter it
2. **5-10 steps max per phase** - If you're writing 15+ steps, you're too detailed
3. **Executor is smart** - It has UI context + app-specific commands. Don't micromanage clicks/tabs/cells.
4. **Memory bridges phases** - Data collected in phase 1 is used in phase 2
"#, current_date, os_name)
}

/// Returns the prompt for reviewing executor progress
pub fn get_review_prompt(
    original_objective: &str,
    current_phase: &str,
    phase_number: u32,
    executor_progress: &str,
    executor_reason: &str,
    memory_contents: &str,
    action_summary: &str,
    current_ui_context: Option<&str>,
) -> String {
    let ui_section = match current_ui_context {
        Some(ui) if !ui.is_empty() => format!("\n## Current UI Context\n{}\n", ui),
        _ => String::new(),
    };

    format!(r#"# REVIEW EXECUTOR PROGRESS

## Original Objective
{}

## Phase {} Progress
{}

## Executor's Report
{}

## Reason for Calling PLAN
{}

## Memory Contents
{}

## Recent Actions
{}
{}
---

## YOUR DECISION

Look at the ORIGINAL OBJECTIVE above. Is it FULLY accomplished?

- If NO (more work needed): Start with "PHASE <N>:" followed by numbered steps for the next phase.
- If YES (original objective is 100% complete): Start with "COMPLETE:" followed by a summary.

⚠️ IMPORTANT: "Phase complete" ≠ "Task complete"
- i.e if data was collected but not yet put into Excel/email/document → NOT COMPLETE
- COMPLETE means the user's ORIGINAL REQUEST is fully satisfied.

⚠️ Your response MUST start with either "PHASE" or "COMPLETE". No other format is accepted.
"#,
        original_objective,
        phase_number,
        current_phase,
        executor_progress,
        executor_reason,
        if memory_contents.is_empty() { "<empty>" } else { memory_contents },
        action_summary,
        ui_section
    )
}

/// Returns the prompt for initial task planning
pub fn get_initial_planning_prompt(
    objective: &str,
    additional_instructions: Option<&str>,
    installed_apps: &[String],
    current_ui_context: Option<&str>,
) -> String {
    let apps_section = if !installed_apps.is_empty() {
        let apps_list = installed_apps.join(", ");
        format!("\n## Available Applications\n{}\n", apps_list)
    } else {
        String::new()
    };

    let instructions_section = match additional_instructions {
        Some(instructions) if !instructions.is_empty() => {
            format!("\n## Additional Instructions\n{}\n", instructions)
        },
        _ => String::new()
    };

    let ui_section = match current_ui_context {
        Some(ui) if !ui.is_empty() => format!("\n## Current UI Context\n{}\n", ui),
        _ => String::new()
    };

    format!(r#"# TASK

{}
{}{}{}
---

Create numbered steps to accomplish this task. Output ONLY the numbered steps, nothing else.
"#, objective, instructions_section, apps_section, ui_section)
}

/// Parse the orchestrator's decision from its response
#[derive(Debug, Clone, PartialEq)]
pub enum OrchestratorDecision {
    /// More steps to execute
    ContinueNextPhase {
        phase_name: String,
        phase_number: u32,
        goal: String,
        steps: Vec<String>,
        memory_instructions: String,
        next_phase_hint: String,
    },
    /// Keep for backward compatibility but rarely used
    RetryCurrentPhase {
        phase_name: String,
        phase_number: u32,
        goal: String,
        steps: Vec<String>,
        memory_instructions: String,
        reason: String,
    },
    /// Keep for backward compatibility
    RequestUserInput {
        question: String,
    },
    /// Task is complete
    Complete {
        summary: String,
    },
    /// Failed to parse response
    ParseError {
        raw_response: String,
    },
}

/// Parse orchestrator response into structured decision
/// Orchestrator outputs either:
/// - "PHASE <N>: <name>\n 1. step\n 2. step..." -> ContinueNextPhase
/// - "COMPLETE: <summary>" -> Complete
pub fn parse_orchestrator_response(response: &str) -> OrchestratorDecision {
    let trimmed = response.trim();
    let trimmed_upper = trimmed.to_uppercase();

    // 1. Check for COMPLETE (strict prefix, also handles "DECISION: COMPLETE")
    let complete_offset = if trimmed_upper.starts_with("COMPLETE") {
        Some("COMPLETE".len())
    } else if trimmed_upper.starts_with("DECISION: COMPLETE") {
        Some("DECISION: COMPLETE".len())
    } else {
        None
    };
    if let Some(offset) = complete_offset {
        let summary = trimmed
            .get(offset..)
            .map(|s| s.trim().trim_start_matches(':').trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(trimmed)
            .to_string();
        return OrchestratorDecision::Complete { summary };
    }

    // 2. Check for PHASE <N>: <name> followed by numbered steps
    if trimmed_upper.starts_with("PHASE") {
        // Parse "PHASE <N>: <phase_name>" from first line
        let first_line = trimmed.lines().next().unwrap_or("");
        let (phase_number, phase_name) = parse_phase_header(first_line);

        // Parse numbered steps from remaining lines
        let remaining = trimmed.lines().skip(1).collect::<Vec<_>>().join("\n");
        let steps = parse_numbered_steps(&remaining);

        if !steps.is_empty() {
            return OrchestratorDecision::ContinueNextPhase {
                phase_name: phase_name.clone(),
                phase_number,
                goal: phase_name,
                steps,
                memory_instructions: String::new(),
                next_phase_hint: String::new(),
            };
        }
    }

    // 3. Fallback: try to parse as loose numbered steps (backward compatibility)
    let steps = parse_numbered_steps(trimmed);
    if !steps.is_empty() {
        // Check if response also mentions completion (LLM didn't follow strict format)
        if trimmed_upper.contains("COMPLETE") || trimmed_upper.contains("TASK IS DONE") ||
           trimmed_upper.contains("FINISHED") || trimmed_upper.contains("FULLY ACCOMPLISHED") {
            let summary = extract_after(trimmed, "complete")
                .or_else(|| extract_after(trimmed, "done"))
                .unwrap_or_else(|| trimmed.to_string());
            return OrchestratorDecision::Complete { summary };
        }

        return OrchestratorDecision::ContinueNextPhase {
            phase_name: "Phase".to_string(),
            phase_number: 1,
            goal: steps.first().cloned().unwrap_or_else(|| "Execute steps".to_string()),
            steps,
            memory_instructions: String::new(),
            next_phase_hint: String::new(),
        };
    }

    // 4. Couldn't parse
    OrchestratorDecision::ParseError {
        raw_response: response.to_string()
    }
}

/// Parse "PHASE <N>: <name>" header line
fn parse_phase_header(line: &str) -> (u32, String) {
    let after_phase = line.trim()
        .strip_prefix("PHASE").or_else(|| line.trim().strip_prefix("Phase")).or_else(|| line.trim().strip_prefix("phase"))
        .unwrap_or(line)
        .trim();

    // Try to extract number and name: "<N>: <name>" or "<N> - <name>"
    let mut chars = after_phase.chars().peekable();
    let mut num_str = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            num_str.push(c);
            chars.next();
        } else {
            break;
        }
    }

    let phase_number = num_str.parse::<u32>().unwrap_or(1);

    // Skip separator (: or - or whitespace)
    let remaining: String = chars.collect();
    let phase_name = remaining.trim().trim_start_matches(':').trim_start_matches('-').trim().to_string();

    let phase_name = if phase_name.is_empty() {
        format!("Phase {}", phase_number)
    } else {
        phase_name
    };

    (phase_number, phase_name)
}

/// Extract text after a keyword
fn extract_after(text: &str, keyword: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if let Some(idx) = lower.find(keyword) {
        let after = &text[idx + keyword.len()..];
        let trimmed = after.trim().trim_start_matches(':').trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Parse numbered steps from response (1. 2. 3. etc.)
fn parse_numbered_steps(text: &str) -> Vec<String> {
    let mut steps = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(step_text) = parse_numbered_line(trimmed) {
            steps.push(step_text);
        }
    }

    steps
}

/// Parse a single numbered line like "1. Do something"
fn parse_numbered_line(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Skip empty lines
    if trimmed.is_empty() {
        return None;
    }

    // Match pattern: digits followed by . or )
    let mut chars = trimmed.chars().peekable();
    let mut num = String::new();

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            num.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    if num.is_empty() {
        return None;
    }

    // Check for . or ) after the number
    match chars.next() {
        Some('.') | Some(')') => {
            let text: String = chars.collect();
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
        _ => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numbered_line() {
        assert_eq!(parse_numbered_line("1. First step"), Some("First step".to_string()));
        assert_eq!(parse_numbered_line("2. Second step"), Some("Second step".to_string()));
        assert_eq!(parse_numbered_line("10. Tenth step"), Some("Tenth step".to_string()));
        assert_eq!(parse_numbered_line("Not a step"), None);
    }

    #[test]
    fn test_parse_numbered_steps() {
        let response = r#"
1. Launch Chrome
2. Navigate to google.com
3. Search for something
"#;
        let steps = parse_numbered_steps(response);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "Launch Chrome");
        assert_eq!(steps[1], "Navigate to google.com");
        assert_eq!(steps[2], "Search for something");
    }

    #[test]
    fn test_parse_complete_decision() {
        let response = "DECISION: COMPLETE\n\nTask complete: All stock prices collected and saved to Excel.";
        match parse_orchestrator_response(response) {
            OrchestratorDecision::Complete { summary } => {
                assert!(summary.contains("stock prices"));
            },
            _ => panic!("Expected Complete decision"),
        }
    }

    #[test]
    fn test_parse_steps_response() {
        let response = r#"
1. Open Chrome
2. Go to finance.yahoo.com
3. Search for AAPL
"#;
        match parse_orchestrator_response(response) {
            OrchestratorDecision::ContinueNextPhase { steps, .. } => {
                assert_eq!(steps.len(), 3);
            },
            _ => panic!("Expected ContinueNextPhase"),
        }
    }
}
