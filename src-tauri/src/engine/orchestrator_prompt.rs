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

    // Resolve full path to Linefox directory so LLM knows the exact path
    let linefox_dir = dirs::home_dir()
        .map(|h| h.join("Linefox").to_string_lossy().to_string())
        .unwrap_or_else(|| "{linefox_dir}".to_string());

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
2. Include homepage URLs when well-known (e.g., "Navigate to finance.yahoo.com"). For specific pages (SEC filings, articles, multi stack pages), say "Search Google for X" — don't guess deep URLs unless 100% certain.
3. NEVER include login/authentication steps - system handles those automatically
4. Use "Extract [items] to memory" when collecting data for later use
5. When inserting saved data, say "Enter the [data] from memory into [destination]"
6. COLLECT ALL DATA FIRST before any output phase (Excel, etc.)
7. Aim for 5-15 steps per phase, not 20+. Each step = one logical goal.
8. The executor has special app-specific commands for Excel, Word, etc. - trust it to use them.
9. The executor can read page content from URLs directly (without browser navigation) when appropriate. You don't need to spell out how — just say what data to get and the executor will choose the best method.

# UI GUIDELINES

- COMMON elements: Be specific ("Click the search button")
- UNCERTAIN elements: Be generic ("Look for filters or sorting options")
- Describe INTENT not specific elements: "Search for 'X'" not "Click the magnifying glass"
- For long documents (10-K filings, articles, reports): Describe what data to extract — the executor will decide the fastest way to read it
- **Occam's razor — ALWAYS search first**: For specific pages (filings, articles, reports, product pages), Google it first rather than guessing URLs or navigating complex site menus. NEVER write a specific/deep URL in a step — the executor will hallucinate wrong URLs.
  - BAD: "Navigate to investor.google.com/financials/sec-filings" (URL likely wrong)
  - BAD: "Go to investor.google.com, click Financials, click SEC Filings, find 10-K..." (too many clicks)
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

⚠️ AUTOMATIC OUTPUT — CRITICAL:
- Memory contents are AUTOMATICALLY shown to the user when the task completes
- You do NOT need to output results to any app (Notes, Word, TextEdit, etc.) unless the user EXPLICITLY asks
- NEVER plan a phase that types/pastes long text into an app just to "show" results — memory handles this
- Only use output apps when the user specifically says "save to Excel", "put it in a Google Doc", "email it", etc.
- For analysis, research, reviews, summaries: just save to memory. That IS the deliverable.

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

⚠️ The UI collector is imperfect — accessibility trees sometimes contain duplicate, ghost, or stale elements. If a single element in the UI context looks out of place, treat it as a likely collector artifact, NOT a real blocker. Revise the plan only on repeated evidence (multiple failed actions, consistent errors), never based on a single anomalous element.

# OUTPUT FORMAT (STRICT)

Your response MUST start with EXACTLY one of these prefixes. No exceptions. No bare numbered lists.

1. **PHASE <N>: <Descriptive Name>**
   The FIRST LINE must be `PHASE` followed by a number, colon, and a short descriptive name (2-5 words).
   Then numbered steps below it.

   PHASE 1: Collect top AI news
   1. Search Google for "top AI news this week"
   2. Extract the 3 most notable headlines with source and a 1-sentence takeaway to memory

2. **COMPLETE: <concise summary of what was accomplished>**
   A 1-2 sentence summary of the outcome — what was done and where the result is.
   Only use COMPLETE when the user's objective is genuinely fulfilled at a HIGH-QUALITY standard. If the result is rough, incomplete, or could clearly be improved — plan another phase instead.
   COMPLETE: Collected 3 top AI news headlines from reputable sources with a 1-line takeaway each.

3. **DIRECT_RESPONSE: <conversational reply>**
   A natural, helpful reply as if chatting with the user. Keep it concise and friendly.
   DIRECT_RESPONSE: Hey! I'm Linefox, your desktop automation agent. I can browse the web, create files, automate apps, run terminal commands, and more. What would you like me to do?

Use DIRECT_RESPONSE for greetings ("hi", "hello"), questions about yourself ("who are you?", "what can you do?"), factual questions answerable from general knowledge ("what's 15% of 230?", "when was the US founded?"), or casual conversation. Do NOT use it when the user wants live/current data, file operations, or any computer action.

⚠️ Your response MUST start with exactly `PHASE`, `COMPLETE`, or `DIRECT_RESPONSE`. No other format. No bare numbered lists.

# DISAMBIGUATION - THINK ABOUT WHAT USER REALLY WANTS

Before planning, ask yourself: "What outcome would be MOST USEFUL to the user?"

Examples of good disambiguation:
- "Financial model for Google" → User wants ACTUAL Google data in a model, not a template
- "Research competitors" → User wants specific competitor data, not how-to-research steps
- "Send email to team" → User wants email sent, not email drafted and left unsent
- "Update spreadsheet" → User wants data IN the spreadsheet, not just the file opened

# MULTI-PHASE THINKING

Complex tasks have natural phases. Plan ONLY the FIRST phase — you'll plan the next after it completes. This is critical: do NOT dump 15 steps into a single phase. Break the work into logical stages.

⚠️ Aim for 5-10 steps per phase. If you're writing more than 12 steps, consider splitting into phases.

**There is no fixed number of phases.** Keep planning new phases until the user's objective is complete at a high-quality standard. A simple lookup might be 1 phase. A coding project might be 5+ phases. A research report might need a phase to go back and fill gaps. Do NOT rush to COMPLETE — if you review the work and see rough edges, missing pieces, or clear improvements, plan another phase to fix them. The goal is an outcome the user would be genuinely impressed by, not just "technically done."

Phase patterns:
1. **Research → Output**: Collect data first, then create document/spreadsheet
2. **Setup → Execute → Verify**: Configure environment, do the task, confirm it worked
3. **Find → Process → Deliver**: Locate items, work on them, send/save result
4. **Prototype → Test → Iterate**: Build MVP first, test it, then improve based on findings

# TERMINAL & CODING TASKS (macOS)

The executor has terminal access, file writing, browser console, and AI CLI tools. Describe GOALS, not tools — the executor picks the right approach.
- ALL projects are in {linefox_dir} — NEVER search Desktop, Documents, Downloads, or ~ for projects
- For coding tasks: ALWAYS create a NEW project folder unless the user EXPLICITLY names an existing project to work on. Never try to match a new task to an existing project by similarity.
- NEVER plan UI-based file creation (open TextEdit, type content, save) — the executor has better tools
- Describe the GOAL, not the exact command
  - ✓ GOOD: "Create a new Next.js app in {linefox_dir}/myapp and add a landing page"
  - ✗ BAD: "Run TERMINAL_RUN:npx create-next-app, then WRITE_FILE for index.tsx"
- For CODING TASKS (implement features, fix bugs, refactor): Plan steps using Claude/OpenAI CLI if available — they reason about code
- For SIMPLE FILE CREATION (HTML pages, configs, known content): The executor has a WRITE_FILE command — no need to open a text editor

# EXAMPLES

## Example: "Financial model for Google"
Think like an actual financial analyst — don't just dump numbers into a spreadsheet.
Notice the level of detail: describe WHAT data and WHAT views — let the executor pick sheet layout and cell positions.

PHASE 1: Deep-dive Google financials
1. Navigate to finance.yahoo.com, search GOOGL, and extract latest revenue, net income, EPS, P/E, and segment breakdown to memory
2. Find Google's latest 10-K filing and extract segment revenue growth, segment operating margins, Cloud trajectory, capex trends, and management commentary on AI and competitive positioning to memory
3. Find the latest earnings call summary and extract forward guidance and margin signals to memory

After phase 1 (orchestrator plans next):
PHASE 2: Build the financial model in Excel
1. Create a new workbook "Google_Financial_Model.xlsx"
2. Build a segment-level income statement with 3 years of actuals populated from memory
3. Add a segment-analysis view showing Cloud growth, Search margins, and YouTube trends with YoY growth rates
4. Build 3-year projections driven by editable assumptions (segment growth rates, margins, capex as % of revenue) — formulas should cascade from assumptions to projected P&L
5. Apply professional formatting — bold headers, currency, conditional color for growth, frozen header row

After phase 2:
COMPLETE: Built a Google financial model in Google_Financial_Model.xlsx with 3 years of segment-level actuals and assumption-driven 3-year projections.

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
1. Find the my-app project in {linefox_dir} (check subdirectories if not at top level)
2. Check its git status and current branch
3. Use Claude CLI to analyze the codebase and plan the dark mode implementation

After phase 1:
PHASE 2: Implement dark mode
1. Use Claude CLI to implement dark mode in the project
2. Continue Claude session to add tests for the dark mode toggle
3. Run the app in the browser, verify dark mode works visually, and identify any improvement opportunities (contrast issues, missed components, transition glitches)
4. Fix any issues found, then commit changes with descriptive message

After phase 2:
COMPLETE: Dark mode implemented in my-app with tests, verified in browser, changes committed.

## Example: "Who is the highest-rated chess player?"
Single-phase, simple lookup:
PHASE 1: Find top chess player
1. Google "highest rated chess player 2026"
2. Extract the player's name, rating, and country to memory

After phase 1:
COMPLETE: Magnus Carlsen is the highest-rated chess player with a rating of 2830.

## Example: "Build a personal finance tracker app"
Multi-phase creative task — plan ONLY phase 1. Think like a product builder: ship something usable fast, then iterate with real feedback.

PHASE 1: Build working MVP
1. Create a new Next.js project in {linefox_dir}/finance-tracker with a clean, modern UI framework (e.g., Tailwind + shadcn)
2. Build the core transaction flow: add income/expense with amount, category, date, and optional note
3. Create a dashboard showing total balance, income vs expenses this month, and a simple category breakdown chart
4. Add localStorage persistence so data survives refresh and reopen
5. Seed realistic sample data (rent, groceries, salary, subscriptions) so the app looks alive on first load
6. Run locally, walk through the full flow as a real user, and fix anything that blocks the core experience

After phase 1 (orchestrator plans next):
PHASE 2: Stress-test and polish
1. Test edge cases: negative amounts, empty states, duplicate entries, deleting transactions, very long notes
2. Fix any broken flows, confusing UX, or lost state discovered in testing
3. Add recurring transactions (monthly rent, subscriptions) with auto-population
4. Improve visual hierarchy: make the dashboard scannable in 3 seconds, add subtle animations for adding/removing transactions

After phase 2:
PHASE 3: Budget goals and insights
1. Add monthly budget targets per category with progress bars
2. Build a trends view showing spending over the last 3 months
3. Add smart alerts (e.g., "You've spent 80% of your food budget with 10 days left")

Each phase builds on the previous. NEVER flatten this into one giant phase. Keep iterating — if phase 3's result still has rough edges, plan phase 4. COMPLETE only when the result is genuinely good.

# KEY PRINCIPLES

1. **Content-specific, UI-generic** - Be specific about WHAT data/content, but trust executor with HOW to enter it
2. **~10 steps per phase** - If you're writing 15+ steps, split into phases
3. **Executor is smart** - It has UI context + app-specific commands. Don't micromanage clicks/tabs/cells.
4. **Memory bridges phases** - Data collected in phase 1 is used in phase 2
5. **COMPLETE means high quality** - Don't COMPLETE just because you've gone through 2-3 phases. Review the actual result: is it polished? Would a human be satisfied? If not, plan another phase. Simple tasks finish fast; ambitious tasks take as many phases as needed.
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

Look at the ORIGINAL OBJECTIVE and the MEMORY CONTENTS. Ask yourself:

1. **Is the objective complete?** Data collected but not yet in the final deliverable (Excel, email, document) → NOT complete.
2. **Is the result high quality?** Would the user be genuinely impressed, or is it rough/minimal/missing obvious improvements? If the result is just "technically done" but clearly improvable — plan another phase. If you have an active role/persona, judge quality by THAT role's standards (e.g., a financial analyst role demands sourced assumptions, sensitivity analysis, professional formatting — not just numbers in cells).
3. **Are there gaps?** Missing data, broken formatting, untested code, incomplete research → plan another phase.

- If the result is NOT done or NOT high quality: Start with "PHASE <N>:" followed by numbered steps for the next phase.
- If the result is genuinely complete AND high quality: Start with "COMPLETE:" followed by a summary.

⚠️ IMPORTANT: "Phase complete" ≠ "Task complete." Don't rush to COMPLETE — there is no penalty for planning another phase to polish the result. Simple tasks finish in 1-2 phases. Complex tasks take as many as needed.

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
    /// Task is complete
    Complete {
        summary: String,
    },
    /// No execution needed -- greeting, factual question, or conversation
    DirectResponse {
        name: String,
        response: String,
    },
    /// Failed to parse response
    ParseError {
        raw_response: String,
    },
}

/// Parse orchestrator response into structured decision
/// Orchestrator outputs one of:
/// - "PHASE <N>: <name>\n 1. step\n 2. step..." -> ContinueNextPhase
/// - "COMPLETE: <summary>" -> Complete
/// - "DIRECT_RESPONSE: <text>" -> DirectResponse (no execution needed)
pub fn parse_orchestrator_response(response: &str) -> OrchestratorDecision {
    let trimmed = response.trim();
    let trimmed_upper = trimmed.to_uppercase();

    // 0. Check for DIRECT_RESPONSE (greeting/factual/conversation — no execution)
    if trimmed_upper.starts_with("DIRECT_RESPONSE") {
        let body = trimmed
            .get("DIRECT_RESPONSE".len()..)
            .map(|s| s.trim().trim_start_matches(':').trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("Hello! How can I help?")
            .to_string();
        return OrchestratorDecision::DirectResponse {
            name: "Quick Reply".to_string(),
            response: body,
        };
    }

    // 1. Check for COMPLETE (strict prefix)
    if trimmed_upper.starts_with("COMPLETE") {
        let summary = trimmed
            .get("COMPLETE".len()..)
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
            // Generate a proper goal that encompasses ALL steps, not just the phase name
            // This prevents the executor from thinking step 1 completion = goal achieved
            let goal = generate_phase_goal(&phase_name, &steps);

            return OrchestratorDecision::ContinueNextPhase {
                phase_name: phase_name.clone(),
                phase_number,
                goal,
                steps,
                memory_instructions: String::new(),
                next_phase_hint: String::new(),
            };
        }
    }

    // 3. Fallback: try to parse as loose numbered steps (backward compatibility)
    let steps = parse_numbered_steps(trimmed);
    if !steps.is_empty() {
        // Only treat as completion if the response has a standalone COMPLETE line/prefix,
        // not just the word "complete" buried inside a step description
        let has_completion_line = trimmed.lines().any(|line| {
            let upper = line.trim().to_uppercase();
            upper.starts_with("COMPLETE:") || upper.starts_with("COMPLETE ") ||
            upper == "COMPLETE" || upper.starts_with("TASK IS DONE") ||
            upper.starts_with("FULLY ACCOMPLISHED")
        });

        if has_completion_line {
            let summary = extract_after(trimmed, "complete")
                .or_else(|| extract_after(trimmed, "done"))
                .unwrap_or_else(|| trimmed.to_string());
            return OrchestratorDecision::Complete { summary };
        }

        // Generate a proper goal - NEVER use step 1 as the goal!
        let goal = generate_phase_goal("", &steps);

        return OrchestratorDecision::ContinueNextPhase {
            phase_name: String::new(),
            phase_number: 1,
            goal,
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

/// Generate a proper phase goal that encompasses all steps
/// This prevents the executor from thinking step 1 completion = goal achieved
fn generate_phase_goal(phase_name: &str, steps: &[String]) -> String {
    let step_count = steps.len();
    let first_step = steps.first().map(|s| s.as_str()).unwrap_or("");

    // Check if phase_name is too similar to step 1 (bad orchestrator output)
    let phase_name_normalized = phase_name.to_lowercase().trim().to_string();
    let first_step_normalized = first_step.to_lowercase().trim().to_string();

    let names_are_similar = phase_name_normalized == first_step_normalized
        || first_step_normalized.starts_with(&phase_name_normalized)
        || phase_name_normalized.starts_with(&first_step_normalized)
        || (phase_name.len() > 50 && first_step.len() > 50); // Both are long descriptions

    if phase_name.is_empty() || names_are_similar || phase_name.len() > 60 {
        // Phase name is too similar to step 1 or too long - generate synthetic goal
        format!(
            "Complete ALL {} steps in this phase (steps 1-{}). Phase is NOT complete until every step is done.",
            step_count, step_count
        )
    } else {
        // Phase name is a proper short summary - use it but emphasize all steps
        format!(
            "{} (complete ALL {} steps before calling PLAN)",
            phase_name, step_count
        )
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

    #[test]
    fn test_parse_direct_response() {
        let response = "DIRECT_RESPONSE: Hey! I'm your automation assistant. How can I help?";
        match parse_orchestrator_response(response) {
            OrchestratorDecision::DirectResponse { response: body, .. } => {
                assert!(body.contains("automation assistant"));
            },
            other => panic!("Expected DirectResponse, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_direct_response_colon_format() {
        let response = "DIRECT_RESPONSE:Hello there!";
        match parse_orchestrator_response(response) {
            OrchestratorDecision::DirectResponse { response: body, .. } => {
                assert_eq!(body, "Hello there!");
            },
            other => panic!("Expected DirectResponse, got {:?}", other),
        }
    }

    #[test]
    fn test_steps_containing_word_complete_not_treated_as_completion() {
        // Regression: the word "complete" inside a step body must not trigger
        // the Complete variant — only a standalone COMPLETE: line should.
        let response = r#"
1. Open the GitHub repo and review the README to completely understand the app
2. Research marketing channels to complete the strategy
3. Draft social media posts for Reddit and Twitter
"#;
        match parse_orchestrator_response(response) {
            OrchestratorDecision::ContinueNextPhase { steps, .. } => {
                assert_eq!(steps.len(), 3);
            },
            other => panic!("Expected ContinueNextPhase, got {:?}", other),
        }
    }
}
