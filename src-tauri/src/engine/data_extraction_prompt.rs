/// Returns the prompt for extracting structured data from task memory for future use
pub fn get_data_extraction_prompt(
    objective: &str,
    memory: &str,
    recent_actions: &str,
) -> String {
    format!(r#"You are analyzing the results of a completed automation task to extract structured data worth preserving for future reference.

# YOUR TASK
Review the task objective, memory bank, and recent actions. Determine if there is data worth storing for future use. If so, extract it into structured records.

# CRITERIA FOR USEFUL DATA
Data is worth storing if it:
- Contains specific, factual information (names, emails, prices, dates, etc.)
- Could be useful for future lookups or reference
- Has clear structure or can be organized into fields
- Represents research findings, contacts, products, financial info, etc.

Data is NOT worth storing if:
- It's purely procedural (e.g., "clicked button X", "navigated to page Y")
- It's temporary or ephemeral (e.g., "loading...", "processing...")
- It's already available elsewhere (e.g., public facts easily searched)
- The memory is empty or only contains confirmation messages

# RECORD TYPES
Use these standard categories (or create a relevant one):
- "contact" - People with emails, phone numbers, job titles
- "company" - Business information, funding, employee counts
- "product" - Items with prices, features, specifications
- "financial_report" - 10-K, quarterly reports, earnings data
- "price_comparison" - Pricing data from multiple sources
- "article" - News articles, blog posts with summaries
- "research" - General research findings
- "property" - Real estate listings, Airbnb, hotels
- "flight" - Travel bookings, flight details
- "event" - Conferences, meetings, dates

# OUTPUT FORMAT
You MUST respond with valid JSON in exactly this format:
```json
{{
  "has_useful_data": true/false,
  "reasoning": "Brief explanation of why data is/isn't worth storing",
  "records": [
    {{
      "record_name": "Human-readable identifier",
      "record_type": "category",
      "source_context": "Where this data came from",
      "data": {{
        "field1": "value1",
        "field2": "value2"
      }}
    }}
  ]
}}
```

# EXAMPLES

## Example 1: Contact Information
**Objective**: Find ML engineers at Series B startups

**Memory**:
Contact 1: Sarah Chen - sarah.chen@techstartup.io - Senior SWE at CloudScale (Series B, $30M)
Contact 2: Michael Rodriguez - michael.rodriguez@dataflow.com - Lead Engineer at DataFlow

**Your Response**:
```json
{{
  "has_useful_data": true,
  "reasoning": "Memory contains contact information with emails, job titles, and company details that could be useful for future outreach",
  "records": [
    {{
      "record_name": "Sarah Chen",
      "record_type": "contact",
      "source_context": "LinkedIn search for ML engineers at Series B startups",
      "data": {{
        "email": "sarah.chen@techstartup.io",
        "title": "Senior SWE",
        "company": "CloudScale",
        "funding_stage": "Series B",
        "funding_amount": "$30M"
      }}
    }},
    {{
      "record_name": "Michael Rodriguez",
      "record_type": "contact",
      "source_context": "LinkedIn search for ML engineers at Series B startups",
      "data": {{
        "email": "michael.rodriguez@dataflow.com",
        "title": "Lead Engineer",
        "company": "DataFlow"
      }}
    }}
  ]
}}
```

## Example 2: Product Research
**Objective**: Compare robot vacuums on Amazon

**Memory**:
Roomba i7+: $799, 4.5 stars, self-emptying, smart mapping
Roborock S7: $649, 4.6 stars, sonic mopping, LiDAR navigation
Shark IQ: $449, 4.2 stars, self-emptying, row-by-row cleaning

**Your Response**:
```json
{{
  "has_useful_data": true,
  "reasoning": "Memory contains product comparison data with prices, ratings, and features useful for purchase decisions",
  "records": [
    {{
      "record_name": "Roomba i7+",
      "record_type": "product",
      "source_context": "Amazon robot vacuum comparison",
      "data": {{
        "price": "$799",
        "rating": "4.5 stars",
        "key_features": ["self-emptying", "smart mapping"],
        "brand": "iRobot"
      }}
    }},
    {{
      "record_name": "Roborock S7",
      "record_type": "product",
      "source_context": "Amazon robot vacuum comparison",
      "data": {{
        "price": "$649",
        "rating": "4.6 stars",
        "key_features": ["sonic mopping", "LiDAR navigation"],
        "brand": "Roborock"
      }}
    }},
    {{
      "record_name": "Shark IQ",
      "record_type": "product",
      "source_context": "Amazon robot vacuum comparison",
      "data": {{
        "price": "$449",
        "rating": "4.2 stars",
        "key_features": ["self-emptying", "row-by-row cleaning"],
        "brand": "Shark"
      }}
    }}
  ]
}}
```

## Example 3: No Useful Data
**Objective**: Send email to John

**Memory**:
Email sent successfully to john@company.com

**Your Response**:
```json
{{
  "has_useful_data": false,
  "reasoning": "Memory only contains a confirmation message, no structured data worth preserving",
  "records": []
}}
```

## Example 4: Financial Report
**Objective**: Get Apple's latest 10-K summary

**Memory**:
Apple 10-K FY2024: Revenue $394.3B (up 2%), Net Income $96.7B, Services revenue grew 14% to $85.2B, iPhone revenue $200.6B, Gross margin 46.6%

**Your Response**:
```json
{{
  "has_useful_data": true,
  "reasoning": "Memory contains key financial metrics from Apple's annual report that could be useful for analysis",
  "records": [
    {{
      "record_name": "Apple 10-K FY2024",
      "record_type": "financial_report",
      "source_context": "SEC 10-K filing for Apple Inc.",
      "data": {{
        "company": "Apple Inc.",
        "fiscal_year": "2024",
        "total_revenue": "$394.3B",
        "revenue_growth": "2%",
        "net_income": "$96.7B",
        "services_revenue": "$85.2B",
        "iphone_revenue": "$200.6B",
        "gross_margin": "46.6%"
      }}
    }}
  ]
}}
```

# YOUR CONTEXT

**Objective**: {}

**Memory Bank**:
{}

**Recent Actions** (most recent first):
{}

# INSTRUCTIONS
1. Analyze the memory content carefully
2. Determine if there's structured data worth preserving
3. Extract data into individual records with appropriate types
4. Output ONLY the JSON response - no additional text"#,
        objective,
        if memory.is_empty() { "(empty)" } else { memory },
        recent_actions
    )
}
