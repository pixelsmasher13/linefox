# Linefox

**AI agent that runs tasks on your desktop - your way.**

Most AI tools talk to you. Linefox works for you. It controls your browser, terminal, and native desktop apps through accessibility APIs, AppleScript, and CLI commands. Describe a task in plain English, and Linefox does it. Chain tasks across different apps in ways that weren't possible before.

## Why Linefox

**Controls any app on your Mac** — Linefox uses a combination of CLIs, accessibility APIs, and AppleScript to drive whatever's on your screen. Browsers, spreadsheets, document editors, dev tools — if you can use it, Linefox can run it.

**Set the objective, not the prompt** — a built-in two-tier architecture separates the supervising agent from the executor. You define the goal, the agent handles the prompting and drives toward it. Tasks run longer autonomously without you babysitting every step.

**Efficient with your tokens** — a sliding memory system keeps only what's relevant to the current task. You get more done per API call instead of burning through your budget re-sending the same context every turn.

**Easy UI** — clean interface to create, edit, run, and schedule tasks. No YAML files, no CLI-only workflows.

**Schedule and forget** — daily, weekly, weekdays, or custom intervals. Tasks run in the background on your machine.

**Remote control** — trigger and monitor tasks directly on the app or via Telegram or Discord.

**Your keys, your data** — everything runs locally. Bring your own API key — Claude, OpenAI, Gemini, Grok, or DeepSeek.

## Requirements

- macOS 11.0 or later
- [Node.js 18+](https://nodejs.org/en/download/package-manager)
- [Rust](https://www.rust-lang.org/tools/install)

## Getting started

1. Clone and install:
```bash
git clone https://github.com/pixelsmasher13/linefox_open.git
cd linefox_open
npm install
```

2. Set up your API keys:
```bash
cp .env.example .env
# Add at least one LLM provider API key
```

3. Run:
```bash
npm run tauri dev
```

4. Grant accessibility permissions when prompted (System Settings > Privacy & Security > Accessibility).

## Building

```bash
npm run tauri build
```

Output goes to `src-tauri/target/release/bundle/macos/`.

## Contributing

Contributions are welcome. Open a PR or file an issue at [github.com/pixelsmasher13/linefox_open](https://github.com/pixelsmasher13/linefox_open).

## License

MIT - see [LICENSE](LICENSE).
