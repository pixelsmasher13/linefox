//! Excel-specific AppleScript commands for macOS
//!
//! These commands provide direct Excel automation via AppleScript,
//! which is more reliable than clicking UI elements.

use log::{info, error};

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

**EXCEL_TYPE - THE PREFERRED COMMAND for ALL cell data (values AND formulas)**
- EXCEL_TYPE:<cell>:<value>[:::<cell>:<value>...]
- ✅ Works for text, numbers, AND formulas (starting with =)
- ✅ Supports BULK operations - set many cells in ONE command
- Example values: EXCEL_TYPE:A1:Revenue:::B1:2024:::C1:2025
- Example formulas: EXCEL_TYPE:C10:=SUM(C2:C9):::D10:=SUM(D2:D9):::E10:=SUM(E2:E9)
- Example mixed: EXCEL_TYPE:A1:Total:::B1:=SUM(B2:B9):::C1:=AVERAGE(C2:C9)

**EXCEL_SET_FORMULA - Only for single formula (rarely needed)**
- EXCEL_SET_FORMULA:<cell>:<formula>
- Use EXCEL_TYPE instead when setting multiple formulas
- Example: EXCEL_SET_FORMULA:C10:=SUM(C2:C9)

**EXCEL_SET_CELL - Only for single value (rarely needed)**
- EXCEL_SET_CELL:<cell>:<value>
- Use EXCEL_TYPE instead when setting multiple values
- Example: EXCEL_SET_CELL:A1:Total

────────────────────────────
## SHEET MANAGEMENT
────────────────────────────

**EXCEL_SELECT_SHEET:<name>**
- Switches to/activates the specified worksheet
- Example: EXCEL_SELECT_SHEET:Summary

**EXCEL_RENAME_SHEET:<old_name>:<new_name>**
- Renames a worksheet
- Example: EXCEL_RENAME_SHEET:Sheet1:Sales Data

**EXCEL_NEW_SHEET:<name>**
- Creates a new worksheet
- Example: EXCEL_NEW_SHEET:Q4 Report

**EXCEL_DELETE_SHEET:<name>**
- Deletes the specified worksheet
- Example: EXCEL_DELETE_SHEET:Sheet2

────────────────────────────
## FORMATTING
────────────────────────────

**EXCEL_FORMAT_CELLS:<range>:<format>**
- Formats: currency, percent, number, date, text, general
- Example: EXCEL_FORMAT_CELLS:B2:B100:currency

**EXCEL_BOLD:<range>**
- Makes the specified range bold
- Example: EXCEL_BOLD:A1:D1

**EXCEL_SET_FILL_COLOR:<range>:<color>**
- Sets background/fill color of cells
- Colors: yellow, green, blue, red, orange, gray, lightgray, white, or RGB like 255,200,200
- Example: EXCEL_SET_FILL_COLOR:A1:G1:yellow || Highlighting header row
- Example: EXCEL_SET_FILL_COLOR:A2:G2:lightgray || Alternating row color
- Example: EXCEL_SET_FILL_COLOR:B5:B10:255,230,230 || Custom RGB color

**EXCEL_SET_FONT_COLOR:<range>:<color>**
- Sets text/font color of cells
- Colors: same as fill colors (yellow, green, blue, red, etc. or RGB)
- Example: EXCEL_SET_FONT_COLOR:A1:G1:white || White text on dark background

**EXCEL_ADD_BORDER:<range>:<style>**
- Adds borders to cells
- Styles: all, outline, top, bottom, left, right, none
- Example: EXCEL_ADD_BORDER:A1:D10:all || Add all borders (grid)
- Example: EXCEL_ADD_BORDER:A1:D1:bottom || Add bottom border to header row
- Example: EXCEL_ADD_BORDER:A1:D10:outline || Add outline only (no inner borders)

**EXCEL_AUTOFIT_COLUMNS:<range>**
- Auto-fits column widths
- Example: EXCEL_AUTOFIT_COLUMNS:A:D

**EXCEL_SET_COLUMN_WIDTH:<column>:<width>**
- Sets a specific column width
- Example: EXCEL_SET_COLUMN_WIDTH:A:25

────────────────────────────
## DOCUMENT OPERATIONS
────────────────────────────

**EXCEL_CLEAR_RANGE:<range>**
- Clears contents of range
- Example: EXCEL_CLEAR_RANGE:A2:Z100

**EXCEL_COPY_RANGE:<source>:<destination>**
- Copies a range to another location
- Example: EXCEL_COPY_RANGE:A1:B10:D1:E10

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

**EXCEL_GET_STRUCTURE:<range>** ⭐ USE THIS TO UNDERSTAND SPREADSHEET LAYOUT
- Bulk-reads VALUES and FORMULAS for a range
- Returns cell addresses with values, and shows [formula] when cells have formulas
- ⚠️ USE THIS BEFORE entering data into existing spreadsheets!
- Shows: "A1: APPLE INC.", "B5: 12345 [=SUM(B2:B4)]", etc.
- Example: EXCEL_GET_STRUCTURE:A1:P30 || Get structure of data area

### 📋 WORKFLOW FOR ENTERING DATA INTO EXISTING SPREADSHEET:
```
1. EXCEL_GET_SHEET_INFO                    → See range size
2. EXCEL_GET_STRUCTURE:A1:P50              → See cell addresses, values, formulas
3. Find row/column intersections from memory
4. EXCEL_TYPE:L8:85269                     → Enter data in CORRECT cell
```

## ⚠️ MANDATORY FIRST STEP — CHECK SPREADSHEET STATE
Before entering ANY data into Excel, you MUST run these commands FIRST:
1. **EXCEL_GET_SHEET_INFO** — Determine if the sheet has existing data or is blank.
2. If the sheet has data → **EXCEL_GET_STRUCTURE:A1:P30** — Inspect layout before writing anything.
3. If the task requires a clean sheet → **EXCEL_NEW_SHEET:<name>** to create a fresh sheet, OR **EXCEL_CLEAR_RANGE:A1:Z1000** to clear existing content.
⚠️ NEVER start typing data without checking first — you may overwrite important existing content!

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
                "percent" => "0.00%",
                "number" => "#,##0.00",
                "date" => "yyyy-mm-dd",
                "text" => "@",
                "general" | _ => "General",
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
            let color = params[1].to_lowercase();
            // Convert color name or RGB to Excel RGB values (0-65535 scale)
            let rgb = match color.as_str() {
                "yellow" => "{65535, 65535, 0}",
                "green" => "{0, 65535, 0}",
                "blue" => "{0, 0, 65535}",
                "red" => "{65535, 0, 0}",
                "orange" => "{65535, 42405, 0}",
                "gray" | "grey" => "{32768, 32768, 32768}",
                "lightgray" | "lightgrey" => "{49152, 49152, 49152}",
                "white" => "{65535, 65535, 65535}",
                "black" => "{0, 0, 0}",
                _ => {
                    // Try to parse RGB like "255,200,200"
                    let parts: Vec<&str> = params[1].split(',').collect();
                    if parts.len() == 3 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            parts[0].trim().parse::<u32>(),
                            parts[1].trim().parse::<u32>(),
                            parts[2].trim().parse::<u32>()
                        ) {
                            // Convert 0-255 to 0-65535
                            let r = (r * 257).min(65535);
                            let g = (g * 257).min(65535);
                            let b = (b * 257).min(65535);
                            return Ok(format!(r#"tell application "Microsoft Excel"
    set color of interior object of range "{}" of active sheet to {{{}, {}, {}}}
    return "✓ Fill color set for {}"
end tell"#, range, r, g, b, range));
                        }
                    }
                    "{65535, 65535, 0}" // Default to yellow
                }
            };
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
            let color = params[1].to_lowercase();
            // Convert color name or RGB to Excel RGB values (0-65535 scale)
            let rgb = match color.as_str() {
                "yellow" => "{65535, 65535, 0}",
                "green" => "{0, 65535, 0}",
                "blue" => "{0, 0, 65535}",
                "red" => "{65535, 0, 0}",
                "orange" => "{65535, 42405, 0}",
                "gray" | "grey" => "{32768, 32768, 32768}",
                "lightgray" | "lightgrey" => "{49152, 49152, 49152}",
                "white" => "{65535, 65535, 65535}",
                "black" => "{0, 0, 0}",
                _ => {
                    // Try to parse RGB like "255,200,200"
                    let parts: Vec<&str> = params[1].split(',').collect();
                    if parts.len() == 3 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            parts[0].trim().parse::<u32>(),
                            parts[1].trim().parse::<u32>(),
                            parts[2].trim().parse::<u32>()
                        ) {
                            // Convert 0-255 to 0-65535
                            let r = (r * 257).min(65535);
                            let g = (g * 257).min(65535);
                            let b = (b * 257).min(65535);
                            return Ok(format!(r#"tell application "Microsoft Excel"
    set color of font object of range "{}" of active sheet to {{{}, {}, {}}}
    return "✓ Font color set for {}"
end tell"#, range, r, g, b, range));
                        }
                    }
                    "{0, 0, 0}" // Default to black
                }
            };
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
            // Simpler approach: iterate through cells directly
            format!(r#"
tell application "Microsoft Excel"
    set ws to active sheet
    set theRange to range "{0}" of ws
    set startRow to first row index of theRange
    set startCol to first column index of theRange
    set rCount to count of rows of theRange
    set cCount to count of columns of theRange
    set colLetters to "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    
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
                set cellVal to value of range cellRef of ws
                set cellFormula to formula of range cellRef of ws
                if cellVal is not missing value then
                    set valText to cellVal as text
                    if valText is not "" then
                        if cellFormula starts with "=" then
                            set output to output & cellRef & ": " & valText & " [" & cellFormula & "]" & return
                        else
                            set output to output & cellRef & ": " & valText & return
                        end if
                    end if
                end if
            end try
        end repeat
    end repeat
    
    return output
end tell
"#, range)
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
            Ok(output)
        },
        Err(e) => {
            error!("Excel command {} failed: {}", command, e);
            Err(format!("Excel command failed: {}", e))
        }
    }
}

/// Execute an Excel command from action parameters HashMap
/// This is the main entry point called from automation_agent_engine
/// Returns Ok(result_message) on success, Err(error_message) on failure
pub async fn execute_excel_command_from_params(params: &std::collections::HashMap<String, String>) -> Result<String, String> {
    let command = params.get("command").ok_or("Missing command parameter")?;

    // Build parameters array based on command type
    let param_vec: Vec<&str> = match command.as_str() {
        "EXCEL_RENAME_SHEET" => {
            let old_name = params.get("old_name").ok_or("Missing old_name")?;
            let new_name = params.get("new_name").ok_or("Missing new_name")?;
            vec![old_name.as_str(), new_name.as_str()]
        },
        "EXCEL_NEW_SHEET" | "EXCEL_DELETE_SHEET" | "EXCEL_SELECT_SHEET" => {
            let sheet_name = params.get("sheet_name").ok_or("Missing sheet_name")?;
            vec![sheet_name.as_str()]
        },
        "EXCEL_SET_CELL" | "EXCEL_SET_FORMULA" => {
            let cell = params.get("cell").ok_or("Missing cell")?;
            let value = params.get("value").ok_or("Missing value")?;
            vec![cell.as_str(), value.as_str()]
        },
        "EXCEL_FORMAT_CELLS" => {
            let range = params.get("range").ok_or("Missing range")?;
            let format = params.get("format").ok_or("Missing format")?;
            vec![range.as_str(), format.as_str()]
        },
        "EXCEL_AUTOFIT_COLUMNS" | "EXCEL_BOLD" | "EXCEL_CLEAR_RANGE" |
        "EXCEL_GET_CELL_VALUE" | "EXCEL_GET_RANGE_VALUES" |
        "EXCEL_GET_FORMULA" | "EXCEL_GET_RANGE_FORMULAS" | "EXCEL_GET_STRUCTURE" => {
            let range = params.get("range").ok_or("Missing range")?;
            vec![range.as_str()]
        },
        "EXCEL_GET_SHEET_INFO" => vec![],
        "EXCEL_SET_FILL_COLOR" | "EXCEL_SET_FONT_COLOR" => {
            let range = params.get("range").ok_or("Missing range")?;
            let color = params.get("color").ok_or("Missing color")?;
            vec![range.as_str(), color.as_str()]
        },
        "EXCEL_ADD_BORDER" => {
            let range = params.get("range").ok_or("Missing range")?;
            let style = params.get("style").ok_or("Missing style")?;
            vec![range.as_str(), style.as_str()]
        },
        "EXCEL_SET_COLUMN_WIDTH" => {
            let column = params.get("column").ok_or("Missing column")?;
            let width = params.get("width").ok_or("Missing width")?;
            vec![column.as_str(), width.as_str()]
        },
        "EXCEL_COPY_RANGE" => {
            let source = params.get("source").ok_or("Missing source")?;
            let destination = params.get("destination").ok_or("Missing destination")?;
            vec![source.as_str(), destination.as_str()]
        },
        _ => return Err(format!("Unknown Excel command: {}", command))
    };

    execute_excel_command(command, &param_vec).await
}

/// Check if the command is a GET command that returns data
pub fn is_data_retrieval_command(command: &str) -> bool {
    matches!(command,
        "EXCEL_GET_CELL_VALUE" | "EXCEL_GET_RANGE_VALUES" |
        "EXCEL_GET_FORMULA" | "EXCEL_GET_RANGE_FORMULAS" | "EXCEL_GET_SHEET_INFO" |
        "EXCEL_GET_STRUCTURE"
    )
}
