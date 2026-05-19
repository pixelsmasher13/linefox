use log::{info, warn};
use chrono::Local;
use tauri::AppHandle;
use serde::{Deserialize, Serialize};

use crate::configuration::state::ServiceAccess;
use crate::repository::task_extracted_data_repository::{
    RecordSummary, get_memory_summary_for_planner, 
    get_records_by_ids, get_records_by_type_for_planner
};
use crate::entity::task_extracted_data::TaskExtractedData;

/// Tool call request from LLM for retrieving memory records
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryToolCall {
    pub tool: String,  // "GET_RECORDS"
    pub record_type: Option<String>,  // Filter by type
    pub record_ids: Option<Vec<i64>>,  // Specific record IDs
    pub limit: Option<i32>,  // Max records to return
}

#[derive(Debug)]
pub struct ScriptGenerator {
    api_key: String,
    api_choice: String,
}

impl ScriptGenerator {
    pub fn new(api_key: String, api_choice: String) -> Self {
        Self { api_key, api_choice }
    }
    
    /// Format memory summary for the planner prompt
    fn format_memory_summary(summaries: &[RecordSummary], types: &[String], total_count: i64) -> String {
        if summaries.is_empty() {
            return "No stored data available.".to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("📦 STORED DATA MEMORY ({} total records)\n", total_count));
        output.push_str(&format!("Available types: {}\n\n", types.join(", ")));
        
        if total_count > 100 {
            output.push_str("(Showing most recent 100 records - use GET_RECORDS tool to retrieve specific types)\n\n");
        }
        
        output.push_str("Recent records:\n");
        for (i, summary) in summaries.iter().enumerate() {
            output.push_str(&format!(
                "  {}. [ID:{}] {} ({}) - {}\n",
                i + 1,
                summary.id,
                summary.record_name,
                summary.record_type,
                summary.source_context.as_deref().unwrap_or("no context")
            ));
        }
        
        output
    }
    
    /// Format retrieved records as context for the planner
    fn format_retrieved_records(records: &[TaskExtractedData]) -> String {
        if records.is_empty() {
            return "No records retrieved.".to_string();
        }
        
        let mut output = String::new();
        output.push_str("📋 RETRIEVED RECORDS:\n\n");
        
        for record in records {
            output.push_str(&format!("### {} ({})\n", record.record_name, record.record_type));
            if let Some(ref ctx) = record.source_context {
                output.push_str(&format!("Source: {}\n", ctx));
            }
            output.push_str(&format!("Data: {}\n\n", 
                serde_json::to_string_pretty(&record.data).unwrap_or_else(|_| "{}".to_string())
            ));
        }
        
        output
    }
    
    /// Parse tool call from LLM response
    fn parse_tool_call(response: &str) -> Option<MemoryToolCall> {
        // Look for GET_RECORDS tool call pattern
        // Format: GET_RECORDS(type="contact", ids=[1,2,3], limit=10)
        // or JSON format: {"tool": "GET_RECORDS", "record_type": "contact", ...}
        
        let response_trimmed = response.trim();
        
        // Try JSON format first
        if response_trimmed.starts_with("{") && response_trimmed.contains("GET_RECORDS") {
            if let Ok(tool_call) = serde_json::from_str::<MemoryToolCall>(response_trimmed) {
                return Some(tool_call);
            }
            // Try to extract JSON from response
            if let Some(start) = response_trimmed.find("{") {
                if let Some(end) = response_trimmed.rfind("}") {
                    let json_str = &response_trimmed[start..=end];
                    if let Ok(tool_call) = serde_json::from_str::<MemoryToolCall>(json_str) {
                        return Some(tool_call);
                    }
                }
            }
        }
        
        // Try simple format: GET_RECORDS:type=contact or GET_RECORDS:ids=1,2,3
        if response_trimmed.starts_with("GET_RECORDS") {
            let mut tool_call = MemoryToolCall {
                tool: "GET_RECORDS".to_string(),
                record_type: None,
                record_ids: None,
                limit: Some(20),
            };
            
            // Parse parameters after GET_RECORDS
            if let Some(params_start) = response_trimmed.find(':') {
                let params_str = &response_trimmed[params_start + 1..];
                for param in params_str.split(',') {
                    let param = param.trim();
                    if param.starts_with("type=") {
                        tool_call.record_type = Some(param[5..].trim().trim_matches('"').to_string());
                    } else if param.starts_with("ids=") {
                        let ids_str = param[4..].trim().trim_matches(|c| c == '[' || c == ']');
                        let ids: Vec<i64> = ids_str
                            .split(',')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect();
                        if !ids.is_empty() {
                            tool_call.record_ids = Some(ids);
                        }
                    } else if param.starts_with("limit=") {
                        if let Ok(limit) = param[6..].trim().parse::<i32>() {
                            tool_call.limit = Some(limit);
                        }
                    }
                }
            }
            
            // Only return if we have some filter criteria
            if tool_call.record_type.is_some() || tool_call.record_ids.is_some() {
                return Some(tool_call);
            }
        }
        
        None
    }

    /// Generate only NL description from a user prompt (skip script generation)
    /// Returns (empty_script, nl_description, generated_name)
    pub async fn generate_script_from_prompt(
        &self,
        prompt: &str,
        name: &str,
        installed_apps: &[String],
    ) -> Result<(String, String, String), String> {
        info!("Generating NL description and name from prompt: {}", prompt);

        // We're skipping script generation, so just return empty script
        let empty_script = "{}";

        // Generate the NL description and name directly from the prompt (without memory)
        let (nl_description, generated_name) = self.generate_nl_description_and_name_from_prompt(prompt, name, installed_apps, None, None).await?;

        Ok((empty_script.to_string(), nl_description, generated_name))
    }
    
    /// Generate NL description with memory access (planner can retrieve stored data)
    /// Returns (empty_script, nl_description, generated_name)
    /// Supports up to MAX_TOOL_CALLS rounds of GET_RECORDS before generating the plan
    pub async fn generate_script_from_prompt_with_memory(
        &self,
        app_handle: &AppHandle,
        prompt: &str,
        name: &str,
        installed_apps: &[String],
    ) -> Result<(String, String, String), String> {
        const MAX_TOOL_CALLS: usize = 2; // Max GET_RECORDS calls before forcing plan generation
        
        info!("Generating NL description with memory access from prompt: {}", prompt);

        // Get memory summary from database
        let (summaries, types, total_count) = app_handle
            .db(|db| get_memory_summary_for_planner(db))
            .map_err(|e| format!("Failed to get memory summary: {}", e))?;
        
        let memory_summary = if !summaries.is_empty() {
            let formatted = Self::format_memory_summary(&summaries, &types, total_count);
            info!("Memory summary being sent to planner:\n{}", formatted);
            Some(formatted)
        } else {
            None
        };
        
        info!("Memory: {} records, {} types: {:?}", total_count, types.len(), types);

        // We're skipping script generation, so just return empty script
        let empty_script = "{}";
        
        // Track all retrieved records across multiple tool calls
        let mut all_retrieved_records: Vec<TaskExtractedData> = Vec::new();
        let mut tool_call_count = 0;

        // First LLM call - may return tool call or direct plan
        let (mut current_response, generated_name) = self.generate_nl_description_and_name_from_prompt(
            prompt, name, installed_apps, memory_summary.as_deref(), None
        ).await?;
        
        info!("Planner initial response (first 300 chars): {}", 
              current_response.chars().take(300).collect::<String>());
        
        // Tool call loop - allow up to MAX_TOOL_CALLS retrievals
        while tool_call_count < MAX_TOOL_CALLS {
            if let Some(tool_call) = Self::parse_tool_call(&current_response) {
                tool_call_count += 1;
                info!("Planner requested memory retrieval #{}: {:?}", tool_call_count, tool_call);
                
                // Execute the tool call
                let new_records = self.execute_memory_tool_call(app_handle, &tool_call)?;
                
                if !new_records.is_empty() {
                    info!("Retrieved {} records (total now: {})", 
                          new_records.len(), 
                          all_retrieved_records.len() + new_records.len());
                    
                    // Add to accumulated records (avoid duplicates by ID)
                    for record in new_records {
                        if !all_retrieved_records.iter().any(|r| r.id == record.id) {
                            all_retrieved_records.push(record);
                        }
                    }
                    
                    // Format all retrieved records so far
                    let retrieved_context = Self::format_retrieved_records(&all_retrieved_records);
                    
                    // Call LLM again with accumulated context
                    // Add note about remaining tool calls
                    let remaining_calls = MAX_TOOL_CALLS - tool_call_count;
                    let context_with_note = if remaining_calls > 0 {
                        format!("{}\n\n(You can make {} more GET_RECORDS call(s) if needed, or provide your plan now)", 
                                retrieved_context, remaining_calls)
                    } else {
                        format!("{}\n\n(No more GET_RECORDS calls available - please provide your plan now)", 
                                retrieved_context)
                    };
                    
                    let (response, _) = self.generate_nl_description_and_name_from_prompt(
                        prompt, name, installed_apps, memory_summary.as_deref(), Some(&context_with_note)
                    ).await?;
                    
                    current_response = response;
                } else {
                    // No records found, break and use current response
                    info!("No records found for tool call, proceeding with plan generation");
                    break;
                }
            } else {
                // No tool call detected, we have our final response
                break;
            }
        }
        
        // If we exhausted tool calls and still getting tool requests, force plan
        if tool_call_count >= MAX_TOOL_CALLS && Self::parse_tool_call(&current_response).is_some() {
            warn!("Max tool calls ({}) reached, forcing plan generation with accumulated data", MAX_TOOL_CALLS);
            
            if !all_retrieved_records.is_empty() {
                let retrieved_context = Self::format_retrieved_records(&all_retrieved_records);
                let final_context = format!("{}\n\n⚠️ MAX TOOL CALLS REACHED - You MUST provide your plan now using the data above.", 
                                           retrieved_context);
                
                let (response, _) = self.generate_nl_description_and_name_from_prompt(
                    prompt, name, installed_apps, memory_summary.as_deref(), Some(&final_context)
                ).await?;
                
                current_response = response;
            }
        }

        Ok((empty_script.to_string(), current_response, generated_name))
    }
    
    /// Execute a memory tool call and retrieve records
    fn execute_memory_tool_call(
        &self,
        app_handle: &AppHandle,
        tool_call: &MemoryToolCall,
    ) -> Result<Vec<TaskExtractedData>, String> {
        let limit = tool_call.limit.unwrap_or(20).min(50); // Cap at 50 records
        
        // If specific IDs requested, fetch those
        if let Some(ref ids) = tool_call.record_ids {
            let limited_ids: Vec<i64> = ids.iter().take(50).copied().collect();
            return app_handle
                .db(|db| get_records_by_ids(db, &limited_ids))
                .map_err(|e| format!("Failed to get records by IDs: {}", e));
        }
        
        // If type requested, fetch by type
        if let Some(ref record_type) = tool_call.record_type {
            return app_handle
                .db(|db| get_records_by_type_for_planner(db, record_type, limit))
                .map_err(|e| format!("Failed to get records by type: {}", e));
        }
        
        Ok(Vec::new())
    }

    /// Generate NL description and name directly from user prompt
    async fn generate_nl_description_and_name_from_prompt(
        &self,
        prompt: &str,
        _name: &str,  // Keep for compatibility but we'll generate our own
        installed_apps: &[String],
        memory_summary: Option<&str>,
        retrieved_records: Option<&str>,
    ) -> Result<(String, String), String> {
        // Build the apps list section for the system prompt
        let apps_section = if !installed_apps.is_empty() {
            let apps_list = installed_apps.join("\n- ");
            format!(
                "\nAVAILABLE APPLICATIONS ON THIS SYSTEM:\n- {}\n\nIMPORTANT: Only use applications from the above list. If the user asks for an app not in the list, suggest the closest alternative from the available apps.",
                apps_list
            )
        } else {
            // Fallback to common app names if no apps detected
            String::from("\nAPPLICATION NAMES (use exactly):\n- Google Chrome (not Chrome)\n- Microsoft Word (not Word)\n- Microsoft Excel (not Excel)\n- Gmail (when in browser)\n- Slack\n- Terminal\n- Finder (on Mac) / File Explorer (on Windows)")
        };

        // Get current date for context
        let current_date = Local::now().format("%A, %B %d, %Y").to_string();

        // Dynamic CLI tools from startup probe — substituted directly to avoid named/positional arg ordering issues
        let cli_tools_section = crate::engine::cli_probe::get_cached_tools_str()
            .unwrap_or_else(|| String::from("AVAILABLE CLI TOOLS (pre-approved): git, npm, pip, cargo, brew, node, python3"));

        // Resolve full path to Linefox directory so LLM knows the exact path
        let linefox_dir = dirs::home_dir()
            .map(|h| h.join("Linefox").to_string_lossy().to_string())
            .unwrap_or_else(|| "{linefox_dir}".to_string());

        // System prompt that balances specificity with conciseness
        let system_prompt = format!(r#"You are an expert agent script writer.
Your job is to create a workflow plan that an executing LLM will follow to achieve HIGH QUALITY COMPLETION OF USER's REQUEST. Create a clear numbered list of steps for the given task, and provide a concise name for the task.

OUTPUT FORMAT:
Option A — If the task requires computer actions (browsing, apps, terminal, files):
First line: NAME: <concise 2-5 word name for the task>
Then a blank line
Then the numbered steps

Option B — If the task is a GREETING, FACTUAL QUESTION, CALCULATION, or CONVERSATION answerable from general knowledge WITHOUT any computer action:
First line: DIRECT_RESPONSE
Second line: NAME: <concise 2-5 word name>
Then a blank line
Then write the actual response/answer to the user's request directly (no steps needed).
Examples of when to use DIRECT_RESPONSE:
- "Hi there" → greeting, just respond
- "What is the founding date of the US?" → factual, just answer
- "What's 15% of 230?" → calculation, just compute
- "Explain quantum computing" → knowledge, just explain
Do NOT use DIRECT_RESPONSE when the user wants CURRENT/LIVE data (stock prices, news, weather) or wants actions performed (send email, open app, etc.).

NAME RULES:
- 2-5 words maximum
- Be descriptive but concise
- Use action words (e.g., "Extract LinkedIn Contacts", "Collect News Headlines", "Research Product Prices")
- Avoid generic names like "Web Task" or "Browser Task"
- Don't include punctuation or special characters

PLAN RULES:
1. INTENT, NOT MECHANICS: Describe the goal of each step. The executor sees the live screen and chooses how. ✓ "Filter results to show only nonstop flights" / ✗ "Click the 'Stops' dropdown and select 'Nonstop only'". Never assume exact button positions, page layouts, or features that may not exist.
2. CHECKPOINTS, NOT KEYSTROKES: One meaningful action per step — a verifiable checkpoint, not a single click. Steps should say WHAT to accomplish, not micro-manage HOW.
3. APP CHOICE: Only direct the user to apps (Excel, Word, etc.) when EXPLICITLY asked or clearly sensible — collected memory data is auto-shown at completion, so don't save it unless instructed. Prefer popular apps from the available list (Chrome over Firefox, Gmail over alternatives, Microsoft Office over others). Use exact app names.
4. DON'T OVER-SCRIPT THE BROWSER: For stable sources (finance.yahoo.com, stockanalysis.com, macrotrends.net, wikipedia.org, github.com, npmjs.com), name the source — "Get latest COIN financials from finance.yahoo.com and stockanalysis.com" — instead of scripting clicks. The executor has a parallel HTTP fetcher (~3s for up to 8 URLs). For SEC filings, news, paywalled or JS-heavy pages, say "Google X" or "find X" and let the executor browser-search.
5. URLS ARE STARTING POINTS: Use known URLs when helpful but don't prescribe page layouts or whether to use browser vs direct fetch. For common patterns (search, filter, sort, export, navigate to settings), name the pattern not the element: ✓ "Use the search functionality" / ✗ "Click the magnifying glass icon in the header".
6. LONG DOCUMENTS: For 10-Ks, articles, reports — say "Request full text" instead of scrolling page by page.
7. SKIP LOGIN: Never include login/credential/sign-in steps — the system handles auth automatically via takeover. Start your plan after any login would occur.
8. CURRENT DATE: {}. Account for explicit or implicit time references in the user's request.
9. TERMINAL/CODE TASKS: Describe the goal, not exact commands. The executor resolves paths, flags, and tools at runtime. ✓ "Find the project and run its build" / ✗ "Run 'cd {linefox_dir}/myproject && npm run build'" (hardcoded paths break when the project moves).
10. AVOID THESE ANTI-PATTERNS:
    ✗ "Click the blue 'Apply Filters' button" or "the three-dot menu next to the avatar" (assumes layout)
    ✗ "Select 'Advanced Search' from the dropdown" (assumes feature exists)
    ✗ "Open Terminal and type..." (executor runs commands directly — never open Terminal.app)
    ✗ "Copy the jokes to clipboard" / "Paste the content" (use "Extract to memory" / "Type from memory")

EXCEL & WORD - DON'T OVER-SPECIFY:
- The executor has access to SPECIAL INPUT AND FORMATTING COMMANDS for Excel and Word
- DO say WHAT to include: "Add the collected data to Excel with headers" or "Format the document professionally"
- Example: ✓ "Enter the data into Excel" NOT ✗ "Put title in Cell C1" or "Change headers in row 1, data starting row 2, bold the headers, add borders"

TERMINAL COMMANDS (macOS) - DIRECT EXECUTION:
- The executor has DIRECT TERMINAL ACCESS via TERMINAL_RUN command - NO NEED to open Terminal.app!
- ⚠️ NEVER say "Open Terminal" or "Launch Terminal" — the executor runs commands DIRECTLY without any app!
- For terminal tasks: Describe the GOAL, not the exact command. The executor resolves paths, flags, and tools at runtime.
- ✓ GOOD: "Find the project's package.json and run the build from that directory"
- ✓ GOOD: "Clone the repo if it doesn't already exist locally, then check its current state"
- ✓ GOOD: "Install dependencies and start the dev server in background"
- ✗ BAD: "Run 'cd {linefox_dir}/myproject && npm run build'" (hardcodes path — what if the project is elsewhere?)
- ✗ BAD: "Open Terminal, navigate to {linefox_dir}/project" (opens Terminal.app UI — WRONG!)
- The executor handles working directories automatically (defaults to {linefox_dir})
- For long-running commands (servers, watchers): Say "Start the dev server in background"
- ⚠️ Commands must be single-line — never use heredocs (<<EOF) or multiline commands

FILE LOCATIONS:
- ALL code files are stored in {linefox_dir} by default - search there first, not ~
- ✓ GOOD: "List projects in {linefox_dir}" or "Find the project matching '<name>' in {linefox_dir}"

{}

⚠️ CLAUDE CLI SESSION & PERMISSION MANAGEMENT - CRITICAL:
Each 'claude "prompt"' starts a NEW session with NO memory of previous commands!

⚠️ IMPORTANT: Claude CLI commands must START with "claude" - no cd prefix!
The default working directory is {linefox_dir}. Include the project path in your prompt.

SESSION: Use --continue for follow-up commands to maintain context.
PERMISSIONS: File write permissions and bash access are granted automatically — do NOT add --permission-mode.

Without --continue, Claude forgets previous context.

When user mentions code review, debugging, or asks for AI help with code, consider using Claude CLI!

CODING TASKS:
- Projects are in {linefox_dir}. ALWAYS create a NEW project folder unless the user EXPLICITLY names an existing project to work or this task is continuing the previous task (i.e. you have a history of your previous actions as part of this task)
- The executor has a WRITE_FILE command for small one-off files (single HTML page, config, simple script)
- For projects/multi-file work: use Claude/OpenAI CLI (faster, reasons about code)
- NEVER plan UI-based file creation (open TextEdit, type content, save)

OCCAM'S RAZOR - SIMPLEST PATH FIRST:
- For public documents (academic articles, blogs, 10-Ks): Google it first!
  ✗ BAD: "Go to investor.apple.com, click Financials, click SEC Filings, find 10-K..."
  ✓ GOOD: "Google 'Apple 10-K 2024 SEC filing' and open the direct link"
- The simplest path that achieves the goal is the best path

SIMPLE LOOKUP RULE:
• Factual questions, rankings, prices, "best X in Y" → 2-3 steps MAX. Google it, extract the answer.
• Try Google first. Only navigate to specialized sites (FIDE, ESPN, stock exchanges) if the user asks or the info isn't in search results.
• If the answer appears on a Google results page or the first link, do NOT build a multi-site research workflow.
(See Example 4 below for exactly how short these plans should be.)

HIGH-QUALITY OUTPUT PRINCIPLE:
- Quality over quantity — a few great results beats many mediocre ones
- Default quantities (unless user specifies):
  • "top news articles" → 3-5 from reputable sources
  • "research on X" → 3-5 most authoritative sources
  • "find products" → top 3-5 best matches
  • "collect contacts" → 5-10 most relevant
  • "find listings" (Airbnb, hotels, jobs) → whatever the user specified; default to 5 if unspecified
- More is not better. Stop when the request is clearly satisfied.

AUTHENTICATION HANDLING:
- SKIP ALL login/credential steps - the system handles authentication automatically
- Start your steps AFTER any login would occur
- Never include: "Enter username", "Enter password", "Click Sign In", "Log in with credentials"
- Simply navigate to the site and proceed with the actual task

MEMORY USAGE (CRITICAL):
- ALWAYS use "Extract [specific items] to memory" when collecting data to use later (unless we're copying the full text of the file through copy commands)
- NEVER say "copy" or "paste" memory - the system handles this automatically
- When you need to insert saved data, say "Type the [items] from memory"
- Memory accumulates data across steps - each extract ADDS to existing memory

DATA COLLECTION STYLE:
- Describe WHAT KIND of data to collect, with a few examples — don't enumerate every field as a mandatory checklist
- The agent executing this plan is smart enough to identify relevant data on a page
- ✓ GOOD: "Extract key property details (pricing, size, amenities, ratings) to memory"
- ✗ BAD: "Extract property name, nightly rate, total price, cleaning fee, bedrooms, bathrooms, max guests, hot tub, ski-in/ski-out, parking, wifi, rating, review count to memory"
- The GOOD version gives clear direction while letting the agent grab what's visible
- The BAD version creates a hard checklist — if one item isn't on the page, the agent loops trying to find it
- For multi-item tasks (10 products, 20 listings): prioritize COMPLETING ALL ITEMS over extracting every field from each one

GOOD EXAMPLES:

Example 1 - User asks: "Find the best productivity tips from Reddit"
1. Navigate to reddit.com and search for "best productivity tips"
2. Open the top 2-3 highly-upvoted posts and extract key tips to memory

Example 2 - User asks: "Find me 10 Airbnb houses with a pool, Oct 25–27 in Maui" (request date: Jan 1, 2026)
1. Navigate to airbnb.com, search for Maui with check-in Oct 25 and check-out Oct 27 2026
2. Filter to include pool amenity
3. Open each of the first 10 listings and extract key details to memory: pricing (nightly rate, total), location, pool type, standout amenities, and guest rating
4. After reviewing 10 listings, verify memory contains data for all 10

Example 3 - User asks: "Review my project / implement a feature / fix a bug with Claude"
(Covers: code review, feature implementation, responsive design, bug fixes — any Claude CLI coding task)
1. Find the project in {linefox_dir} and check its git status and current branch
2. Use Claude CLI to implement the requested changes (include the project path in the prompt)
3. Use Claude CLI --continue to verify the changes are complete
4. Commit the changes with a descriptive message
5. Extract summary of changes to memory

Example 4 - User asks: "Find the highest-rated chess player in the world"
1. Google "highest rated chess player 2026" and read the search results page
2. Extract the player's name, rating, and country to memory
(The results page shows the answer directly — no need to navigate to FIDE.)

REPETITIVE TASKS:
- Be explicit about completion conditions
- "Repeat steps 4-7 for each search result until 10 items collected or no more results"
- "Process each LinkedIn profile with 'Director' or 'VP' in title"
- For multi-item collection tasks (5+ items): end the plan with a verification step like "After reviewing N items, verify memory contains data for all N". Skip this for simple lookups or single-item tasks.
{}
{}

Output the NAME line, then a blank line, then the numbered steps. No other text."#, current_date, apps_section, cli_tools_section, 
            // Add memory section if available
            if memory_summary.is_some() || retrieved_records.is_some() {
                let mut memory_section = String::new();
                memory_section.push_str("\n\n═══════════════════════════════════════════\n");
                memory_section.push_str("📚 STORED DATA MEMORY ACCESS\n");
                memory_section.push_str("═══════════════════════════════════════════\n\n");
                
                memory_section.push_str("You have access to previously collected data from past tasks. This data can be used in your plan.\n\n");
                
                memory_section.push_str("TOOL: GET_RECORDS\n");
                memory_section.push_str("Use this tool to retrieve stored records BEFORE generating your plan.\n\n");
                memory_section.push_str("Format: GET_RECORDS:type=<type>,limit=<number>\n");
                memory_section.push_str("   OR: GET_RECORDS:ids=<id1>,<id2>,<id3>\n\n");
                memory_section.push_str("Examples:\n");
                memory_section.push_str("  GET_RECORDS:type=contact,limit=10\n");
                memory_section.push_str("  GET_RECORDS:ids=5,12,18\n");
                memory_section.push_str("  GET_RECORDS:type=financial_report\n\n");
                
                memory_section.push_str("WHEN TO USE GET_RECORDS:\n");
                memory_section.push_str("- If the task involves data you've collected before (contacts, products, research)\n");
                memory_section.push_str("- If the user references previous work (e.g., 'use those contacts', 'that company info')\n");
                memory_section.push_str("- If having existing data would make the task faster/better\n\n");
                
                memory_section.push_str("HOW TO USE RETRIEVED DATA IN YOUR PLAN:\n");
                memory_section.push_str("- Reference the data directly: 'Using the contact emails from stored memory...'\n");
                memory_section.push_str("- The executor can access this data during execution\n\n");
                
                if let Some(summary) = memory_summary {
                    memory_section.push_str("═══════════════════════════════════════════\n");
                    memory_section.push_str(&summary);
                    memory_section.push_str("\n═══════════════════════════════════════════\n");
                }
                
                if retrieved_records.is_some() {
                    memory_section.push_str("\n⚠️ RETRIEVED DATA IS PROVIDED BELOW - Use it in your plan!\n");
                }
                
                memory_section
            } else {
                String::new()
            }
        );

        // Build user prompt with optional retrieved records
        let user_prompt = if let Some(records) = retrieved_records {
            format!(
                "Task: {}\n\n{}\n\nUsing the retrieved data above, create workflow steps for this objective. Reference the stored data where appropriate.",
                prompt, records
            )
        } else {
            format!(
                "Task: {}\n\nCreate workflow steps for this objective. Be specific about applications and actions.{}",
                prompt,
                if memory_summary.is_some() {
                    "\n\nNote: If this task could benefit from stored data (see STORED DATA MEMORY above), respond ONLY with a GET_RECORDS tool call first. Otherwise, provide the NAME and numbered steps."
                } else {
                    ""
                }
            )
        };

        // Call the appropriate LLM API
        let response_text = match self.api_choice.as_str() {
            "gemini" => {
                info!("🔑 Using Gemini API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::gemini::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "proxy" => {
                info!("🔑 Using Heelix Cloud proxy for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::proxy::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "openai" => {
                info!("🔑 Using OpenAI API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::openai::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "openai-codex" => {
                info!("🔑 Using ChatGPT subscription (Codex) for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::openai_codex::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "grok" => {
                info!("🔑 Using Grok API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::grok::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "claude" | _ => {
                info!("🔑 Using Claude API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::claude::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            }
        };

        // Parse the response to extract name and steps (or direct response)
        let response_trimmed = response_text.trim();
        let lines: Vec<&str> = response_trimmed.lines().collect();

        // Check for DIRECT_RESPONSE format (no execution needed)
        let is_direct_response = !lines.is_empty() && lines[0].trim().eq_ignore_ascii_case("DIRECT_RESPONSE");

        if is_direct_response {
            // Format: DIRECT_RESPONSE\nNAME: <name>\n\n<response body>
            let generated_name = lines.iter()
                .find(|l| l.starts_with("NAME:"))
                .map(|l| l.strip_prefix("NAME:").unwrap_or("").trim().to_string())
                .unwrap_or_else(|| prompt.split_whitespace().take(4).collect::<Vec<_>>().join(" "));

            let response_body = lines.iter()
                .skip_while(|l| l.trim().eq_ignore_ascii_case("DIRECT_RESPONSE") || l.starts_with("NAME:") || l.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();

            info!("Direct response detected - name: {}, body length: {}", generated_name, response_body.len());

            // Prefix with DIRECT_RESPONSE: so the caller can detect it
            let prefixed = format!("DIRECT_RESPONSE:{}", response_body);
            return Ok((prefixed, generated_name));
        }

        // Normal plan format: NAME: <name>\n\n<steps>
        let generated_name = if !lines.is_empty() && lines[0].starts_with("NAME:") {
            lines[0].strip_prefix("NAME:").unwrap_or("").trim().to_string()
        } else {
            prompt.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
        };

        let nl_description = if lines.len() > 2 {
            lines.iter()
                .skip_while(|line| line.starts_with("NAME:") || line.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        } else {
            response_trimmed.to_string()
        };

        info!("Generated name: {}", generated_name);
        info!("Generated {} steps of description", nl_description.lines().count());

        Ok((nl_description, generated_name))
    }
    
    /// Generate a continuation plan when user wants to follow up on a previous task
    /// Returns (plan_description, continuation_name)
    pub async fn generate_continuation_plan(
        &self,
        previous_objective: &str,
        previous_completion_message: &str,
        recent_steps: &[String],
        new_request: &str,
    ) -> Result<(String, String), String> {
        info!("Generating continuation plan for: {}", new_request);
        
        let user_prompt = crate::engine::continuation_planner_prompt::get_continuation_planner_prompt(
            previous_objective,
            previous_completion_message,
            recent_steps,
            new_request,
        );
        
        let system_prompt = "You are a task continuation planner. Create clear, actionable step-by-step plans.";
        
        let response_text = match self.api_choice.to_lowercase().as_str() {
            "openai" => {
                info!("🔑 Using OpenAI API for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::openai::call_llm_api(
                        &self.api_key, user_prompt.clone(), system_prompt, 1500
                    ).await?;
                response_text
            }
            "openai-codex" => {
                info!("🔑 Using ChatGPT subscription (Codex) for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::openai_codex::call_llm_api(
                        &self.api_key, user_prompt.clone(), system_prompt, 1500
                    ).await?;
                response_text
            }
            "grok" => {
                info!("🔑 Using Grok API for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::grok::call_llm_api(
                        &self.api_key, user_prompt.clone(), system_prompt, 1500
                    ).await?;
                response_text
            }
            "proxy" => {
                info!("🔑 Using Heelix Cloud proxy for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::proxy::call_llm_api(
                        &self.api_key, user_prompt.clone(), system_prompt, 1500
                    ).await?;
                response_text
            }
            "claude" | _ => {
                info!("🔑 Using Claude API for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::claude::call_llm_api(
                        &self.api_key, user_prompt, system_prompt, 1500
                    ).await?;
                response_text
            }
        };

        // Parse the response
        let response_trimmed = response_text.trim();
        let lines: Vec<&str> = response_trimmed.lines().collect();

        // Extract the name from the first line
        let continuation_name = if !lines.is_empty() && lines[0].to_uppercase().starts_with("NAME:") {
            lines[0].split(':').skip(1).collect::<Vec<_>>().join(":").trim().to_string()
        } else {
            format!("Continue: {}", new_request.split_whitespace().take(4).collect::<Vec<_>>().join(" "))
        };

        // Extract the plan (everything after the name line)
        let plan_description = lines.iter()
            .skip_while(|line| line.to_uppercase().starts_with("NAME:") || line.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        info!("Generated continuation name: {}", continuation_name);
        info!("Generated continuation plan with {} lines", plan_description.lines().count());

        Ok((plan_description, continuation_name))
    }
}