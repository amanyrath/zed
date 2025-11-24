use gpui::SharedString;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;
use text::Anchor;

/// Severity level for review comments
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewSeverity {
    /// Informational suggestion or explanation
    Info,
    /// Minor improvement suggestion
    Suggestion,
    /// Potential issue that should be addressed
    Warning,
    /// Critical issue that must be fixed
    Error,
}

impl ReviewSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            ReviewSeverity::Info => "Info",
            ReviewSeverity::Suggestion => "Suggestion",
            ReviewSeverity::Warning => "Warning",
            ReviewSeverity::Error => "Error",
        }
    }

    pub fn icon_name(&self) -> ui::IconName {
        match self {
            ReviewSeverity::Info => ui::IconName::Info,
            ReviewSeverity::Suggestion => ui::IconName::Sparkle,
            ReviewSeverity::Warning => ui::IconName::Warning,
            ReviewSeverity::Error => ui::IconName::XCircle,
        }
    }
}

/// A single comment in a review thread
#[derive(Debug, Clone)]
pub struct ReviewComment {
    /// Unique identifier for this comment
    pub id: CommentId,
    /// The role of the comment author (user or AI)
    pub role: CommentRole,
    /// The content of the comment
    pub content: SharedString,
    /// Optional severity for AI-generated comments
    pub severity: Option<ReviewSeverity>,
    /// Optional suggested code replacement
    pub suggested_code: Option<SharedString>,
}

impl ReviewComment {
    pub fn user(content: impl Into<SharedString>) -> Self {
        Self {
            id: CommentId::new(),
            role: CommentRole::User,
            content: content.into(),
            severity: None,
            suggested_code: None,
        }
    }

    pub fn ai(
        content: impl Into<SharedString>,
        severity: ReviewSeverity,
        suggested_code: Option<SharedString>,
    ) -> Self {
        Self {
            id: CommentId::new(),
            role: CommentRole::Assistant,
            content: content.into(),
            severity: Some(severity),
            suggested_code,
        }
    }
}

/// Identifies who authored a comment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentRole {
    User,
    Assistant,
}

/// Unique identifier for a comment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentId(u64);

impl CommentId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a review thread
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

/// A code selection that a review thread is associated with
#[derive(Debug, Clone)]
pub struct CodeSelection {
    /// The file path containing the selection
    pub file_path: PathBuf,
    /// The programming language of the file
    pub language: Option<SharedString>,
    /// The selected code text
    pub selected_text: SharedString,
    /// The full surrounding context (before and after selection)
    pub context: SharedString,
    /// Line range of the selection (1-indexed)
    pub line_range: Range<u32>,
    /// Anchor range in the buffer (may become invalid if buffer changes)
    pub anchor_range: Option<Range<Anchor>>,
}

impl CodeSelection {
    pub fn line_count(&self) -> u32 {
        self.line_range.end.saturating_sub(self.line_range.start)
    }

    pub fn summary(&self) -> String {
        let file_name = self
            .file_path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        format!(
            "{}:{}-{}",
            file_name, self.line_range.start, self.line_range.end
        )
    }
}

/// A review thread containing a code selection and conversation
#[derive(Debug, Clone)]
pub struct ReviewThread {
    /// Unique identifier for this thread
    pub id: ThreadId,
    /// The code selection this thread is about
    pub selection: CodeSelection,
    /// The conversation history
    pub comments: Vec<ReviewComment>,
    /// Whether the thread is currently loading an AI response
    pub is_loading: bool,
    /// Whether the thread has been resolved/dismissed
    pub is_resolved: bool,
    /// Whether the thread is collapsed in the UI
    pub is_collapsed: bool,
}

impl ReviewThread {
    pub fn new(selection: CodeSelection, initial_question: impl Into<SharedString>) -> Self {
        let user_comment = ReviewComment::user(initial_question);
        Self {
            id: ThreadId::new(),
            selection,
            comments: vec![user_comment],
            is_loading: true,
            is_resolved: false,
            is_collapsed: false,
        }
    }

    pub fn add_user_comment(&mut self, content: impl Into<SharedString>) {
        self.comments.push(ReviewComment::user(content));
        self.is_loading = true;
    }

    pub fn add_ai_response(
        &mut self,
        content: impl Into<SharedString>,
        severity: ReviewSeverity,
        suggested_code: Option<SharedString>,
    ) {
        self.comments
            .push(ReviewComment::ai(content, severity, suggested_code));
        self.is_loading = false;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }

    pub fn resolve(&mut self) {
        self.is_resolved = true;
    }

    pub fn toggle_collapsed(&mut self) {
        self.is_collapsed = !self.is_collapsed;
    }

    pub fn last_severity(&self) -> Option<ReviewSeverity> {
        self.comments
            .iter()
            .rev()
            .find_map(|c| c.severity)
    }

    pub fn has_suggestions(&self) -> bool {
        self.comments.iter().any(|c| c.suggested_code.is_some())
    }
}
