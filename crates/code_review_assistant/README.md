# Code Review Assistant

An AI-powered code review assistant panel for Zed editor that provides contextual feedback on selected code.

## Features

- **Selection-based review**: Select code in the editor and get AI-powered feedback
- **Contextual analysis**: AI receives surrounding code context for better understanding
- **Multiple threads**: Support for multiple independent review threads per file
- **Severity indicators**: Feedback categorized as Error, Warning, Suggestion, or Info
- **Code suggestions**: AI can provide improved code snippets
- **Follow-up questions**: Continue the conversation within each thread

## Usage

1. Open any file in the editor
2. Select the code you want reviewed
3. Open the Code Review panel (`Ctrl+Shift+R` on Linux/Windows, `Cmd+Shift+R` on macOS)
4. Optionally type a specific question in the input box
5. Click "Review Selection" or press `Ctrl+Alt+R` (`Cmd+Alt+R` on macOS)
6. View the AI's feedback and continue the conversation

## Keyboard Shortcuts

| Action | Linux/Windows | macOS |
|--------|---------------|-------|
| Toggle Panel Focus | `Ctrl+Shift+R` | `Cmd+Shift+R` |
| Review Selection | `Ctrl+Alt+R` | `Cmd+Alt+R` |

## Settings

Configure the code review assistant in your settings.json:

```json
{
  "code_review": {
    "button": true,
    "dock": "right",
    "default_width": 360,
    "context_lines": 10,
    "show_inline_annotations": true,
    "custom_prompt": "Focus on security and performance issues."
  }
}
```

### Available Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `button` | bool | `true` | Show panel button in dock |
| `dock` | string | `"right"` | Panel position: `"left"` or `"right"` |
| `default_width` | number | `360` | Default panel width in pixels |
| `context_lines` | number | `10` | Lines of context before/after selection |
| `show_inline_annotations` | bool | `true` | Show inline annotations in editor |
| `custom_prompt` | string | `null` | Custom system prompt prepended to requests |

## Architecture

### Key Components

- **`CodeReviewPanel`**: Main panel UI component implementing `Panel` trait
- **`ReviewThread`**: Represents a conversation about a code selection
- **`ReviewComment`**: Individual message in a thread (user or AI)
- **`CodeSelection`**: Captured code selection with context

### Data Flow

1. User selects code in editor
2. Selection captured with surrounding context
3. Request sent to configured language model
4. Response parsed for severity and code suggestions
5. Thread updated and UI refreshed

## Development

### Building

```bash
cargo check -p code_review_assistant
```

### Running Tests

```bash
cargo test -p code_review_assistant
```

## Trade-offs

Made with limited time:

1. **No persistence**: Review threads are lost when closing the editor
2. **Basic severity detection**: Uses keyword matching rather than structured AI output
3. **Single file focus**: Doesn't support cross-file review threads
4. **No diff application**: Suggested code changes must be manually applied

## Future Improvements

With more time, would add:

1. **Structured AI output**: Use function calling for better severity/suggestion parsing
2. **Apply suggestions**: One-click to apply suggested code changes
3. **Thread persistence**: Save and restore threads across sessions
4. **Inline annotations**: Show review markers directly in editor gutter
5. **Team collaboration**: Share review threads with collaborators
6. **Multi-file context**: Include related files in review context

## How AI Tools Were Used

This implementation used Claude to:
- Explore the Zed codebase architecture and patterns
- Generate initial code structure following existing conventions
- Debug GPUI-specific patterns (Entity, Context, async closures)
- Iterate on UI component design

All AI suggestions were verified against existing code patterns in the codebase.

## License

GPL-3.0-or-later (following Zed's licensing)
