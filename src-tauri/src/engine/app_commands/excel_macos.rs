//! Excel-specific AppleScript commands for macOS
//!
//! These commands provide direct Excel automation via AppleScript,
//! which is more reliable than clicking UI elements.

use log::{info, warn, error};

/// Auto-fetch lightweight Excel context: active sheet name, all sheet names, used range.
/// This is called automatically when Excel is the active app to give the LLM orientation.
pub async fn get_excel_sheet_context() -> Result<String, String> {
    let script = r#"
try
    tell application "Microsoft Excel"
        set workbookName to "(no workbook)"
        try
            set workbookName to name of active workbook
        end try
        set sheetName to name of active sheet
        set sheetList to ""
        try
            set allSheets to sheets of active workbook
            repeat with i from 1 to count of allSheets
                set s to item i of allSheets
                if i = 1 then
                    set sheetList to (name of s)
                else
                    set sheetList to sheetList & ", " & (name of s)
                end if
            end repeat
        end try
        set numRows to 0
        set numCols to 0
        try
            set theRange to used range of active sheet
            set numRows to count of rows of theRange
            set numCols to count of columns of theRange
        end try
        return "Workbook: " & workbookName & " | Active sheet: " & sheetName & " | All sheets: " & sheetList & " | Used range (active sheet): " & numRows & " rows x " & numCols & " cols"
    end tell
on error errMsg
    return "Excel context unavailable"
end try
"#;
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .await
        .map_err(|e| format!("Failed to run AppleScript: {}", e))?;

    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(result)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("Excel context fetch failed: {}", stderr))
    }
}

/// Returns the prompt section documenting Excel-specific commands
pub fn get_excel_commands_prompt() -> String {
    r#"
────────────────────────────
# EXCEL-SPECIFIC COMMANDS (macOS)
────────────────────────────
⚠️ These commands are ONLY available when Microsoft Excel is the active application.
They use native AppleScript automation and are MORE RELIABLE than clicking UI elements.
PREFER these commands over CLICK/TYPE when working with Excel.

────────────────────────────
## CELL DATA ENTRY - USE EXCEL_TYPE FOR EVERYTHING!
────────────────────────────
⚡ ENTER ALL DATA IN ONE COMMAND. Do NOT split across multiple turns.
Build the entire sheet (labels, values, formulas) in a single EXCEL_TYPE call.

**EXCEL_TYPE - supports two formats:**

**Format 1: Pipe-separated (good for small batches)**
- EXCEL_TYPE:<cell>:<value>[|||<cell>:<value>...]
- Example: EXCEL_TYPE:A1:Revenue|||B1:2024|||C1:=SUM(B2:B9)

**Format 2: JSON (preferred for large data entry - use this to populate entire sheets)**
- EXCEL_TYPE:{"A1": "Revenue", "B1": 2024, "B2": "=B1*1.1", "C1": "Cost", ...}
- ✅ Handles all values cleanly: strings, numbers, formulas, empty cells (null)
- ✅ No delimiter conflicts — use this when entering 10+ cells
- Example: EXCEL_TYPE:{"A1": "Header", "A2": 1000, "A3": "=A2*1.1", "B1": "Col B", "B2": null}

Rules:
- Numbers: use raw numbers (1000, 0.15), NOT quoted strings
- Formulas: prefix with = ("=SUM(B2:B9)")
- Empty cell: use null in JSON or skip the cell
- ✅ Works for text, numbers, AND formulas
- ⚡ Enter ALL data for a section/sheet at once — do NOT use multiple turns

────────────────────────────
## SHEET MANAGEMENT
────────────────────────────

✅ ||| CHAINING works for sheet management too! Create/rename/delete multiple sheets in ONE command.
⚡ When scaffolding a multi-sheet workbook (e.g. financial model), create ALL sheets in a single chained call BEFORE entering data. Do NOT create sheets one per turn.

**EXCEL_SELECT_SHEET:<name>**
- Switches to/activates the specified worksheet
- Before SELECT, check the `has sheets: [...]` list in the previous response. If the sheet isn't there, use EXCEL_NEW_SHEET first — selecting a non-existent sheet errors with -1728.
- Example: EXCEL_SELECT_SHEET:Summary

**EXCEL_RENAME_SHEET:<old_name>:<new_name>**
- Renames a worksheet
- Example: EXCEL_RENAME_SHEET:Sheet1:Sales Data

**EXCEL_NEW_SHEET:<name>**
- Creates a new worksheet (appended to end of workbook)
- Example: EXCEL_NEW_SHEET:Q4 Report
- Example chained (scaffold whole model at once): EXCEL_NEW_SHEET:Historicals|||EXCEL_NEW_SHEET:Assumptions|||EXCEL_NEW_SHEET:Projections|||EXCEL_NEW_SHEET:Valuation|||EXCEL_NEW_SHEET:Summary
- Example mixed (rename default + add the rest): EXCEL_RENAME_SHEET:Sheet1:Cover|||EXCEL_NEW_SHEET:Historicals|||EXCEL_NEW_SHEET:Assumptions|||EXCEL_NEW_SHEET:Projections

**EXCEL_DELETE_SHEET:<name>**
- Deletes the specified worksheet
- Example: EXCEL_DELETE_SHEET:Sheet2

────────────────────────────
## FORMATTING
────────────────────────────

✅ ||| CHAINING works for formatting commands too! Apply multiple ranges in ONE command:
- EXCEL_BOLD:A1:D1|||A5:D5|||A8:D8 || Bold all header rows at once
- EXCEL_FORMAT_CELLS:B2:B10:currency|||C2:C10:percent || Format multiple ranges
- EXCEL_SET_FILL_COLOR:A1:G1:blue|||A5:G5:gray || Color multiple rows

**EXCEL_FORMAT_CELLS:<range>:<format>**
- Named formats: currency, currency0, percent, percent0, percent1, number, number0, number1, date, text, general
- Or pass a custom Excel format string directly (e.g. #,##0.000 or $#,##0;($#,##0))
- currency0 = no decimals ($1,234), number0 = no decimals (1,234), percent1 = one decimal (12.5%)
- Example: EXCEL_FORMAT_CELLS:B2:B100:currency
- Example: EXCEL_FORMAT_CELLS:B2:B10:number0 || Thousands with no decimals
- Example chained: EXCEL_FORMAT_CELLS:B2:B10:currency|||C2:C10:percent|||D2:D10:number0

**EXCEL_BOLD:<range>**
- Makes the specified range bold
- Example: EXCEL_BOLD:A1:D1

**EXCEL_ITALIC:<range>**
- Makes the specified range italic
- Example: EXCEL_ITALIC:A1:D1

**EXCEL_UNDERLINE:<range>**
- Underlines the specified range
- Example: EXCEL_UNDERLINE:A1:D1

**EXCEL_SET_FONT:<range>:<font_name>**
- Sets font family
- Example: EXCEL_SET_FONT:A1:G1:Arial
- Example: EXCEL_SET_FONT:A1:Z100:Calibri

**EXCEL_SET_FONT_SIZE:<range>:<size>**
- Sets font size in points
- Example: EXCEL_SET_FONT_SIZE:A1:G1:14
- Example: EXCEL_SET_FONT_SIZE:A2:G50:9

**EXCEL_SET_FILL_COLOR:<range>:<color>**
- Sets background/fill color of cells
- Colors: named (yellow, green, blue, red, orange, gray, lightgray, white, black), hex (#RRGGBB or #RGB), or RGB (255,200,200)
- Example: EXCEL_SET_FILL_COLOR:A1:G1:yellow || Named color
- Example: EXCEL_SET_FILL_COLOR:A1:G1:#1F4E79 || Hex color (dark blue)
- Example: EXCEL_SET_FILL_COLOR:B5:B10:255,230,230 || RGB triplet
- Example: EXCEL_SET_FILL_COLOR:A1:I1:#D9E1F2 || Light blue header

**EXCEL_SET_FONT_COLOR:<range>:<color>**
- Sets text/font color of cells
- Colors: same as fill colors (named, hex #RRGGBB, or RGB triplet)
- Example: EXCEL_SET_FONT_COLOR:A1:G1:white || Named
- Example: EXCEL_SET_FONT_COLOR:B2:F10:#0000FF || Blue for input cells
- Example: EXCEL_SET_FONT_COLOR:A1:G1:#FFFFFF || White on dark background

**EXCEL_ADD_BORDER:<range>:<style>**
- Adds borders to cells
- Styles: all, outline, top, bottom, left, right, none
- Example: EXCEL_ADD_BORDER:A1:D10:all || Add all borders (grid)
- Example: EXCEL_ADD_BORDER:A1:D1:bottom || Add bottom border to header row
- Example: EXCEL_ADD_BORDER:A1:D10:outline || Add outline only (no inner borders)

**EXCEL_MERGE_CELLS:<range>**
- Merges a range of cells into one
- Example: EXCEL_MERGE_CELLS:A1:D1 || Merge header across columns

**EXCEL_UNMERGE_CELLS:<range>**
- Unmerges previously merged cells
- Example: EXCEL_UNMERGE_CELLS:A1:D1

**EXCEL_ALIGN:<range>:<alignment>**
- Sets horizontal alignment: left, center, right
- Example: EXCEL_ALIGN:A1:G1:center || Center header row

**EXCEL_VERTICAL_ALIGN:<range>:<alignment>**
- Sets vertical alignment: top, center (or middle), bottom
- Example: EXCEL_VERTICAL_ALIGN:A1:G1:center

**EXCEL_WRAP_TEXT:<range>**
- Enables text wrapping in cells
- Example: EXCEL_WRAP_TEXT:A1:D10

**EXCEL_SET_ROW_HEIGHT:<row>:<height>**
- Sets the height of a specific row in points
- Example: EXCEL_SET_ROW_HEIGHT:1:30 || Tall header row
- Example: EXCEL_SET_ROW_HEIGHT:5:5 || Thin separator row

**EXCEL_AUTOFIT_COLUMNS:<range>**
- Auto-fits column widths
- Example: EXCEL_AUTOFIT_COLUMNS:A:D

**EXCEL_SET_COLUMN_WIDTH:<column>:<width>**
- Sets a specific column width
- Example: EXCEL_SET_COLUMN_WIDTH:A:25

────────────────────────────
## VIEW
────────────────────────────

**EXCEL_FREEZE_PANES:<cell>**
- Freezes rows above and columns left of the specified cell
- Example: EXCEL_FREEZE_PANES:B2 || Freeze row 1 and column A (most common for financial models)
- Example: EXCEL_FREEZE_PANES:A2 || Freeze row 1 only

**EXCEL_UNFREEZE_PANES**
- Removes frozen panes
- Example: EXCEL_UNFREEZE_PANES || Removing frozen panes

────────────────────────────
## WORKBOOK MANAGEMENT
────────────────────────────

**EXCEL_NEW_WORKBOOK** ⚠️ DESTRUCTIVE — USE AT MOST ONCE PER AUTOMATION
- Creates a NEW blank workbook in a SEPARATE file. ANY sheets/data in the previous workbook still exist but are now in a background workbook you've abandoned. This is effectively starting over.
- Use ONLY when Excel opens with no workbook at all, or when the user explicitly asks for a fresh file.
- DO NOT use to "recover from a broken state" — there is no broken state. Every Excel command response now includes `Workbook '<name>' has sheets: [...] | Active: <sheet> | Open workbooks: <N>` so you can see exactly what's there.
  - Active sheet is `SheetN` where N>1? That's normal — Excel auto-names new sheets `Sheet2`, `Sheet3`, etc. It does NOT mean the workbook is corrupted.
  - Don't see the sheet you expect? Read the `has sheets: [...]` list before assuming anything. If your sheet is in that list, just EXCEL_SELECT_SHEET to it. If it isn't, EXCEL_NEW_SHEET to create it. Either way, NEW_WORKBOOK is wrong.
  - `Open workbooks: 2` (or higher) means a previous NEW_WORKBOOK has already orphaned a file. STOP creating new ones. If you intended to start fresh once, you already did — work with the active workbook.
  - Workbook name changed unexpectedly (e.g. was `Workbook1`, now `Workbook2`)? You opened or created another one — switch back via window/file menu rather than creating yet another.
- Example: EXCEL_NEW_WORKBOOK

**EXCEL_OPEN_WORKBOOK:<file_path>**
- Opens an existing workbook from a file path
- Example: EXCEL_OPEN_WORKBOOK:/Users/me/Documents/report.xlsx
- Example: EXCEL_OPEN_WORKBOOK:~/Desktop/financials.xlsx

────────────────────────────
## DOCUMENT OPERATIONS
────────────────────────────

**EXCEL_CLEAR_RANGE:<range>**
- Clears contents of range
- Example: EXCEL_CLEAR_RANGE:A2:Z100

**EXCEL_COPY_RANGE:<source>:<destination>**
- Copies a range to another location
- Example: EXCEL_COPY_RANGE:A1:B10:D1:E10

**EXCEL_SAVE**
- Saves the active workbook to the Linefox directory (~/Linefox/)
- Example: EXCEL_SAVE

**EXCEL_SAVE_AS:<filename>**
- Saves the workbook with a new filename to the Linefox directory (~/Linefox/)
- Provide ONLY a filename, NOT a full path — the system saves to ~/Linefox/ automatically
- Example: EXCEL_SAVE_AS:Financial_Model.xlsx

────────────────────────────
## DATA RETRIEVAL & INSPECTION (stores in MEMORY)
────────────────────────────

**EXCEL_GET_CELL_VALUE:<cell>**
- Gets the computed value of a cell
- Example: EXCEL_GET_CELL_VALUE:B5

**EXCEL_GET_RANGE_VALUES:<range>**
- Gets computed values from a range (comma-separated)
- Example: EXCEL_GET_RANGE_VALUES:A1:A10

**EXCEL_GET_FORMULA:<cell>**
- Gets the formula in a cell (returns value if no formula)
- Use this to check what formula is currently in a cell
- Example: EXCEL_GET_FORMULA:C10

**EXCEL_GET_RANGE_FORMULAS:<range>**
- Gets formulas/values from a range to inspect spreadsheet state
- Shows formulas where they exist, values otherwise
- Example: EXCEL_GET_RANGE_FORMULAS:E4:G10

**EXCEL_GET_SHEET_INFO**
- Gets overview of current sheet: used range, row/column count
- Helps understand the spreadsheet structure
- Example: EXCEL_GET_SHEET_INFO

**EXCEL_GET_STRUCTURE:<range>**
- Bulk-reads VALUES, FORMULAS, and FORMATTING for a range
- Returns cell addresses with values, formulas in [brackets], and formatting in {curly braces}
- Formatting shown: bold, fmt:<number format>, align:center/right
- NOTE: Font/fill colors are NOT shown (unreliable on macOS). Trust your SET_FILL_COLOR/SET_FONT_COLOR commands — they work correctly even though colors don't appear here.
- Only non-default formatting is shown (no tags = default font, left-aligned, General format)
- Output example:
  A1: Revenue {bold, align:center}
  B2: 12345 [=SUM(B3:B4)] {fmt:$#,##0.00}
  C2: 0.15 {fmt:0.0%}
- Example: EXCEL_GET_STRUCTURE:A1:P30 || Get structure of data area

### 📋 WORKFLOW FOR ENTERING DATA INTO EXISTING SPREADSHEET:
```
1. EXCEL_GET_SHEET_INFO                    → See range size
2. EXCEL_GET_STRUCTURE:A1:P50              → See cell addresses, values, formulas, formatting
3. Find row/column intersections from memory
4. EXCEL_TYPE:L8:85269                     → Enter data in CORRECT cell
```

## GETTING STARTED
When you first open a spreadsheet, check what's already there:
1. **EXCEL_GET_SHEET_INFO** — See if the sheet has existing data.
2. If it has data → **EXCEL_GET_STRUCTURE:A1:P30** to understand the layout before writing.
3. If you need a clean sheet → **EXCEL_NEW_SHEET:<name>** or **EXCEL_CLEAR_RANGE:A1:Z1000**.

After that, trust your commands — do NOT re-check the sheet state after every action.
Only use EXCEL_GET_STRUCTURE again if something went wrong or after completing a large batch of changes (20+ steps).

## 📋 PROFESSIONAL EXCEL GUIDELINES

### Spreadsheet Layout
- **Row 1**: Column headers (bold, frozen)
- **Column A**: Row labels or primary identifiers
- **Data starts Row 2**: Keep header row separate from data

### Financial Model Structure
```
Rows 1-X:    Headers + Assumptions (inputs)
Separator:   Blank row
Next rows:   Calculations
Separator:   Blank row
Final rows:  Outputs/Summary
```

### Date & Time Headers
- **Time flows LEFT→RIGHT**: Oldest on left, newest on right
- **Monthly**: Jan-24, Feb-24... OR 2024-01, 2024-02...
- **Quarterly**: Q1 2024, Q2 2024...
- **Annual**: 2023, 2024, 2025 as column headers

### Number Formatting
- **Currency**: Include thousand separators ($1,234.56)
- **Percentages**: Show as % (12.5% not 0.125)
- **Negatives**: Use parentheses (1,234) or red color
- **Decimals**: Consistent decimal places per column

### Professional Standards
- **Inputs/Assumptions**: Blue font (industry standard)
- **Formulas**: Black font (default)
- **Hard-coded values**: Flag for review
- **Headers**: Bold, possibly with background color

### Formula Best Practices
- **Use cell references**: Never hard-code assumptions in formulas lay them out separately
- **One calculation per row**: Show intermediate steps
- **Label clearly**: Row labels should explain the calculation

DO NOT SPEND MORE THAN 10-15 ACTIONS ON FORMATTING - FORMAT WELL ENOUGH AND MOVE ON

────────────────────────────
"#.to_string()
}

/// Parse a color string into AppleScript RGB format "{R, G, B}" (0-65535 scale).
/// Supports: named colors, RGB triplet "255,200,200", hex "#RRGGBB" or "#RGB".
/// Returns None if the string cannot be parsed.
fn parse_color_to_applescript_rgb(color_str: &str) -> Option<String> {
    let lower = color_str.to_lowercase();
    // Named colors
    match lower.as_str() {
        "yellow" => return Some("{65535, 65535, 0}".to_string()),
        "green" => return Some("{0, 65535, 0}".to_string()),
        "blue" => return Some("{0, 0, 65535}".to_string()),
        "red" => return Some("{65535, 0, 0}".to_string()),
        "orange" => return Some("{65535, 42405, 0}".to_string()),
        "gray" | "grey" => return Some("{32768, 32768, 32768}".to_string()),
        "lightgray" | "lightgrey" => return Some("{49152, 49152, 49152}".to_string()),
        "white" => return Some("{65535, 65535, 65535}".to_string()),
        "black" => return Some("{0, 0, 0}".to_string()),
        _ => {}
    }
    // Hex color: #RRGGBB or #RGB
    let trimmed = color_str.trim();
    if trimmed.starts_with('#') {
        let hex = &trimmed[1..];
        let (r8, g8, b8) = if hex.len() == 6 {
            let r = u32::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u32::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u32::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        } else if hex.len() == 3 {
            let r = u32::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u32::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u32::from_str_radix(&hex[2..3], 16).ok()? * 17;
            (r, g, b)
        } else {
            return None;
        };
        let r = (r8 * 257).min(65535);
        let g = (g8 * 257).min(65535);
        let b = (b8 * 257).min(65535);
        return Some(format!("{{{}, {}, {}}}", r, g, b));
    }
    // RGB triplet: "255,200,200"
    let parts: Vec<&str> = color_str.split(',').collect();
    if parts.len() == 3 {
        if let (Ok(r8), Ok(g8), Ok(b8)) = (
            parts[0].trim().parse::<u32>(),
            parts[1].trim().parse::<u32>(),
            parts[2].trim().parse::<u32>()
        ) {
            let r = (r8 * 257).min(65535);
            let g = (g8 * 257).min(65535);
            let b = (b8 * 257).min(65535);
            return Some(format!("{{{}, {}, {}}}", r, g, b));
        }
    }
    None
}

/// Execute an Excel AppleScript command
/// Returns Ok(result_message) on success, Err(error_message) on failure
pub async fn execute_excel_command(command: &str, params: &[&str]) -> Result<String, String> {
    let script = match command {
        "EXCEL_RENAME_SHEET" => {
            if params.len() != 2 {
                return Err("EXCEL_RENAME_SHEET requires old_name and new_name".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set name of worksheet "{}" of active workbook to "{}"
    return "✓ Renamed sheet '{}' to '{}'"
end tell
"#, params[0], params[1], params[0], params[1])
        },

        "EXCEL_NEW_SHEET" => {
            if params.is_empty() {
                return Err("EXCEL_NEW_SHEET requires a sheet name".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set newSheet to make new worksheet at end of active workbook
    set name of newSheet to "{}"
    return "✓ Created new sheet '{}'"
end tell
"#, params[0], params[0])
        },

        "EXCEL_DELETE_SHEET" => {
            if params.is_empty() {
                return Err("EXCEL_DELETE_SHEET requires a sheet name".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    delete worksheet "{}" of active workbook
    return "✓ Deleted sheet '{}'"
end tell
"#, params[0], params[0])
        },

        "EXCEL_SELECT_SHEET" => {
            if params.is_empty() {
                return Err("EXCEL_SELECT_SHEET requires a sheet name".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    activate object worksheet "{}" of active workbook
    return "✓ Selected sheet '{}'"
end tell
"#, params[0], params[0])
        },

        "EXCEL_SET_CELL" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_CELL requires cell reference and value".to_string());
            }
            let cell = params[0];
            let value = params[1].replace("\"", "\\\"");
            format!(r#"
tell application "Microsoft Excel"
    set value of range "{}" of active sheet to "{}"
    return "✓ Value set in {}: {}"
end tell
"#, cell, value, cell, value)
        },

        "EXCEL_SET_FORMULA" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_FORMULA requires cell reference and formula".to_string());
            }
            let cell = params[0];
            let formula = params[1];
            format!(r#"
tell application "Microsoft Excel"
    set formula of range "{}" of active sheet to "{}"
    return "✓ Formula set in {}: {}"
end tell
"#, cell, formula, cell, formula)
        },

        "EXCEL_FORMAT_CELLS" => {
            if params.len() != 2 {
                return Err("EXCEL_FORMAT_CELLS requires range and format type".to_string());
            }
            let range = params[0];
            let format_type = params[1].to_lowercase();
            let format = match format_type.as_str() {
                "currency" => "$#,##0.00",
                "currency0" => "$#,##0",
                "percent" => "0.00%",
                "percent0" => "0%",
                "percent1" => "0.0%",
                "number" => "#,##0.00",
                "number0" => "#,##0",
                "number1" => "#,##0.0",
                "date" => "yyyy-mm-dd",
                "text" => "@",
                "general" => "General",
                // Allow custom Excel format strings directly (e.g. "#,##0.000")
                custom => custom,
            };
            format!(r#"
tell application "Microsoft Excel"
    set number format of range "{}" of active sheet to "{}"
    return "✓ Formatted {} as {}"
end tell
"#, range, format, range, format_type)
        },

        "EXCEL_SET_FILL_COLOR" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_FILL_COLOR requires range and color".to_string());
            }
            let range = params[0];
            let rgb = parse_color_to_applescript_rgb(params[1])
                .unwrap_or_else(|| "{65535, 65535, 0}".to_string()); // default yellow
            format!(r#"
tell application "Microsoft Excel"
    set color of interior object of range "{}" of active sheet to {}
    return "✓ Fill color set for {}"
end tell
"#, range, rgb, range)
        },

        "EXCEL_SET_FONT_COLOR" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_FONT_COLOR requires range and color".to_string());
            }
            let range = params[0];
            let rgb = parse_color_to_applescript_rgb(params[1])
                .unwrap_or_else(|| "{0, 0, 0}".to_string()); // default black
            format!(r#"
tell application "Microsoft Excel"
    set color of font object of range "{}" of active sheet to {}
    return "✓ Font color set for {}"
end tell
"#, range, rgb, range)
        },

        "EXCEL_AUTOFIT_COLUMNS" => {
            if params.is_empty() {
                return Err("EXCEL_AUTOFIT_COLUMNS requires a column range".to_string());
            }
            // Handle both "A:H" format and "A1:H100" format
            let range = params[0];
            format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    autofit (entire column of theRange)
    return "✓ Columns autofitted for {}"
end tell
"#, range, range)
        },

        "EXCEL_SET_COLUMN_WIDTH" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_COLUMN_WIDTH requires column and width".to_string());
            }
            // Use range to get the column, works with "A" or "A:A" or "A1"
            let col = params[0];
            let width = params[1];
            format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}1" of active sheet
    set column width of (entire column of theRange) to {}
    return "✓ Column {} width set to {}"
end tell
"#, col, width, col, width)
        },

        "EXCEL_BOLD" => {
            if params.is_empty() {
                return Err("EXCEL_BOLD requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set bold of font object of range "{}" of active sheet to true
    return "✓ Bold applied to {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_ITALIC" => {
            if params.is_empty() {
                return Err("EXCEL_ITALIC requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set italic of font object of range "{}" of active sheet to true
    return "✓ Italic applied to {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_UNDERLINE" => {
            if params.is_empty() {
                return Err("EXCEL_UNDERLINE requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set underline of font object of range "{}" of active sheet to underline style single
    return "✓ Underline applied to {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_SET_FONT" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_FONT requires range and font name".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set name of font object of range "{}" of active sheet to "{}"
    return "✓ Font set to {} for {}"
end tell
"#, params[0], params[1], params[1], params[0])
        },

        "EXCEL_SET_FONT_SIZE" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_FONT_SIZE requires range and size".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set font size of font object of range "{}" of active sheet to {}
    return "✓ Font size set to {} for {}"
end tell
"#, params[0], params[1], params[1], params[0])
        },

        "EXCEL_WRAP_TEXT" => {
            if params.is_empty() {
                return Err("EXCEL_WRAP_TEXT requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set wrap text of range "{}" of active sheet to true
    return "✓ Wrap text enabled for {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_MERGE_CELLS" => {
            if params.is_empty() {
                return Err("EXCEL_MERGE_CELLS requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    merge range "{}" of active sheet
    return "✓ Merged cells {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_UNMERGE_CELLS" => {
            if params.is_empty() {
                return Err("EXCEL_UNMERGE_CELLS requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    unmerge range "{}" of active sheet
    return "✓ Unmerged cells {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_ALIGN" => {
            if params.len() != 2 {
                return Err("EXCEL_ALIGN requires range and alignment".to_string());
            }
            let align_const = match params[1].to_lowercase().as_str() {
                "left" => "horizontal align left",
                "center" | "centre" => "horizontal align center",
                "right" => "horizontal align right",
                _ => "horizontal align center",
            };
            format!(r#"
tell application "Microsoft Excel"
    set horizontal alignment of range "{}" of active sheet to {}
    return "✓ Alignment set to {} for {}"
end tell
"#, params[0], align_const, params[1], params[0])
        },

        "EXCEL_VERTICAL_ALIGN" => {
            if params.len() != 2 {
                return Err("EXCEL_VERTICAL_ALIGN requires range and alignment".to_string());
            }
            let align_const = match params[1].to_lowercase().as_str() {
                "top" => "vertical align top",
                "center" | "centre" | "middle" => "vertical align center",
                "bottom" => "vertical align bottom",
                _ => "vertical align center",
            };
            format!(r#"
tell application "Microsoft Excel"
    set vertical alignment of range "{}" of active sheet to {}
    return "✓ Vertical alignment set to {} for {}"
end tell
"#, params[0], align_const, params[1], params[0])
        },

        "EXCEL_SET_ROW_HEIGHT" => {
            if params.len() != 2 {
                return Err("EXCEL_SET_ROW_HEIGHT requires row and height".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set row height of row {} of active sheet to {}
    return "✓ Row {} height set to {}"
end tell
"#, params[0], params[1], params[0], params[1])
        },

        "EXCEL_ADD_BORDER" => {
            if params.len() != 2 {
                return Err("EXCEL_ADD_BORDER requires range and style".to_string());
            }
            let range = params[0];
            let style = params[1].to_lowercase();
            // Border styles: all, outline, top, bottom, left, right, none
            // Using correct Excel AppleScript border syntax
            let script = match style.as_str() {
                "all" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    -- Outline borders
    set myBorders to {{border top, border bottom, border left, border right}}
    repeat with i from 1 to count of myBorders
        set theBorder to get border theRange which border (item i of myBorders)
        set weight of theBorder to border weight thin
    end repeat
    -- Inside borders (accessed separately)
    try
        set theBorder to get border theRange which border border inside vertical
        set weight of theBorder to border weight thin
    end try
    try
        set theBorder to get border theRange which border border inside horizontal
        set weight of theBorder to border weight thin
    end try
    return "✓ All borders added to {}"
end tell
"#, range, range),
                "outline" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set myBorders to {{border top, border bottom, border left, border right}}
    repeat with i from 1 to count of myBorders
        set theBorder to get border theRange which border (item i of myBorders)
        set weight of theBorder to border weight thin
        set line style of theBorder to line style single
    end repeat
    return "✓ Outline border added to {}"
end tell
"#, range, range),
                "top" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set theBorder to get border theRange which border border top
    set weight of theBorder to border weight thin
    set line style of theBorder to line style single
    return "✓ Top border added to {}"
end tell
"#, range, range),
                "bottom" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set theBorder to get border theRange which border border bottom
    set weight of theBorder to border weight thin
    set line style of theBorder to line style single
    return "✓ Bottom border added to {}"
end tell
"#, range, range),
                "left" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set theBorder to get border theRange which border border left
    set weight of theBorder to border weight thin
    set line style of theBorder to line style single
    return "✓ Left border added to {}"
end tell
"#, range, range),
                "right" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set theBorder to get border theRange which border border right
    set weight of theBorder to border weight thin
    set line style of theBorder to line style single
    return "✓ Right border added to {}"
end tell
"#, range, range),
                "none" => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set myBorders to {{border top, border bottom, border left, border right}}
    repeat with i from 1 to count of myBorders
        set theBorder to get border theRange which border (item i of myBorders)
        set line style of theBorder to line style none
    end repeat
    try
        set theBorder to get border theRange which border border inside vertical
        set line style of theBorder to line style none
    end try
    try
        set theBorder to get border theRange which border border inside horizontal
        set line style of theBorder to line style none
    end try
    return "✓ Borders removed from {}"
end tell
"#, range, range),
                _ => format!(r#"
tell application "Microsoft Excel"
    set theRange to range "{}" of active sheet
    set myBorders to {{border top, border bottom, border left, border right}}
    repeat with i from 1 to count of myBorders
        set theBorder to get border theRange which border (item i of myBorders)
        set weight of theBorder to border weight thin
    end repeat
    try
        set theBorder to get border theRange which border border inside vertical
        set weight of theBorder to border weight thin
    end try
    try
        set theBorder to get border theRange which border border inside horizontal
        set weight of theBorder to border weight thin
    end try
    return "✓ All borders added to {}"
end tell
"#, range, range),
            };
            script
        },

        "EXCEL_CLEAR_RANGE" => {
            if params.is_empty() {
                return Err("EXCEL_CLEAR_RANGE requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    clear contents range "{}" of active sheet
    return "✓ Cleared range {}"
end tell
"#, params[0], params[0])
        },

        "EXCEL_COPY_RANGE" => {
            if params.len() != 2 {
                return Err("EXCEL_COPY_RANGE requires source and destination ranges".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    copy range "{}" of active sheet
    select range "{}" of active sheet
    paste special active sheet
    return "✓ Copied {} to {}"
end tell
"#, params[0], params[1], params[0], params[1])
        },

        "EXCEL_GET_CELL_VALUE" => {
            if params.is_empty() {
                return Err("EXCEL_GET_CELL_VALUE requires a cell reference".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    get value of range "{}" of active sheet
end tell
"#, params[0])
        },

        "EXCEL_GET_RANGE_VALUES" => {
            if params.is_empty() {
                return Err("EXCEL_GET_RANGE_VALUES requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set theValues to value of range "{}" of active sheet
    set AppleScript's text item delimiters to ","
    if class of theValues is list then
        set flatList to {{}}
        repeat with rowData in theValues
            if class of rowData is list then
                repeat with cellVal in rowData
                    set end of flatList to (cellVal as text)
                end repeat
            else
                set end of flatList to (rowData as text)
            end if
        end repeat
        return flatList as text
    else
        return theValues as text
    end if
end tell
"#, params[0])
        },

        "EXCEL_GET_FORMULA" => {
            if params.is_empty() {
                return Err("EXCEL_GET_FORMULA requires a cell reference".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    get formula of range "{}" of active sheet
end tell
"#, params[0])
        },

        "EXCEL_GET_RANGE_FORMULAS" => {
            if params.is_empty() {
                return Err("EXCEL_GET_RANGE_FORMULAS requires a range".to_string());
            }
            format!(r#"
tell application "Microsoft Excel"
    set theFormulas to formula of range "{}" of active sheet
    set AppleScript's text item delimiters to ","
    if class of theFormulas is list then
        set flatList to {{}}
        repeat with rowData in theFormulas
            if class of rowData is list then
                repeat with cellFormula in rowData
                    set end of flatList to (cellFormula as text)
                end repeat
            else
                set end of flatList to (rowData as text)
            end if
        end repeat
        return flatList as text
    else
        return theFormulas as text
    end if
end tell
"#, params[0])
        },

        "EXCEL_GET_SHEET_INFO" => {
            r#"
tell application "Microsoft Excel"
    set theRange to used range of active sheet
    set startRow to first row index of theRange
    set startCol to first column index of theRange
    set numRows to count of rows of theRange
    set numCols to count of columns of theRange
    set sheetName to name of active sheet
    return "Sheet: " & sheetName & ", Used range: Row " & startRow & "-" & (startRow + numRows - 1) & ", Col " & startCol & "-" & (startCol + numCols - 1) & " (" & numRows & " rows x " & numCols & " cols)"
end tell
"#.to_string()
        },

        // Bulk read structure: values + formulas for a range
        "EXCEL_GET_STRUCTURE" => {
            if params.is_empty() {
                return Err("EXCEL_GET_STRUCTURE requires a range (e.g., A1:P30)".to_string());
            }
            let range = params[0];
            // Read values, formulas, and formatting for non-empty cells
            // Uses r##""## to allow # characters inside the AppleScript (for hex colors)
            format!(r##"
tell application "Microsoft Excel"
    set ws to active sheet
    set theRange to range "{0}" of ws
    set startRow to first row index of theRange
    set startCol to first column index of theRange
    set rCount to count of rows of theRange
    set cCount to count of columns of theRange
    set colLetters to "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    set hashChar to "#"

    set output to "STRUCTURE of {0} (" & rCount & " rows x " & cCount & " cols):" & return

    repeat with r from startRow to (startRow + rCount - 1)
        repeat with c from startCol to (startCol + cCount - 1)
            if c ≤ 26 then
                set colLetter to character c of colLetters
            else
                set colLetter to "A" & character (c - 26) of colLetters
            end if
            set cellRef to colLetter & r
            try
                set theCell to range cellRef of ws
                set cellVal to value of theCell
                set cellFormula to formula of theCell
                if cellVal is not missing value then
                    set valText to cellVal as text
                    if valText is not "" then
                        -- Build base: value + formula
                        if cellFormula starts with "=" then
                            set cellLine to cellRef & ": " & valText & " [" & cellFormula & "]"
                        else
                            set cellLine to cellRef & ": " & valText
                        end if

                        -- Collect formatting tags
                        set fmtTags to {{}}

                        try
                            if bold of font object of theCell then
                                set end of fmtTags to "bold"
                            end if
                        end try

                        try
                            set nf to number format of theCell
                            if nf is not "General" and nf is not "@" then
                                set end of fmtTags to "fmt:" & nf
                            end if
                        end try

                        -- NOTE: font/fill colors omitted — AppleScript returns unreliable values
                        -- for theme colors (reads as #010101 regardless of actual color).
                        -- The SET commands work fine; only the READ is broken.

                        try
                            set ha to horizontal alignment of theCell
                            if ha is horizontal align center then
                                set end of fmtTags to "align:center"
                            else if ha is horizontal align right then
                                set end of fmtTags to "align:right"
                            end if
                        end try

                        -- Append formatting tags if any
                        if (count of fmtTags) > 0 then
                            set AppleScript's text item delimiters to ", "
                            set cellLine to cellLine & " {{" & (fmtTags as text) & "}}"
                            set AppleScript's text item delimiters to ""
                        end if

                        set output to output & cellLine & return
                    end if
                end if
            end try
        end repeat
    end repeat

    return output
end tell
"##, range)
        },

        "EXCEL_FREEZE_PANES" => {
            if params.is_empty() {
                return Err("EXCEL_FREEZE_PANES requires a cell reference (e.g. B2 to freeze row 1 and column A)".to_string());
            }
            let cell = params[0];
            format!(r#"
tell application "Microsoft Excel"
    select range "{}" of active sheet
    set freeze panes of active window to false
    set freeze panes of active window to true
    return "✓ Panes frozen at {}"
end tell
"#, cell, cell)
        },

        "EXCEL_UNFREEZE_PANES" => {
            r#"
tell application "Microsoft Excel"
    set freeze panes of active window to false
    return "✓ Panes unfrozen"
end tell
"#.to_string()
        },

        "EXCEL_SAVE" => {
            // Smart save: always save to ~/Linefox/<name>.xlsx
            // Avoids Excel's sandboxed container path issue
            let linefox_dir = dirs::home_dir()
                .map(|h| h.join("Linefox").to_string_lossy().to_string())
                .unwrap_or_else(|| "~/Linefox".to_string());
            format!(r#"
tell application "Microsoft Excel"
    set wbName to name of active workbook
    -- Strip .xlsx if present, we'll add it back
    if wbName ends with ".xlsx" then
        set wbName to text 1 thru -6 of wbName
    end if
    set savePath to "{}" & "/" & wbName & ".xlsx"
    save active workbook in POSIX file savePath
    return "✓ Workbook saved as: " & savePath
end tell
"#, linefox_dir)
        },

        "EXCEL_SAVE_AS" => {
            if params.is_empty() {
                return Err("EXCEL_SAVE_AS requires a file path".to_string());
            }
            // Extract just the filename from whatever path the LLM provides,
            // then save to ~/Linefox/<filename> so files always go to the right place
            let raw_path = params[0];
            let filename = std::path::Path::new(raw_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(raw_path);
            // Ensure .xlsx extension
            let filename = if !filename.to_lowercase().ends_with(".xlsx") {
                format!("{}.xlsx", filename)
            } else {
                filename.to_string()
            };
            let linefox_dir = dirs::home_dir()
                .map(|h| h.join("Linefox").to_string_lossy().to_string())
                .unwrap_or_else(|| "~/Linefox".to_string());
            let save_path = format!("{}/{}", linefox_dir, filename);
            format!(r#"
tell application "Microsoft Excel"
    set posixFile to POSIX file "{}"
    save active workbook in posixFile
    return "✓ Workbook saved as: {}"
end tell
"#, save_path, save_path)
        },

        "EXCEL_NEW_WORKBOOK" => {
            r#"
tell application "Microsoft Excel"
    make new workbook
    return "✓ New workbook created"
end tell
"#.to_string()
        },

        "EXCEL_OPEN_WORKBOOK" => {
            if params.is_empty() {
                return Err("EXCEL_OPEN_WORKBOOK requires a file path".to_string());
            }
            let file_path = params[0];
            // Expand ~ to home directory in AppleScript
            format!(r#"
tell application "Microsoft Excel"
    set filePath to "{}"
    if filePath starts with "~" then
        set filePath to (POSIX path of (path to home folder)) & text 2 thru -1 of filePath
    end if
    open filePath
    return "✓ Opened workbook: " & filePath
end tell
"#, file_path)
        },

        _ => {
            return Err(format!("Unknown Excel command: {}", command));
        }
    };

    info!("Executing Excel AppleScript command: {}", command);

    // Execute the AppleScript
    match crate::window_details_collector::macos::macos_acting_engine::execute_applescript(&script).await {
        Ok(output) => {
            info!("Excel command {} executed successfully: {}", command, output);
            // Append the workbook structure so the agent has full state
            // visibility. Without this, "active sheet name doesn't match my
            // mental model" → "must be broken" → NEW_WORKBOOK panic loops.
            // Best-effort: if the summary fetch fails (rare), return original.
            match get_workbook_summary().await {
                Some(summary) => Ok(format!("{}\n{}", output, summary)),
                None => Ok(output),
            }
        },
        Err(e) => {
            error!("Excel command {} failed: {}", command, e);
            Err(format!("Excel command failed: {}", e))
        }
    }
}

/// Returns a one-line summary of the active workbook so the agent never
/// has to guess which file/sheets exist. Format:
///   "Workbook 'ServiceNow_Model.xlsx' has sheets: [Sheet1, Overview] | Active: Overview | Open workbooks: 1"
/// The workbook name lets the agent notice when NEW_WORKBOOK has orphaned
/// its previous work (workbook name changed; "Open workbooks" count > 1).
/// Returns None on any AppleScript failure (e.g. no workbook open) — caller
/// should treat that as "no extra info" rather than an error.
pub async fn get_workbook_summary() -> Option<String> {
    let script = r#"
tell application "Microsoft Excel"
    try
        set sheetNames to {}
        repeat with s in sheets of active workbook
            set end of sheetNames to (name of s as string)
        end repeat
        set AppleScript's text item delimiters to ", "
        set sheetsStr to sheetNames as text
        set AppleScript's text item delimiters to ""
        set activeName to (name of active sheet) as string
        set wbName to (name of active workbook) as string
        set wbCount to (count of workbooks) as string
        return "Workbook '" & wbName & "' has sheets: [" & sheetsStr & "] | Active: " & activeName & " | Open workbooks: " & wbCount
    on error
        return ""
    end try
end tell
"#;
    match crate::window_details_collector::macos::macos_acting_engine::execute_applescript(script).await {
        Ok(s) if !s.trim().is_empty() => Some(s),
        _ => None,
    }
}

/// Execute an Excel command from action parameters HashMap
/// This is the main entry point called from automation_agent_engine
/// Returns Ok(result_message) on success, Err(error_message) on failure
///
/// Supports ||| chaining for formatting commands: if the raw_input contains |||,
/// it is split into individual operations and each is executed sequentially.
pub async fn execute_excel_command_from_params(params: &std::collections::HashMap<String, String>) -> Result<String, String> {
    let command = params.get("command").ok_or("Missing command parameter")?;

    // Check for ||| chaining in raw_input (LLM sends e.g. "EXCEL_BOLD:A1:D1|||A5:D5|||A8:D8")
    // Split and execute each sub-command individually for formatting commands.
    if let Some(raw) = params.get("raw_input") {
        if raw.contains("|||") {
            return execute_chained_excel_commands(command, raw).await;
        }
    }

    let param_vec: Vec<&str> = build_param_vec(command, params)?;
    execute_excel_command(command, &param_vec).await
}

/// Handle ||| chained Excel commands by splitting and executing each one.
/// Supports both same-command chaining (EXCEL_BOLD:A1:D1|||A5:D5) and
/// mixed-command chaining (EXCEL_RENAME_SHEET:Sheet1:Cover|||EXCEL_NEW_SHEET:Summary).
async fn execute_chained_excel_commands(command: &str, raw_input: &str) -> Result<String, String> {
    // Strip the initial command prefix so we get the full param string
    let prefix = format!("{}:", command);
    let params_part = if raw_input.to_uppercase().starts_with(&prefix.to_uppercase()) {
        &raw_input[prefix.len()..]
    } else {
        raw_input
    };

    let parts: Vec<&str> = params_part.split("|||").collect();
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part.is_empty() { continue; }

        // Detect if this part starts with a different EXCEL_ command
        let (effective_cmd, effective_params) = if part.to_uppercase().starts_with("EXCEL_") {
            // Mixed chaining: part has its own command prefix, e.g. "EXCEL_NEW_SHEET:Summary"
            if let Some(colon_pos) = part.find(':') {
                let cmd = &part[..colon_pos];
                let params = &part[colon_pos + 1..];
                (cmd.to_uppercase(), params.to_string())
            } else {
                // Command with no params (e.g. EXCEL_GET_SHEET_INFO)
                (part.to_uppercase(), String::new())
            }
        } else {
            // Same-command chaining: part is just params for the original command
            (command.to_string(), part.to_string())
        };

        let sub_params = parse_sub_command_params(&effective_cmd, &effective_params)?;
        let param_vec: Vec<&str> = sub_params.iter().map(|s| s.as_str()).collect();

        match execute_excel_command(&effective_cmd, &param_vec).await {
            Ok(result) => results.push(result),
            Err(e) => errors.push(format!("Part {}: {}", i + 1, e)),
        }
    }

    if !errors.is_empty() {
        if results.is_empty() {
            return Err(format!("All {} operations failed: {}", errors.len(), errors.join("; ")));
        }
        return Ok(format!("{} succeeded, {} failed: {}", results.len(), errors.len(), results.join(" | ")));
    }

    Ok(results.join(" | "))
}

/// Parse parameters for a single sub-command from a ||| chain.
/// E.g. for EXCEL_FORMAT_CELLS, "B2:B10:currency" -> ["B2:B10", "currency"]
/// E.g. for EXCEL_BOLD, "A1:D1" -> ["A1:D1"]
/// E.g. for EXCEL_RENAME_SHEET, "Sheet1:Cover" -> ["Sheet1", "Cover"]
fn parse_sub_command_params(command: &str, raw_part: &str) -> Result<Vec<String>, String> {
    match command.to_uppercase().as_str() {
        "EXCEL_RENAME_SHEET" => {
            let parts: Vec<&str> = raw_part.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(format!("EXCEL_RENAME_SHEET requires old_name:new_name, got: {}", raw_part));
            }
            Ok(vec![parts[0].trim().to_string(), parts[1].trim().to_string()])
        },
        "EXCEL_NEW_SHEET" | "EXCEL_DELETE_SHEET" | "EXCEL_SELECT_SHEET" | "EXCEL_OPEN_WORKBOOK" => {
            if raw_part.is_empty() {
                return Err(format!("{} requires a parameter", command));
            }
            Ok(vec![raw_part.trim().to_string()])
        },
        "EXCEL_SET_CELL" | "EXCEL_SET_FORMULA" => {
            let parts: Vec<&str> = raw_part.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(format!("{} requires cell:value, got: {}", command, raw_part));
            }
            Ok(vec![parts[0].trim().to_string(), parts[1].trim().to_string()])
        },
        "EXCEL_FORMAT_CELLS" | "EXCEL_SET_FILL_COLOR" | "EXCEL_SET_FONT_COLOR" |
        "EXCEL_ADD_BORDER" | "EXCEL_SET_FONT" | "EXCEL_SET_FONT_SIZE" |
        "EXCEL_ALIGN" | "EXCEL_VERTICAL_ALIGN" => {
            // range:value — split from end since range contains ':'
            let parts: Vec<&str> = raw_part.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(format!("{} sub-part requires range:value, got: {}", command, raw_part));
            }
            Ok(vec![parts[1].to_string(), parts[0].to_string()])
        },
        "EXCEL_SET_COLUMN_WIDTH" | "EXCEL_COPY_RANGE" | "EXCEL_SET_ROW_HEIGHT" => {
            let parts: Vec<&str> = raw_part.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(format!("{} requires two parameters, got: {}", command, raw_part));
            }
            Ok(vec![parts[0].trim().to_string(), parts[1].trim().to_string()])
        },
        // Single-range commands: EXCEL_BOLD, EXCEL_ITALIC, EXCEL_UNDERLINE,
        // EXCEL_WRAP_TEXT, EXCEL_MERGE_CELLS, EXCEL_UNMERGE_CELLS, EXCEL_AUTOFIT_COLUMNS, etc.
        _ => Ok(vec![raw_part.to_string()]),
    }
}

fn build_param_vec<'a>(command: &str, params: &'a std::collections::HashMap<String, String>) -> Result<Vec<&'a str>, String> {
    match command {
        "EXCEL_RENAME_SHEET" => {
            let old_name = params.get("old_name").ok_or("Missing old_name")?;
            let new_name = params.get("new_name").ok_or("Missing new_name")?;
            Ok(vec![old_name.as_str(), new_name.as_str()])
        },
        "EXCEL_NEW_SHEET" | "EXCEL_DELETE_SHEET" | "EXCEL_SELECT_SHEET" => {
            let sheet_name = params.get("sheet_name").ok_or("Missing sheet_name")?;
            Ok(vec![sheet_name.as_str()])
        },
        "EXCEL_SET_CELL" | "EXCEL_SET_FORMULA" => {
            let cell = params.get("cell").ok_or("Missing cell")?;
            let value = params.get("value").ok_or("Missing value")?;
            Ok(vec![cell.as_str(), value.as_str()])
        },
        "EXCEL_FORMAT_CELLS" => {
            let range = params.get("range").ok_or("Missing range")?;
            let format = params.get("format").ok_or("Missing format")?;
            Ok(vec![range.as_str(), format.as_str()])
        },
        "EXCEL_AUTOFIT_COLUMNS" | "EXCEL_BOLD" | "EXCEL_ITALIC" | "EXCEL_UNDERLINE" |
        "EXCEL_WRAP_TEXT" | "EXCEL_MERGE_CELLS" | "EXCEL_UNMERGE_CELLS" |
        "EXCEL_CLEAR_RANGE" |
        "EXCEL_GET_CELL_VALUE" | "EXCEL_GET_RANGE_VALUES" |
        "EXCEL_GET_FORMULA" | "EXCEL_GET_RANGE_FORMULAS" | "EXCEL_GET_STRUCTURE" |
        "EXCEL_FREEZE_PANES" => {
            let range = params.get("range").ok_or("Missing range")?;
            Ok(vec![range.as_str()])
        },
        "EXCEL_GET_SHEET_INFO" | "EXCEL_UNFREEZE_PANES" | "EXCEL_SAVE" | "EXCEL_NEW_WORKBOOK" => Ok(vec![]),
        "EXCEL_OPEN_WORKBOOK" | "EXCEL_SAVE_AS" => {
            let path = params.get("path").ok_or("Missing path")?;
            Ok(vec![path.as_str()])
        },
        "EXCEL_SET_FILL_COLOR" | "EXCEL_SET_FONT_COLOR" => {
            let range = params.get("range").ok_or("Missing range")?;
            let color = params.get("color").ok_or("Missing color")?;
            Ok(vec![range.as_str(), color.as_str()])
        },
        "EXCEL_ADD_BORDER" => {
            let range = params.get("range").ok_or("Missing range")?;
            let style = params.get("style").ok_or("Missing style")?;
            Ok(vec![range.as_str(), style.as_str()])
        },
        "EXCEL_SET_COLUMN_WIDTH" => {
            let column = params.get("column").ok_or("Missing column")?;
            let width = params.get("width").ok_or("Missing width")?;
            Ok(vec![column.as_str(), width.as_str()])
        },
        "EXCEL_COPY_RANGE" => {
            let source = params.get("source").ok_or("Missing source")?;
            let destination = params.get("destination").ok_or("Missing destination")?;
            Ok(vec![source.as_str(), destination.as_str()])
        },
        "EXCEL_SET_FONT" => {
            let range = params.get("range").ok_or("Missing range")?;
            let font = params.get("font").ok_or("Missing font")?;
            Ok(vec![range.as_str(), font.as_str()])
        },
        "EXCEL_SET_FONT_SIZE" => {
            let range = params.get("range").ok_or("Missing range")?;
            let size = params.get("size").ok_or("Missing size")?;
            Ok(vec![range.as_str(), size.as_str()])
        },
        "EXCEL_ALIGN" | "EXCEL_VERTICAL_ALIGN" => {
            let range = params.get("range").ok_or("Missing range")?;
            let align = params.get("align").ok_or("Missing align")?;
            Ok(vec![range.as_str(), align.as_str()])
        },
        "EXCEL_SET_ROW_HEIGHT" => {
            let row = params.get("row").ok_or("Missing row")?;
            let height = params.get("height").ok_or("Missing height")?;
            Ok(vec![row.as_str(), height.as_str()])
        },
        _ => Err(format!("Unknown Excel command: {}", command))
    }
}

/// Check if the command is a GET command that returns data
pub fn is_data_retrieval_command(command: &str) -> bool {
    matches!(command,
        "EXCEL_GET_CELL_VALUE" | "EXCEL_GET_RANGE_VALUES" |
        "EXCEL_GET_FORMULA" | "EXCEL_GET_RANGE_FORMULAS" | "EXCEL_GET_SHEET_INFO" |
        "EXCEL_GET_STRUCTURE"
    )
}
