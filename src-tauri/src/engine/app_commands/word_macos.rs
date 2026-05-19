//! Word-specific AppleScript commands for macOS//!
//! These commands provide direct Word automation via AppleScript,
//! which is more reliable than clicking UI elements.

use log::{info, error};

/// Returns the prompt section documenting Word-specific commands
pub fn get_word_commands_prompt() -> String {
    r#"
────────────────────────────
# WORD-SPECIFIC COMMANDS (macOS)
────────────────────────────
⚠️ These commands are ONLY available when Microsoft Word is the active application.
They use native AppleScript automation and are MORE RELIABLE than clicking UI elements.
PREFER these commands over CLICK/TYPE when working with Word.


## ::: CHAINING (supported for INSERT commands only)
You CAN chain insert/text commands with ::: to build documents faster:


✅ WORD_INSERT_TEXT:First paragraph:::WORD_INSERT_TEXT:Second paragraph
✅ WORD_INSERT_PARAGRAPH:Chapter 1:::WORD_INSERT_TEXT:This chapter covers...
✅ Mixed: WORD_INSERT_TEXT:Title:::WORD_INSERT_PARAGRAPH:Body text here

⚠️ Do NOT chain formatting commands (BOLD, ITALIC, ALIGN, SET_FONT) with :::
Formatting requires select → format → deselect workflow and cannot be batched.





## ⚠️ FORMATTING WORKFLOW (IMPORTANT!)
Formatting commands (BOLD, ITALIC, UNDERLINE, ALIGN, SET_FONT, SET_FONT_SIZE) only work on SELECTED text.
You MUST follow this workflow:
1. **SELECT** the text you want to format using one of the selection commands
2. **APPLY** the formatting command
3. **DESELECT** with WORD_DESELECT before doing anything else

⚠️ CRITICAL: After formatting, you MUST call WORD_DESELECT before pressing keys or inserting text!
If text is still selected, PRESS:enter or WORD_INSERT_TEXT will DELETE the selected text and replace it.

⚠️ FORMATTING CARRY-OVER: After formatting text (bold, large font, etc.), the NEXT text you insert
will inherit that formatting! Use WORD_INSERT_PARAGRAPH (auto-resets to Calibri 11pt) or call
WORD_RESET_FORMATTING before WORD_INSERT_TEXT to prevent carry-over.

Example workflow to bold a title then write normal body text:
```
WORD_SELECT_TEXT:MEMO || Selecting title to format
WORD_BOLD || Applying bold
WORD_SET_FONT_SIZE:18 || Setting title size
WORD_DESELECT || MUST deselect before inserting text
WORD_INSERT_PARAGRAPH:This is normal body text || New paragraph auto-resets formatting
```








────────────────────────────
## SELECTION COMMANDS (use these FIRST before formatting)
────────────────────────────
@@ -130,292 +140,354 @@
- Insert entire blocks to text without using \n etc
- Example: WORD_INSERT_TEXT:Hello, World!
- Example: WORD_INSERT_TEXT:"This is paragraphy 1. Mary had a little lamb." || Inserting full paragraph







**WORD_FIND_REPLACE:<find>:<replace>**
- Finds and replaces all occurrences
- Example: WORD_FIND_REPLACE:old text:new text

**WORD_DELETE**
- Deletes the currently selected text
- Must select text first using WORD_SELECT_TEXT or other selection commands
- Example: WORD_SELECT_TEXT:unwanted text || Selecting text to delete
- Example: WORD_DELETE || Removing the selected text

**WORD_MOVE_AFTER_TEXT:<text>**
- Finds text and places cursor immediately after it
- Use this to position cursor before inserting new content
- Example workflow:
  WORD_MOVE_AFTER_TEXT:Introduction || Moving cursor after Introduction heading
  WORD_INSERT_TEXT:New paragraph here || Inserting content after Introduction

**WORD_INSERT_TABLE:<rows>:<cols>**
- Inserts a table
- Example: WORD_INSERT_TABLE:3:4 || Creating a 3x4 table

────────────────────────────
## DOCUMENT COMMANDS
────────────────────────────

**WORD_GET_TEXT**
- Gets all document text (stores in MEMORY)
- Example: WORD_GET_TEXT || Reading document content

**WORD_GET_WORD_COUNT**
- Gets document word count (stores in MEMORY)
- Example: WORD_GET_WORD_COUNT

**WORD_GET_DOCUMENT_INFO**
- Gets document structure overview with formatting per paragraph (stores in MEMORY)
- Shows each paragraph with: text preview, bold, italic, underline, alignment, font, size
- Use this to understand the current state/structure of the document
- Example: WORD_GET_DOCUMENT_INFO || Getting document overview to see current formatting

## 📋 PROFESSIONAL WORD GUIDELINES

### Document Setup
- **Before starting**: Check if document has content. If yes, create NEW document (WORD_NEW_DOCUMENT) unless editing existing
- **Never overwrite**: Don't add content to documents with existing work unless explicitly asked

### Text Formatting
- **Body text**: Calibri 11pt or Times New Roman 12pt (consistent throughout)
- **Emphasis**: Bold for important terms, italics for titles/emphasis
- **Alignment**: Left-align body text for readability

### Content Organization
- **Lead with key points**: Executive summary or main findings first
- **Logical flow**: Introduction → Main content → Conclusion
- **Use lists**: Bullet points for 3+ related items

### Find & Replace Tips
- `^p` = paragraph break
- `^t` = tab
- `^l` = line break (soft return)

DO NOT SPEND MORE THAN 10-15 ACTIONS ON FORMATTING - FORMAT WELL ENOUGH AND MOVE ON

────────────────────────────
"#.to_string()
}

/// Execute a Word AppleScript command
/// Returns Ok(result_message) on success, Err(error_message) on failure
pub async fn execute_word_command(command: &str, params: &[&str]) -> Result<String, String> {
    let script = match command {
        "WORD_NEW_DOCUMENT" => {
            r#"
tell application "Microsoft Word"
    make new document
    activate
end tell
"#.to_string()
        },

        "WORD_OPEN" => {
            if params.is_empty() {
                return Err("WORD_OPEN requires a file path".to_string());
            }
            format!(r#"
tell application "Microsoft Word"
    open "{}"
    activate
end tell
"#, params[0])
        },

        "WORD_CLOSE" => {
            r#"
tell application "Microsoft Word"
    close active document
end tell
"#.to_string()
        },

        "WORD_INSERT_TEXT" => {
            if params.is_empty() {
                return Err("WORD_INSERT_TEXT requires text".to_string());
            }
            let text = params[0].replace("\"", "\\\"");
            // Always append a paragraph break so each insert starts its own paragraph
            format!(r#"
tell application "Microsoft Word"
    insert text "{}" & return at end of text object of active document
    return "Inserted text successfully"
end tell
"#, text)
        },

        "WORD_INSERT_PARAGRAPH" => {
            let text = if params.is_empty() || params[0].trim().is_empty() {
                "".to_string()
            } else {
                params[0].replace("\"", "\\\"")
            };
            // Reset formatting at insertion point before inserting so the new
            // paragraph doesn't inherit bold/size/etc. from previous text.
            format!(r#"
tell application "Microsoft Word"
    -- Move to end and reset formatting so new paragraph is clean
    set docEnd to end of content of text object of active document
    set selection start of selection to docEnd
    set selection end of selection to docEnd
    set bold of font object of selection to false
    set italic of font object of selection to false
    set underline of font object of selection to underline none
    set name of font object of selection to "Calibri"
    set font size of font object of selection to 11
    set alignment of paragraph format of selection to align paragraph left
    insert text "{}" & return at end of text object of active document
    return "Inserted paragraph successfully"
end tell
"#, text)
        },
























































        "WORD_RESET_FORMATTING" => {
            // Reset the font/paragraph at the current insertion point to defaults.
            // Call this after formatting text and before inserting new text to prevent
            // carry-over of bold, font size, etc.
            r#"
tell application "Microsoft Word"
    set bold of font object of selection to false
    set italic of font object of selection to false
    set underline of font object of selection to underline none
    set name of font object of selection to "Calibri"
    set font size of font object of selection to 11
    set alignment of paragraph format of selection to align paragraph left
    return "Formatting reset to defaults (Calibri 11pt, normal)"
end tell
"#.to_string()
        },

        "WORD_FIND_REPLACE" => {
            if params.len() != 2 {
                return Err("WORD_FIND_REPLACE requires find_text and replace_text".to_string());
            }
            let find_text = params[0].replace("\"", "\\\"");
            let replace_text = params[1].replace("\"", "\\\"");
            format!(r#"
tell application "Microsoft Word"
    set findObj to find object of selection
    set content of findObj to "{}"
    set replacement of findObj to "{}"
    execute find findObj replace replace all
end tell
"#, find_text, replace_text)
        },

        "WORD_SET_FONT" => {
            if params.is_empty() {
                return Err("WORD_SET_FONT requires a font name".to_string());
            }
            format!(r#"
tell application "Microsoft Word"
    set name of font object of selection to "{}"
end tell
"#, params[0])
        },

        "WORD_SET_FONT_SIZE" => {
            if params.is_empty() {
                return Err("WORD_SET_FONT_SIZE requires a size".to_string());
            }
            format!(r#"
tell application "Microsoft Word"
    set font size of font object of selection to {}
end tell
"#, params[0])
        },

        "WORD_BOLD" => {
            // Idempotent: always sets bold to true (never toggles)
            r#"
tell application "Microsoft Word"
    set bold of font object of selection to true
    return "✓ Text is now BOLD"
end tell
"#.to_string()
        },

        "WORD_UNBOLD" => {
            r#"
tell application "Microsoft Word"
    set bold of font object of selection to false
    return "✓ Bold removed"
end tell
"#.to_string()
        },

        "WORD_ITALIC" => {
            // Idempotent: always sets italic to true (never toggles)
            r#"
tell application "Microsoft Word"
    set italic of font object of selection to true
    return "✓ Text is now ITALIC"
end tell
"#.to_string()
        },

        "WORD_UNITALIC" => {
            r#"
tell application "Microsoft Word"
    set italic of font object of selection to false
    return "✓ Italic removed"
end tell
"#.to_string()
        },

        "WORD_UNDERLINE" => {
            // Idempotent: always sets underline (never toggles)
            r#"
tell application "Microsoft Word"
    set underline of font object of selection to underline single
    return "✓ Text is now UNDERLINED"
end tell
"#.to_string()
        },

        "WORD_UNUNDERLINE" => {
            r#"
tell application "Microsoft Word"
    set underline of font object of selection to underline none
    return "✓ Underline removed"
end tell
"#.to_string()
        },

        "WORD_ALIGN" => {
            if params.is_empty() {
                return Err("WORD_ALIGN requires an alignment (left, center, right, justify)".to_string());
            }
            let alignment = match params[0].to_lowercase().as_str() {
                "left" => "align paragraph left",
                "center" => "align paragraph center",
                "right" => "align paragraph right",
                "justify" => "align paragraph justify",
                _ => "align paragraph left",
            };
            format!(r#"
tell application "Microsoft Word"
    set alignment of paragraph format of selection to {}
end tell
"#, alignment)
        },

        "WORD_INSERT_TABLE" => {
            if params.len() != 2 {
                return Err("WORD_INSERT_TABLE requires rows and columns".to_string());
            }
            format!(r#"
tell application "Microsoft Word"
    make new table at active document with properties {{number of rows:{}, number of columns:{}}}
end tell
"#, params[0], params[1])
        },

        "WORD_DELETE" => {
            r#"
tell application "Microsoft Word"
    set content of text object of selection to ""
end tell
"#.to_string()
        },

        "WORD_MOVE_AFTER_TEXT" => {
            if params.is_empty() {
                return Err("WORD_MOVE_AFTER_TEXT requires text to find".to_string());
            }
            let text = params[0].replace("\"", "\\\"");
            format!(r#"
tell application "Microsoft Word"
    -- Move to start of document first
    set selection start of selection to 0
    set selection end of selection to 0

    -- Use Word's Find feature to locate the text
    set findObj to find object of selection
    clear formatting findObj
    set content of findObj to "{}"
    set forward of findObj to true
    set wrap of findObj to find stop
    set match case of findObj to false

    set findResult to execute find findObj
    if findResult then
        -- Move cursor to end of found text (collapse selection to end)
        set selEnd to selection end of selection
        set selection start of selection to selEnd
        set selection end of selection to selEnd
        return "Cursor moved after: {}"
    else
        error "Text not found in document"
    end if
end tell
"#, text, text)
        },

        "WORD_GET_TEXT" => {
            r#"
tell application "Microsoft Word"
    get content of text object of active document
end tell
"#.to_string()
        },

        "WORD_GET_SELECTION" => {
            r#"
tell application "Microsoft Word"
    get content of text object of selection
end tell
"#.to_string()
        },

        "WORD_GET_FORMATTING" => {
            r#"
tell application "Microsoft Word"
    set sel to selection
    set fontObj to font object of sel
    set paraFormat to paragraph format of sel

    -- Get font properties
    set fontName to name of fontObj
    set fontSize to font size of fontObj
    set isBold to bold of fontObj
    set isItalic to italic of fontObj
    set underlineStyle to underline of fontObj

    -- Get paragraph alignment
    set paraAlign to alignment of paraFormat

    -- Convert alignment to readable string
    set alignStr to "unknown"
    if paraAlign is align paragraph left then
        set alignStr to "left"
    else if paraAlign is align paragraph center then
        set alignStr to "center"
    else if paraAlign is align paragraph right then
        set alignStr to "right"
    else if paraAlign is align paragraph justify then
        set alignStr to "justify"
    end if

    -- Convert underline to readable string
    set underlineStr to "none"
    if underlineStyle is not underline none then
        set underlineStr to "underlined"
    end if

    return "Font: " & fontName & ", Size: " & fontSize & "pt, Bold: " & isBold & ", Italic: " & isItalic & ", Underline: " & underlineStr & ", Alignment: " & alignStr
end tell
"#.to_string()
        },

        "WORD_GET_WORD_COUNT" => {
            r#"
tell application "Microsoft Word"
    get word count of active document
end tell
"#.to_string()
        },

        "WORD_GET_DOCUMENT_INFO" => {
            r#"
tell application "Microsoft Word"
    set docText to text object of active document
    set paraCount to count of paragraphs of docText

    -- Get cursor/selection position (wrapped in try — fails when dialog is open)
    set cursorInfo to "Cursor: unavailable"
    try
        set selStart to selection start of selection
        set selEnd to selection end of selection
        if selStart is equal to selEnd then
            -- No selection, just a cursor position
            -- Find which paragraph the cursor is in
            set cursorPara to 0
            set charsSoFar to 0
            repeat with i from 1 to paraCount
                set paraLen to (length of content of text object of paragraph i of docText)
                set charsSoFar to charsSoFar + paraLen
                if selStart < charsSoFar and cursorPara is 0 then
                    set cursorPara to i
                end if
            end repeat
            if cursorPara is 0 then set cursorPara to paraCount
            set cursorInfo to "Cursor: position " & selStart & " (in paragraph " & cursorPara & ")" & return & "Selection: none"
        else
            -- There is selected text
            set selLength to selEnd - selStart
            set cursorInfo to "Cursor: position " & selStart & return & "Selection: chars " & selStart & "-" & selEnd & " (" & selLength & " chars selected)"
        end if
    end try

    set resultText to cursorInfo & return & return & "Document Structure (" & paraCount & " paragraphs):" & return & return

    repeat with i from 1 to paraCount
        set thePara to paragraph i of docText
        set paraContent to content of text object of thePara

        -- Trim and preview the text (first 50 chars)
        set paraPreview to paraContent
        if length of paraPreview > 50 then
            set paraPreview to text 1 thru 50 of paraPreview & "..."
        end if
        -- Remove newlines from preview
        set paraPreview to my replaceText(paraPreview, return, " ")
        set paraPreview to my replaceText(paraPreview, linefeed, " ")

        -- Get formatting of first character in paragraph
        set paraRange to text object of thePara
        set fontObj to font object of paraRange
        set paraFormat to paragraph format of paraRange

        set fontName to name of fontObj
        set fontSize to font size of fontObj
        set isBold to bold of fontObj
        set isItalic to italic of fontObj
        set underlineStyle to underline of fontObj

        -- Get alignment
        set paraAlign to alignment of paraFormat
        set alignStr to "left"
        if paraAlign is align paragraph center then
            set alignStr to "center"
        else if paraAlign is align paragraph right then
            set alignStr to "right"
        else if paraAlign is align paragraph justify then
            set alignStr to "justify"
        end if

        -- Build underline string
        set underlineStr to ""
        if underlineStyle is not underline none then
            set underlineStr to ", Underline"
        end if

        -- Build formatting tags
        set formatTags to ""
        if isBold then set formatTags to formatTags & "Bold "
        if isItalic then set formatTags to formatTags & "Italic "
        if formatTags is "" then set formatTags to "Normal"

        set resultText to resultText & "P" & i & ": \"" & paraPreview & "\"" & return
        set resultText to resultText & "    [" & formatTags & "| " & alignStr & " | " & fontName & " " & fontSize & "pt" & underlineStr & "]" & return

    end repeat

    return resultText
end tell

on replaceText(theText, searchStr, replaceStr)
    set AppleScript's text item delimiters to searchStr
    set textItems to text items of theText
    set AppleScript's text item delimiters to replaceStr
    set theText to textItems as text
    set AppleScript's text item delimiters to ""
    return theText
end replaceText
"#.to_string()
        },

        "WORD_DESELECT" => {
            r#"
tell application "Microsoft Word"
    set selEnd to selection end of selection
    set selection start of selection to selEnd
    set selection end of selection to selEnd
    return "Deselected — cursor at end of previous selection"
end tell
"#.to_string()
        },

        "WORD_SELECT_ALL" => {
            r#"
tell application "Microsoft Word"
    set docEnd to end of content of text object of active document
    set selection start of selection to 0
    set selection end of selection to docEnd
end tell
"#.to_string()
        },

        "WORD_SELECT_PARAGRAPH" => {
            if params.is_empty() {
                return Err("WORD_SELECT_PARAGRAPH requires a paragraph number".to_string());
            }
            format!(r#"
tell application "Microsoft Word"
    set paraNum to {}
    set docText to text object of active document
    set paraCount to count of paragraphs of docText

    if paraNum > paraCount then
        error "Paragraph " & paraNum & " does not exist. Document has " & paraCount & " paragraphs."
    end if

    set thePara to paragraph paraNum of docText

    -- Get the text range of the paragraph
    set paraRange to text object of thePara

    -- Select the paragraph by creating a range and selecting it
    select paraRange
end tell
"#, params[0])
        },

        "WORD_SELECT_BETWEEN" => {
            if params.len() != 2 {
                return Err("WORD_SELECT_BETWEEN requires start_text and end_text".to_string());
            }
            let start_text = params[0].replace("\"", "\\\"");
            let end_text = params[1].replace("\"", "\\\"");
            format!(r#"
tell application "Microsoft Word"
    set docText to content of text object of active document

    -- Find start position
    set startPos to offset of "{}" in docText
    if startPos is 0 then
        error "Start text '{}' not found in document"
    end if

    -- Find end position (search from after start)
    set searchFrom to startPos + (length of "{}")
    set restOfDoc to text (searchFrom) thru -1 of docText
    set endOffset to offset of "{}" in restOfDoc
    if endOffset is 0 then
        error "End text '{}' not found after start text"
    end if
    set endPos to searchFrom + endOffset + (length of "{}") - 2

    -- Select the range (convert to 0-based for Word)
    set selection start of selection to (startPos - 1)
    set selection end of selection to endPos

    return "Selected from '{}' to '{}' (" & (endPos - startPos + 1) & " characters)"
end tell
"#, start_text, start_text, start_text, end_text, end_text, end_text, start_text, end_text)
        },

        "WORD_SELECT_TEXT" => {
            if params.is_empty() {
                return Err("WORD_SELECT_TEXT requires text to find".to_string());
            }
            let text = params[0].replace("\"", "\\\"");
            format!(r#"
tell application "Microsoft Word"
    -- Move to start of document first
    set selection start of selection to 0
    set selection end of selection to 0

    -- Use Word's Find feature to locate and select the text
    set findObj to find object of selection
    clear formatting findObj
    set content of findObj to "{}"
    set forward of findObj to true
    set wrap of findObj to find stop
    set match case of findObj to false

    set findResult to execute find findObj
    if findResult then
        return "Selected: {}"
    else
        error "Text not found in document"
    end if
end tell
"#, text, text)
        },

        _ => {
            return Err(format!("Unknown Word command: {}", command));
        }
    };

    info!("Executing Word AppleScript command: {}", command);

    // Execute the AppleScript
    match crate::window_details_collector::macos::macos_acting_engine::execute_applescript(&script).await {
        Ok(output) => {
            info!("Word command {} executed successfully: {}", command, output);
            // Return confirmation if AppleScript returned empty (write operations)
            if output.trim().is_empty() {
                Ok(format!("{} completed successfully", command))
            } else {
                Ok(output)
            }
        },
        Err(e) => {
            error!("Word command {} failed: {}", command, e);
            Err(format!("Word command failed: {}", e))
        }
    }
}

/// Execute a Word command from action parameters HashMap
/// This is the main entry point called from automation_agent_engine
/// Returns Ok(result_message) on success, Err(error_message) on failure
///
/// Supports ::: chaining for insert commands (WORD_INSERT_TEXT, WORD_INSERT_PARAGRAPH).
pub async fn execute_word_command_from_params(params: &std::collections::HashMap<String, String>) -> Result<String, String> {
    let command = params.get("command").ok_or("Missing command parameter")?;

    // Handle ::: chaining in raw_input
    if let Some(raw) = params.get("raw_input") {
        if raw.contains(":::") {
            return execute_chained_word_commands(command, raw).await;
        }
    }

    // Build parameters array based on command type
    let param_vec: Vec<&str> = match command.as_str() {
        "WORD_NEW_DOCUMENT" | "WORD_CLOSE" |
        "WORD_BOLD" | "WORD_UNBOLD" | "WORD_ITALIC" | "WORD_UNITALIC" | 
        "WORD_UNDERLINE" | "WORD_UNUNDERLINE" | "WORD_DELETE" | "WORD_DESELECT" |
        "WORD_RESET_FORMATTING" |
        "WORD_GET_TEXT" | "WORD_GET_SELECTION" | "WORD_GET_FORMATTING" | "WORD_GET_WORD_COUNT" | "WORD_GET_DOCUMENT_INFO" | "WORD_SELECT_ALL" => vec![],

        "WORD_OPEN" => {
            let filepath = params.get("filepath").ok_or("Missing filepath")?;
            vec![filepath.as_str()]
        },
        "WORD_INSERT_TEXT" => {
            let text = params.get("text").ok_or("Missing text")?;
            vec![text.as_str()]
        },
        "WORD_INSERT_PARAGRAPH" => {
            // Allow empty paragraph for spacing
            let text = params.get("text").map(|s| s.as_str()).unwrap_or("");
            vec![text]
        },

        "WORD_FIND_REPLACE" => {
            let find = params.get("find").ok_or("Missing find")?;
            let replace = params.get("replace").ok_or("Missing replace")?;
            vec![find.as_str(), replace.as_str()]
        },
        "WORD_SET_FONT" => {
            let font = params.get("font").ok_or("Missing font")?;
            vec![font.as_str()]
        },
        "WORD_SET_FONT_SIZE" => {
            let size = params.get("size").ok_or("Missing size")?;
            vec![size.as_str()]
        },
        "WORD_ALIGN" => {
            let alignment = params.get("alignment").ok_or("Missing alignment")?;
            vec![alignment.as_str()]
        },
        "WORD_INSERT_TABLE" => {
            let rows = params.get("rows").ok_or("Missing rows")?;
            let cols = params.get("cols").ok_or("Missing cols")?;
            vec![rows.as_str(), cols.as_str()]
        },
        "WORD_SELECT_PARAGRAPH" => {
            let paragraph = params.get("paragraph").ok_or("Missing paragraph number")?;
            vec![paragraph.as_str()]
        },
        "WORD_SELECT_TEXT" => {
            let text = params.get("text").ok_or("Missing text to select")?;
            vec![text.as_str()]
        },
        "WORD_MOVE_AFTER_TEXT" => {
            let text = params.get("text").ok_or("Missing text to find")?;
            vec![text.as_str()]
        },
        "WORD_SELECT_BETWEEN" => {
            let start_text = params.get("start_text").ok_or("Missing start_text")?;
            let end_text = params.get("end_text").ok_or("Missing end_text")?;
            vec![start_text.as_str(), end_text.as_str()]
        },
        _ => return Err(format!("Unknown Word command: {}", command))
    };

    execute_word_command(command, &param_vec).await
}

/// Handle ::: chained Word commands by splitting and executing each one.
async fn execute_chained_word_commands(command: &str, raw_input: &str) -> Result<String, String> {
    let prefix = format!("{}:", command);
    let params_part = if raw_input.to_uppercase().starts_with(&prefix.to_uppercase()) {
        &raw_input[prefix.len()..]
    } else {
        raw_input
    };

    let parts: Vec<&str> = params_part.split(":::").collect();
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part.is_empty() { continue; }

        // Detect if this part starts with a different WORD_ command
        let (effective_cmd, effective_params) = if part.to_uppercase().starts_with("WORD_") {
            if let Some(colon_pos) = part.find(':') {
                (&part[..colon_pos], part[colon_pos + 1..].to_string())
            } else {
                (part, String::new())
            }
        } else {
            (command, part.to_string())
        };

        let param_vec = if effective_params.is_empty() {
            vec![]
        } else {
            vec![effective_params.as_str()]
        };

        match execute_word_command(effective_cmd, &param_vec).await {
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

/// Check if the command is a GET command that returns data
pub fn is_data_retrieval_command(command: &str) -> bool {
    command == "WORD_GET_TEXT" || command == "WORD_GET_WORD_COUNT" || command == "WORD_GET_SELECTION" || command == "WORD_GET_FORMATTING" || command == "WORD_GET_DOCUMENT_INFO"
}
