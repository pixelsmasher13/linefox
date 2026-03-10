use log::info;
use chrono::Local;

#[derive(Debug)]
pub struct ScriptGenerator {
    api_key: String,
    api_choice: String,
}

impl ScriptGenerator {
    pub fn new(api_key: String, api_choice: String) -> Self {
        Self { api_key, api_choice }
    }
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

        // Generate the NL description and name directly from the prompt
        let (nl_description, generated_name) = self.generate_nl_description_and_name_from_prompt(prompt, name, installed_apps).await?;

        Ok((empty_script.to_string(), nl_description, generated_name))
    }

    /// Generate NL description and name directly from user prompt
    async fn generate_nl_description_and_name_from_prompt(
        &self,
        prompt: &str,
        _name: &str,  // Keep for compatibility but we'll generate our own
        installed_apps: &[String],
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

        // Dynamic CLI tools from startup probe
        let cli_tools_section = crate::engine::cli_probe::get_cached_tools_str()
            .unwrap_or_else(|| String::from("\nAVAILABLE CLI TOOLS (pre-approved): git, npm, pip, cargo, brew, node, python3\n"));

        // System prompt that balances specificity with conciseness
        let system_prompt = format!(r#"You are an expert agent script writer.
Your job is to create a workflow script that a less powerful LLM will follow that will lead to HIGH QUALITY COMPLETION OF USER's REQUEST. Create a clear numbered list of steps for the given task, and provide a concise name for the task.

OUTPUT FORMAT:
First line: NAME: <concise 2-5 word name for the taks>
Then a blank line
Then the numbered steps

NAME RULES:
- 2-5 words maximum
- Be descriptive but concise
- Use action words (e.g., "Extract LinkedIn Contacts", "Collect News Headlines", "Research Product Prices")
- Avoid generic names like "Web Task" or "Browser Task"
- Don't include punctuation or special characters

CRITICAL RULES:
1. OUTPUT APPS: Only use apps (Excel, Word, etc.) when the user EXPLICITLY asks to use them OR when it's clearly sensible for the task. Collected data in memory is automatically shown to the user at completion so you don't have to save collected data unless user explicitly instructs you to.
2. SPECIFY EXACT application names from the available apps list when needed, STRONGLY prefer most popular apps (e.g., Chrome over Firefox, Gmail over other email clients, Microsoft Office over alternatives)
3. Be specific about UI elements and actions but skip obvious intermediate steps
4. Combine navigation and action when sensible (e.g., "Navigate to google.com and search for...")
5. Include exact URLs, button names, and field labels when certain they exist
6. One main action per step - be clear and direct
7. NEVER include login/authentication steps - the system handles those automatically via takeover when needed
8. Current Date: {}. Take today's date /year into account if there is date explicitly or implicitly embedded in the user request 

EXCEL & WORD - DON'T OVER-SPECIFY:
- The executor has access to SPECIAL INPUT AND FORMATTING COMMANDS for Excel and Word
- DO say WHAT to include: "Add the collected data to Excel with headers" or "Format the document professionally"
- Example: ✓ "Enter the data into Excel" NOT ✗ "Put title in Cell C1 or "Change headers in row 1, data starting row 2, bold the headers, add borders" 

TERMINAL COMMANDS (macOS) - DIRECT EXECUTION:
- The executor has DIRECT TERMINAL ACCESS via TERMINAL_RUN command - NO NEED to open Terminal.app!
- For git, npm, pip, cargo, shell commands: Simply say "Run git clone <url>" or "Run npm install"
- ✓ GOOD: "Run 'git clone https://github.com/user/repo'" (direct terminal execution)
- ✗ BAD: "Open Terminal, type 'cd ~', press Enter, type 'git clone...'" (unnecessary UI automation)
- The executor handles working directories automatically (defaults to ~/Linefox)
- For long-running commands (servers, watchers): Say "Start the dev server in background"

⚠️ AVOID SLOW COMMANDS - Commands timeout after 15 minutes!
- ✓ GOOD: "ls ~/Linefox" or "find ~/Linefox -name '*project*'" (search specific directories)
- ALL code files are stored in ~/Linefox by default - search there first, not ~

{cli_tools}

⚠️ CLAUDE CLI SESSION & PERMISSION MANAGEMENT - CRITICAL:
Each 'claude "prompt"' starts a NEW session with NO memory of previous commands!

⚠️ IMPORTANT: Claude CLI commands must START with "claude" - no cd prefix!
The default working directory is ~/Linefox. Include a specific path in your prompt if needed.

SESSION: Use --continue for follow-up commands to maintain context.
PERMISSIONS: File write permissions and bash access are granted automatically — do NOT add --permission-mode.

Without --continue, Claude forgets previous context.

Example Claude workflow in steps:
1. Run 'claude -p "implement user authentication in ~/myproject"' (starts session)
2. Run 'claude -p --continue "now write the code to files"' (continues SAME session)
3. Run 'git add . && git commit -m "Add auth feature"' (commit changes)

When user mentions code review, debugging, or asks for AI help with code, consider using these CLI tools!

CODING TASKS - LOCAL REPO AWARENESS (CRITICAL):
When the user asks to modify, update, fix, or work with a repository/project:
1. FIRST: Run 'ls ~/Linefox' to see what projects already exist locally
2. If repo/project exists locally: Run 'git -C ~/Linefox/<repo> status' and 'git -C ~/Linefox/<repo> branch' to understand current state (dirty tree? current branch? unpushed commits?)
3. If repo does NOT exist locally: THEN clone from GitHub with 'git clone <url> ~/Linefox/<repo>'
4. NEVER clone a repo that already exists in ~/Linefox — always work with the local copy
5. When user says "this repo" or "my project" without specifying a name, list ~/Linefox first to identify it

OCCAM'S RAZOR - SIMPLEST PATH FIRST:
- For public documents (academic articles, blogs, 10-Ks): Google it first!
  ✗ BAD: "Go to investor.apple.com, click Financials, click SEC Filings, find 10-K..."
  ✓ GOOD: "Google 'Apple 10-K 2024 SEC filing' and open the direct link"
- The simplest path that achieves the goal is the best path

SIMPLE LOOKUP RULE:
• Factual questions, rankings, prices, "best X in Y" → 2-3 steps MAX. Google it, extract the answer.
• Try Google first. Only navigate to specialized sites (FIDE, ESPN, stock exchanges) if the user asks or the info isn't in search results.
• If the answer appears on a Google results page or the first link, do NOT build a multi-site research workflow.
(See Examples 7 & 8 below for exactly how short these plans should be.)

UI ELEMENT GUIDELINES (PREVENT WILD GOOSE CHASES):
- For COMMON/STANDARD elements: Be specific (e.g., "Click the search button", "Enter text in the search box")
- For UNCERTAIN elements: Use GENERIC descriptions:
  ✓ "Look for filters or sorting options" (not "Click the 'Advanced Filters' dropdown")
  ✓ "Find and click the submit/continue button" (not "Click the blue 'Next Step' button")
  ✓ "Navigate to settings/preferences" (not "Click the gear icon in top-right corner")
- NEVER specify:
  - Exact positions unless absolutely certain (avoid "top-right", "bottom-left")
  - Features that may not exist (avoid "Click Advanced Options" unless you KNOW it exists)
- When unsure, describe the INTENT not the specific element:
  ✓ "Search for 'machine learning'" (intent clear, method flexible)
  ✗ "Click the magnifying glass icon in the header" (too specific if uncertain)
- UNCERTAIN elements: Be generic ("Look for filters or sorting options")
- Describe INTENT not specific elements: "Search for 'X'" not "Click the magnifying glass"
- For long documents (10-K filings, articles, reports): Say "Request full text to read the document" - this is faster than scrolling page by page

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
1. Navigate to reddit.com and search for 'best productivity tips'
2. Sort results by relevance, click the first highly-upvoted post and extract key tips to memory
3. Go back and check 2-3 more top posts, extract any additional valuable tips to memory

Example 2 - User asks: "Research top AI tools and save to Excel"
1. Google 'best AI tools 2026', open the first reputable source (TechCrunch, The Verge, etc.)
2. Extract the top 10 AI tools with descriptions and pricing to memory
3. Open Excel, create headers (Tool Name, Description, Pricing), type the tools from memory, save as 'AI_Tools_Research.xlsx'

Example 3 - User asks: "Find me 10 Airbnb houses with a pool, Oct 25–27 in Maui" (request date: Jan 1, 2026)
1. Navigate to airbnb.com, enter "Maui" as destination, set check-in Oct 25 and check-out Oct 27 2026, click Search
2. Use filters to add "Pool" as an amenity
3. Click the first listing (use the AXLINK element — if nothing opens, you clicked the map; try the text link)
4. Review details and extract: name, location, price/night, bedrooms/bathrooms, pool type, host rating to memory
5. Click back and repeat steps 3–4 for listings 2 through 10

Example 4 - User asks: "Clone the top trending repo from GitHub and show me its README"
1. Run 'ls ~/Linefox' to check existing repos; navigate to github.com/trending
2. Identify the top repo and run 'git clone <url> ~/Linefox/<repo-name>' (skip if already exists)
3. Run 'cat ~/Linefox/<repo-name>/README.md' and extract key purpose/setup info to memory

Example 5 - User asks: "Review my project / implement a feature / fix a bug with Claude"
(Covers: code review, feature implementation, responsive design, bug fixes — any Claude CLI coding task)
1. Run 'ls ~/Linefox' to identify the project; run 'git -C ~/Linefox/<project> status && git branch' to check state
2. Run 'claude -p "<task description> in ~/Linefox/<project>"' (permissions granted automatically)
3. Run 'claude -p --continue "verify changes are complete and correct"' (same session)
4. Run 'git -C ~/Linefox/<project> add . && git commit -m "<summary>"' to commit
5. Extract summary of changes to memory

Example 6 - User asks: "Set up a new project"
1. Run 'ls ~/Linefox' to avoid name conflicts
2. Run the appropriate scaffold command (e.g. 'npx create-react-app ~/Linefox/my-app' or 'cargo new ~/Linefox/my-app')
3. Run the dev server in background; extract the local URL to memory

Example 7 - User asks: "Find the highest-rated chess player in the world"
1. Google "highest rated chess player 2026" and read the search results page
2. Extract the player's name, rating, and country to memory
(The results page shows the answer directly — no need to navigate to FIDE.)

Example 8 - User asks: "Find the best hotel in Boston" / "What's Tesla's stock price?"
1. Google the query and read the results page or first link
2. Extract the top 2-3 options (or the direct answer) with key details to memory
(For rankings, prices, stats: one Google search is enough. Do not build a multi-site workflow.)

GOOD GENERIC EXAMPLES (when UI is uncertain):
✓ "Look for sorting or filtering options to show top/relevant results"
✓ "Find and use the search functionality"
✓ "Navigate to settings or preferences section"
✓ "Look for export, download, or save options"
✓ "Find the main content area and extract relevant information"
✓ "Go back to previous page" (instead of "Click the back arrow button")

BAD EXAMPLES (never include these):
✗ "Copy the jokes to clipboard" (use "Extract to memory" instead)
✗ "Paste the content" (use "Type from memory" instead)
✗ "Select all and copy" (use "Extract to memory" instead)
✗ "Enter your email in the 'Email' field" (authentication handled automatically)
✗ "Type your password" (authentication handled automatically)
✗ "Click Sign in" (authentication handled automatically)
✗ "Click the blue 'Apply Filters' button in the sidebar" (too specific - color/location may not exist)
✗ "Click the three-dot menu icon next to the profile picture" (assumes specific UI exists)
✗ "Select 'Advanced Search' from the dropdown menu" (assumes feature exists)
✗ "Click the back arrow button in top-left" (too specific - say "Go back to previous page")
✗ "Scroll down and look at the 5th comment" (too prescriptive - say "Read through top comments")
✗ "Open Terminal application and type 'git clone...'" (use direct terminal: "Run 'git clone...'")
✗ "Navigate to home directory and run npm install" (just say "Run 'npm install'" - cwd is automatic)

REPETITIVE TASKS:
- Be explicit about completion conditions
- "Repeat steps 4-7 for each search result until 10 items collected or no more results"
- "Process each LinkedIn profile with 'Director' or 'VP' in title"

Output the NAME line, then a blank line, then the numbered steps. No other text."#, current_date, cli_tools = cli_tools_section);

        // Build user prompt
        let user_prompt = format!(
            "Task: {}\n\nCreate workflow steps for this objective. Be specific about applications and actions.",
            prompt
        );

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
            "grok" => {
                info!("🔑 Using Grok API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::grok::call_llm_api(
                        &self.api_key, user_prompt, &system_prompt, 1500
                    ).await?;
                response_text
            },
            "deepseek" => {
                info!("🔑 Using DeepSeek API for NL description and name");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::deepseek::call_llm_api(
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

        // Parse the response to extract name and steps
        let response_trimmed = response_text.trim();
        let lines: Vec<&str> = response_trimmed.lines().collect();

        // Extract the name from the first line
        let generated_name = if !lines.is_empty() && lines[0].starts_with("NAME:") {
            lines[0].strip_prefix("NAME:").unwrap_or("").trim().to_string()
        } else {
            // Fallback to using first words of prompt if parsing fails
            prompt.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
        };

        // Extract the steps (everything after the name line and blank line)
        let nl_description = if lines.len() > 2 {
            // Skip the NAME line and any blank lines, join the rest
            lines.iter()
                .skip_while(|line| line.starts_with("NAME:") || line.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        } else {
            // Fallback to full response if parsing fails
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
            "grok" => {
                info!("🔑 Using Grok API for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::grok::call_llm_api(
                        &self.api_key, user_prompt.clone(), system_prompt, 1500
                    ).await?;
                response_text
            }
            "deepseek" => {
                info!("🔑 Using DeepSeek API for continuation plan");
                let (response_text, _input_tokens, _output_tokens) =
                    crate::engine::llm_providers::deepseek::call_llm_api(
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