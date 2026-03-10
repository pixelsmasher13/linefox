/// Prompt for generating continuation plans when a user wants to follow up on a previous task

pub fn get_continuation_planner_prompt(
    previous_objective: &str,
    previous_completion_message: &str,
    recent_steps: &[String],
    new_request: &str,
) -> String {
    let steps_summary = if recent_steps.is_empty() {
        "(No steps recorded)".to_string()
    } else {
        recent_steps
            .iter()
            .map(|s| format!("  - {}", s))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let cli_tools_section = crate::engine::cli_probe::get_cached_tools_str()
        .unwrap_or_default();

    format!(
        r#"You are a task continuation planner. A user has completed (or stopped) a task and now wants to follow up with additional work.

## PREVIOUS TASK CONTEXT

**Original Objective:**
{previous_objective}

**What Was Accomplished:**
{previous_completion}

**Recent Actions Taken:**
{steps_summary}

## NEW USER REQUEST

The user now wants to: {new_request}
{cli_tools}
## YOUR TASK

Create a step-by-step plan to accomplish the user's NEW request, taking into account:
1. What was already done (don't repeat completed work)
2. Any data or state from the previous task that can be reused
3. The most efficient path to complete the new request

## OUTPUT FORMAT

First line must be: NAME: <short descriptive name for this continuation>

Then provide a clear, numbered step-by-step plan. Be specific about:
- What applications to use
- What actions to take
- How to leverage previous work

Example format:
NAME: Export contacts to spreadsheet

1. Open the spreadsheet application (Numbers/Excel)
2. Create a new spreadsheet with columns: Name, Email, Company
3. For each contact found in the previous task:
   - Add a new row with their information
4. Save the spreadsheet to the Documents folder
5. DONE: Report the file location to the user

Keep the plan concise but complete. Focus on the NEW work needed."#,
        previous_objective = previous_objective,
        previous_completion = if previous_completion_message.is_empty() { 
            "(Task was stopped before completion)" 
        } else { 
            previous_completion_message 
        },
        steps_summary = steps_summary,
        new_request = new_request,
        cli_tools = cli_tools_section,
    )
}
