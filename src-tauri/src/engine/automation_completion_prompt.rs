/// Returns the completion prompt for generating a closing message after automation finishes
pub fn get_completion_prompt(
    objective: &str,
    memory: &str,
    recent_actions: &str,
) -> String {
    format!(r#"You are generating a closing message to inform the user about the completion of their automation task.

# YOUR TASK
Review the automation objective, memory bank, and recent actions, then provide a natural, conversational response that:
1. If the user asked for information (e.g., "summarize this", "find properties"), deliver the answer directly like you're having a conversation
2. If the user requested an action (e.g., "add to Excel", "send emails"), briefly confirm what was done

# GUIDELINES
- Write naturally and conversationally, as if speaking directly to the user
- For information requests: Lead with the answer/information, not "I found..." or "I completed..."
- For queries like "summarize this article" → Start with "Here's the summary:" or just give the summary
- For queries like "find properties" → You can say "I found..." or "Here are..." - be natural
- Don't use task-oriented language like "accomplished", "extracted", "executed"
- Use casual, helpful language like a knowledgeable assistant
- If data was saved to a file, mention where

# CRITICAL: WHERE DOES THE OUTPUT GO?
The key question is: did the user ask to save/export the data somewhere else, or does the chat UI deliver the result?

**If output is saved elsewhere** (file, spreadsheet, email, database, etc.):
- Brief confirmation: "Saved 20 contacts to contacts.xlsx" or "Sent follow-up emails to 5 contacts"
- 2-4 sentences is appropriate

**If output is delivered in this chat** (no save/export destination):
- This message IS the deliverable - present the full data
- Include ALL relevant items from memory, formatted for easy reading
- Organize clearly (by category, source, etc.) when there's lots of data
- Match the user's requested format ("nicely formatted list", "comparison table", etc.)
- Filter out irrelevant items that don't match the user's criteria (e.g., if they asked for "AI headlines", skip non-AI headlines)

# EXAMPLES

## Example 1: Direct Query (Browsing/Finding Information)
**Objective**: Browse me some 2-bedroom apartments on Airbnb in San Francisco under $200/night

**Memory**:
Property 1: Cozy Mission District Loft - $175/night, 2BR/1BA, 4.9 stars, near BART
Property 2: Sunset District Home - $185/night, 2BR/2BA, 4.8 stars, ocean views
Property 3: Downtown Studio Plus Den - $195/night, 2BR/1BA, 4.7 stars, walkable

**Your Response**:
I found 3 great options for you:

1. **Cozy Mission District Loft** - $175/night, 2BR/1BA, rated 4.9⭐, convenient BART access
2. **Sunset District Home** - $185/night, 2BR/2BA, rated 4.8⭐, beautiful ocean views
3. **Downtown Studio Plus Den** - $195/night, 2BR/1BA, rated 4.7⭐, highly walkable area

All properties are under $200/night and available for your dates.

## Example 2: Direct Query (Contact Information)
**Objective**: Find contact emails for software engineers at Series B startups on LinkedIn

**Memory**:
Contact 1: sarah.chen@techstartup.io - Senior SWE at CloudScale (Series B, $30M raised)
Contact 2: michael.rodriguez@dataflow.com - Lead Engineer at DataFlow (Series B, $25M raised)
Contact 3: jenny.park@aiplatform.ai - Staff Engineer at AIPlatform (Series B, $40M raised)

**Your Response**:
Found 3 software engineers at Series B startups:

• **Sarah Chen** (sarah.chen@techstartup.io) - Senior SWE at CloudScale ($30M Series B)
• **Michael Rodriguez** (michael.rodriguez@dataflow.com) - Lead Engineer at DataFlow ($25M Series B)
• **Jenny Park** (jenny.park@aiplatform.ai) - Staff Engineer at AIPlatform ($40M Series B)

## Example 3: Action Task (Data Entry)
**Objective**: Extract contact information from my inbox and add them to Excel

**Memory**:
Contact 1: John Smith - john.smith@acme.com - Acme Corp
Contact 2: Lisa Wong - lwong@techco.io - TechCo
[... 8 more contacts ...]

**Your Response**:
Successfully extracted 10 contacts from your inbox and added them to Excel at ~/Documents/HeelixOutput/contacts.xlsx. The spreadsheet includes names, email addresses, and company information for each contact.

## Example 4: Action Task (Sending Communications)
**Objective**: Send follow-up emails to the 5 people I met at the conference

**Memory**:
Sent to: alex@startup.com, priya@venture.vc, tom@enterprise.com, sofia@agency.co, mike@consulting.biz

**Your Response**:
Sent personalized follow-up emails to all 5 conference contacts: Alex (startup.com), Priya (venture.vc), Tom (enterprise.com), Sofia (agency.co), and Mike (consulting.biz). Each email referenced your conversation and included your availability for a follow-up call.

## Example 5: Research Task
**Objective**: Compare pricing for project management tools

**Memory**:
Asana: $10.99/user/mo (Standard), $24.99/user/mo (Business)
Monday.com: $9/user/mo (Standard), $14/user/mo (Pro)
ClickUp: $7/user/mo (Unlimited), $12/user/mo (Business)
Notion: $8/user/mo (Plus), $15/user/mo (Business)

**Your Response**:
Here's the pricing comparison:

• **ClickUp**: $7-12/user/mo (most affordable)
• **Notion**: $8-15/user/mo
• **Monday.com**: $9-14/user/mo
• **Asana**: $10.99-24.99/user/mo

ClickUp offers the lowest entry price at $7/user/month for their Unlimited plan.

## Example 6: Summarization Task
**Objective**: Summarize the top article on BBC News

**Memory**:
Article: "BBC now faces tough legal battle with world's most powerful man. What happens next?" by Katie Razzall
Content: The BBC is facing a potential lawsuit from President Trump, who is demanding billions in damages and refuses to accept the corporation's apology for what they claim was an unintentional error. Despite the BBC's attempts to resolve the matter, Trump remains unsatisfied and is pursuing legal action.

**Your Response**:
Here's the top BBC News article: **"BBC now faces tough legal battle with world's most powerful man. What happens next?"** by Katie Razzall.

The BBC is facing a potential lawsuit from President Trump over what the corporation calls an unintentional error. Trump is demanding billions in damages and has rejected the BBC's apology. The corporation now faces an expensive legal battle during a critical time when they should be focusing on charter renewal, with key leadership transitions also taking place.

## Example 7: Comprehensive List (Output delivered in chat)
**Objective**: Review BBC, FT, Bloomberg - I want a nicely formatted list of tech/AI headlines

**Memory**: [Contains 10+ headlines per source, mix of tech/AI and unrelated topics]

**Your Response**:
Here are the tech/AI headlines:

**BBC Business**
- Meta buys Chinese-founded AI start-up Manus
- China to crack down on AI firms

**Financial Times**
- AI forecast to put 200,000 European banking jobs at risk by 2030
- Microsoft's Nadella overhauls leadership as he plots AI strategy

**Bloomberg**
- Xi Touts China's AI, Chip Wins In New Year's Speech
- Musk's xAI Buys Building to Expand Data Center

*Key: Include ALL relevant items from memory, filter out off-topic items, organize by category*

# YOUR CONTEXT

**Objective**: {}

**Memory Bank**:
{}

**Recent Actions** (most recent first):
{}

# OUTPUT FORMAT
Provide ONLY the closing message text. No preamble, no "Here's the message:", just the message itself.
- Output saved elsewhere: 2-4 sentences confirming what was done
- Output delivered here: Present ALL the data in a clean, easy-to-digest format"#,
        objective,
        if memory.is_empty() { "(empty)" } else { memory },
        recent_actions
    )
}
