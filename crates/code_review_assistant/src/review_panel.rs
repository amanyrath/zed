use crate::review_settings::CodeReviewSettings;
use crate::review_thread::{
    CodeSelection, CommentRole, ReviewSeverity, ReviewThread, ThreadId,
};
use crate::ToggleFocus;
use anyhow::Result;
use collections::HashMap;
use editor::{Editor, EditorElement};
use futures::StreamExt;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, WeakEntity, Window,
};
use language::Buffer;
use language_model::{
    LanguageModel, LanguageModelRegistry, LanguageModelRequest, LanguageModelRequestMessage, Role,
};
use multi_buffer::MultiBuffer;
use panel::{panel_editor_container, panel_editor_style, PanelHeader};
use project::Project;
use settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;
use text::ToPoint;
use ui::{prelude::*, Scrollbars, Tooltip, WithScrollbar};
use util::ResultExt;
use workspace::dock::{DockPosition, Panel, PanelEvent};
use workspace::Workspace;

const CODE_REVIEW_PANEL_KEY: &str = "CodeReviewPanel";

pub struct CodeReviewPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    threads: Vec<ReviewThread>,
    selected_thread: Option<ThreadId>,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    input_editor: Entity<Editor>,
    pending_tasks: HashMap<ThreadId, Task<()>>,
    fs: Arc<dyn project::Fs>,
    _settings_subscription: Subscription,
}

#[derive(Debug, Clone)]
pub enum Event {
    Focus,
}

impl EventEmitter<Event> for CodeReviewPanel {}
impl EventEmitter<PanelEvent> for CodeReviewPanel {}

impl CodeReviewPanel {
    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let app_state = workspace.app_state().clone();
        let fs = app_state.fs.clone();
        let weak_workspace = workspace.weak_handle();

        cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            cx.on_focus(&focus_handle, window, Self::focus_in).detach();

            let input_buffer = cx.new(|cx| Buffer::local("", cx));
            let input_editor = cx.new(|cx| {
                let buffer = cx.new(|cx| MultiBuffer::singleton(input_buffer, cx));
                let mut editor = Editor::new(
                    editor::EditorMode::AutoHeight {
                        min_lines: 2,
                        max_lines: Some(6),
                    },
                    buffer,
                    None,
                    window,
                    cx,
                );
                editor.set_placeholder_text("Ask about the selected code...", window, cx);
                editor.set_show_gutter(false, cx);
                editor.set_use_modal_editing(true);
                editor
            });

            let settings_subscription =
                cx.observe_global::<settings::SettingsStore>(|_, cx| cx.notify());

            Self {
                focus_handle,
                width: None,
                threads: Vec::new(),
                selected_thread: None,
                project,
                workspace: weak_workspace,
                input_editor,
                pending_tasks: HashMap::default(),
                fs,
                _settings_subscription: settings_subscription,
            }
        })
    }

    fn focus_in(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Event::Focus);
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            Self::new(workspace, window, cx)
        })
    }

    pub fn review_current_selection(
        &mut self,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
        else {
            return;
        };

        let selection = editor.read(cx).selections.newest::<usize>(cx);
        if selection.is_empty() {
            return;
        }

        let buffer = editor.read(cx).buffer().read(cx);
        let snapshot = buffer.snapshot(cx);

        let start_point = selection.start.to_point(&snapshot);
        let end_point = selection.end.to_point(&snapshot);

        let selected_text: String = snapshot
            .text_for_range(selection.start..selection.end)
            .collect();

        if selected_text.trim().is_empty() {
            return;
        }

        let settings = CodeReviewSettings::get_global(cx);
        let context_lines = settings.context_lines as usize;

        let context_start_row = start_point.row.saturating_sub(context_lines as u32);
        let context_end_row = (end_point.row + context_lines as u32 + 1)
            .min(snapshot.max_point().row + 1);

        let context_start = snapshot.point_to_offset(text::Point::new(context_start_row, 0));
        let context_end_point = text::Point::new(context_end_row, 0);
        let context_end = if context_end_row <= snapshot.max_point().row {
            snapshot.point_to_offset(context_end_point)
        } else {
            snapshot.len()
        };

        let context_text: String = snapshot
            .text_for_range(context_start..context_end)
            .collect();

        let file_path = buffer
            .file_at(selection.start)
            .map(|f| f.path().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("untitled"));

        let language = buffer
            .language_at(selection.start)
            .map(|l| SharedString::from(l.name().to_string()));

        let code_selection = CodeSelection {
            file_path,
            language,
            selected_text: selected_text.into(),
            context: context_text.into(),
            line_range: (start_point.row + 1)..(end_point.row + 2),
            anchor_range: None,
        };

        let input_text = self.input_editor.read(cx).text(cx);
        let question = if input_text.trim().is_empty() {
            "Please review this code and provide feedback on potential improvements, issues, or best practices.".to_string()
        } else {
            input_text
        };

        let thread = ReviewThread::new(code_selection, question.as_str());
        let thread_id = thread.id;
        self.threads.push(thread);
        self.selected_thread = Some(thread_id);

        self.input_editor.update(cx, |editor, cx| {
            editor.clear(window, cx);
        });

        self.request_ai_review(thread_id, window, cx);
        cx.notify();
    }

    fn request_ai_review(
        &mut self,
        thread_id: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.threads.iter().find(|t| t.id == thread_id) else {
            return;
        };

        let Some(model) = LanguageModelRegistry::read_global(cx)
            .active_model()
        else {
            self.add_error_to_thread(thread_id, "No AI model configured. Please set up a language model in settings.", cx);
            return;
        };

        let settings = CodeReviewSettings::get_global(cx);
        let prompt = build_review_prompt(thread, settings.custom_prompt.as_deref());
        let request = LanguageModelRequest {
            messages: vec![LanguageModelRequestMessage {
                role: Role::User,
                content: prompt.into(),
                cache: false,
            }],
            tools: Vec::new(),
            stop: Vec::new(),
            temperature: Some(0.3),
        };

        let task = cx.spawn_in(window, {
            let model = model.clone();
            async move |this, cx| {
                let result = stream_ai_response(model, request, &cx).await;

                this.update(cx, |panel, cx| {
                    match result {
                        Ok((content, severity, suggestion)) => {
                            if let Some(thread) = panel.threads.iter_mut().find(|t| t.id == thread_id) {
                                thread.add_ai_response(content, severity, suggestion);
                            }
                        }
                        Err(err) => {
                            panel.add_error_to_thread(
                                thread_id,
                                &format!("Failed to get AI response: {}", err),
                                cx,
                            );
                        }
                    }
                    panel.pending_tasks.remove(&thread_id);
                    cx.notify();
                })
                .log_err();
            }
        });

        self.pending_tasks.insert(thread_id, task);
    }

    fn add_error_to_thread(
        &mut self,
        thread_id: ThreadId,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
            thread.add_ai_response(message, ReviewSeverity::Error, None);
            thread.set_loading(false);
        }
        cx.notify();
    }

    pub fn add_followup(
        &mut self,
        thread_id: ThreadId,
        question: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
            thread.add_user_comment(question);
        }
        self.request_ai_review(thread_id, window, cx);
        cx.notify();
    }

    pub fn clear_threads(&mut self, cx: &mut Context<Self>) {
        self.threads.clear();
        self.selected_thread = None;
        self.pending_tasks.clear();
        cx.notify();
    }

    pub fn resolve_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
            thread.resolve();
        }
        cx.notify();
    }

    pub fn toggle_thread_collapsed(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
            thread.toggle_collapsed();
        }
        cx.notify();
    }

    fn render_thread(&self, thread: &ReviewThread, cx: &mut Context<Self>) -> impl IntoElement {
        let thread_id = thread.id;
        let is_selected = self.selected_thread == Some(thread_id);
        let is_collapsed = thread.is_collapsed;
        let is_resolved = thread.is_resolved;
        let severity = thread.last_severity();

        let severity_color = match severity {
            Some(ReviewSeverity::Error) => cx.theme().status().error,
            Some(ReviewSeverity::Warning) => cx.theme().status().warning,
            Some(ReviewSeverity::Suggestion) => cx.theme().status().info,
            Some(ReviewSeverity::Info) | None => cx.theme().status().hint,
        };

        let header = h_flex()
            .w_full()
            .gap_2()
            .px_2()
            .py_1()
            .bg(if is_selected {
                cx.theme().colors().element_selected
            } else {
                cx.theme().colors().element_background
            })
            .border_l_2()
            .border_color(severity_color)
            .child(
                Icon::new(if is_collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                })
                .size(IconSize::Small)
                .color(Color::Muted),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        Label::new(thread.selection.summary())
                            .size(LabelSize::Small)
                            .color(if is_resolved {
                                Color::Muted
                            } else {
                                Color::Default
                            }),
                    ),
            )
            .when(thread.is_loading, |el| {
                el.child(
                    Icon::new(IconName::ArrowCircle)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
            })
            .when(is_resolved, |el| {
                el.child(
                    Icon::new(IconName::Check)
                        .size(IconSize::Small)
                        .color(Color::Success),
                )
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.toggle_thread_collapsed(thread_id, cx);
            }));

        let comments = if is_collapsed {
            div()
        } else {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .children(thread.comments.iter().map(|comment| {
                    self.render_comment(comment, cx)
                }))
        };

        v_flex()
            .w_full()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .overflow_hidden()
            .child(header)
            .child(comments)
    }

    fn render_comment(&self, comment: &ReviewComment, cx: &mut Context<Self>) -> impl IntoElement {
        let is_user = comment.role == CommentRole::User;

        let role_label = if is_user { "You" } else { "AI" };
        let role_color = if is_user {
            Color::Accent
        } else {
            Color::Success
        };

        let mut content = v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .bg(if is_user {
                cx.theme().colors().element_background
            } else {
                cx.theme().colors().surface_background
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new(role_label).size(LabelSize::Small).color(role_color))
                    .when_some(comment.severity, |el, severity| {
                        el.child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Icon::new(severity.icon_name())
                                        .size(IconSize::Small)
                                        .color(match severity {
                                            ReviewSeverity::Error => Color::Error,
                                            ReviewSeverity::Warning => Color::Warning,
                                            ReviewSeverity::Suggestion => Color::Accent,
                                            ReviewSeverity::Info => Color::Muted,
                                        }),
                                )
                                .child(
                                    Label::new(severity.label())
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().colors().text)
                    .child(comment.content.clone()),
            );

        if let Some(ref suggestion) = comment.suggested_code {
            content = content.child(
                v_flex()
                    .mt_2()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().colors().editor_background)
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new("Suggested code:")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_family("monospace")
                            .text_color(cx.theme().colors().text)
                            .child(suggestion.clone()),
                    ),
            );
        }

        content
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .h_full()
            .w_full()
            .items_center()
            .justify_center()
            .gap_2()
            .p_4()
            .child(
                Icon::new(IconName::Sparkle)
                    .size(IconSize::XLarge)
                    .color(Color::Muted),
            )
            .child(
                Label::new("AI Code Review")
                    .size(LabelSize::Large)
                    .color(Color::Default),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().colors().text_muted)
                    .max_w(rems(16.))
                    .text_center()
                    .child("Select code in the editor and use 'Review Selection' to get AI-powered feedback."),
            )
            .child(
                div()
                    .mt_4()
                    .text_xs()
                    .text_color(cx.theme().colors().text_muted)
                    .child("Keyboard: Ctrl+Shift+R (Review Selection)"),
            )
    }
}

impl Focusable for CodeReviewPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CodeReviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let threads = &self.threads;
        let has_threads = !threads.is_empty();

        v_flex()
            .key_context("CodeReviewPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                PanelHeader::new("Code Review")
                    .end_slot(
                        IconButton::new("clear", IconName::Trash)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Clear all reviews"))
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.clear_threads(cx);
                            }))
                            .visible(has_threads),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(if has_threads {
                        div()
                            .size_full()
                            .child(
                                WithScrollbar::new(
                                    div()
                                        .id("review-threads-scroll")
                                        .p_2()
                                        .children(
                                            threads
                                                .iter()
                                                .map(|thread| self.render_thread(thread, cx)),
                                        ),
                                    Scrollbars::show_y(),
                                ),
                            )
                            .into_any_element()
                    } else {
                        self.render_empty_state(cx).into_any_element()
                    }),
            )
            .child(
                v_flex()
                    .p_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().colors().border)
                    .child(panel_editor_container(
                        EditorElement::new(&self.input_editor, panel_editor_style(cx)),
                        false,
                        cx,
                    ))
                    .child(
                        Button::new("review-btn", "Review Selection")
                            .style(ButtonStyle::Filled)
                            .full_width()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(workspace) = this.workspace.upgrade() {
                                    workspace.update(cx, |workspace, cx| {
                                        this.review_current_selection(workspace, window, cx);
                                    });
                                }
                            })),
                    ),
            )
    }
}

impl Panel for CodeReviewPanel {
    fn persistent_name() -> &'static str {
        "CodeReviewPanel"
    }

    fn panel_key() -> &'static str {
        CODE_REVIEW_PANEL_KEY
    }

    fn position(&self, _: &Window, cx: &App) -> DockPosition {
        CodeReviewSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            let code_review = settings.code_review.get_or_insert_with(Default::default);
            code_review.dock = position;
        });
    }

    fn size(&self, _: &Window, cx: &App) -> Pixels {
        self.width
            .unwrap_or_else(|| Pixels(CodeReviewSettings::get_global(cx).default_width))
    }

    fn set_size(&mut self, size: Option<Pixels>, _: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _: &Window, cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::Sparkle).filter(|_| CodeReviewSettings::get_global(cx).button)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Code Review")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        3
    }
}

impl PanelHeader for CodeReviewPanel {}

fn build_review_prompt(thread: &ReviewThread, custom_prompt: Option<&str>) -> String {
    let mut prompt = String::new();

    if let Some(custom) = custom_prompt {
        prompt.push_str(custom);
        prompt.push_str("\n\n");
    }

    prompt.push_str("You are an expert code reviewer. Analyze the following code and provide constructive feedback.\n\n");

    prompt.push_str("## Guidelines:\n");
    prompt.push_str("- Focus on code quality, potential bugs, and best practices\n");
    prompt.push_str("- Provide specific, actionable suggestions\n");
    prompt.push_str("- If you suggest code changes, provide the improved code\n");
    prompt.push_str("- Be concise but thorough\n");
    prompt.push_str("- Categorize your feedback by severity: Error (bugs/security issues), Warning (potential problems), Suggestion (improvements), or Info (explanations)\n\n");

    if let Some(ref lang) = thread.selection.language {
        prompt.push_str(&format!("## Language: {}\n\n", lang));
    }

    prompt.push_str(&format!(
        "## File: {} (lines {}-{})\n\n",
        thread.selection.file_path.display(),
        thread.selection.line_range.start,
        thread.selection.line_range.end - 1
    ));

    prompt.push_str("## Context (surrounding code):\n```\n");
    prompt.push_str(&thread.selection.context);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## Selected code to review:\n```\n");
    prompt.push_str(&thread.selection.selected_text);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## User's question/request:\n");

    for comment in &thread.comments {
        if comment.role == CommentRole::User {
            prompt.push_str(&comment.content);
            prompt.push_str("\n");
        }
    }

    prompt.push_str("\n## Your review:\n");

    prompt
}

async fn stream_ai_response(
    model: Arc<dyn LanguageModel>,
    request: LanguageModelRequest,
    cx: &AsyncWindowContext,
) -> Result<(SharedString, ReviewSeverity, Option<SharedString>)> {
    let mut response_stream = model
        .stream_completion(request, cx)
        .await
        .context("Failed to start AI stream")?;

    let mut full_response = String::new();

    while let Some(chunk) = response_stream.next().await {
        match chunk {
            Ok(text) => {
                full_response.push_str(&text);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Stream error: {}", e));
            }
        }
    }

    let severity = detect_severity(&full_response);
    let suggested_code = extract_code_suggestion(&full_response);

    Ok((full_response.into(), severity, suggested_code))
}

fn detect_severity(response: &str) -> ReviewSeverity {
    let lower = response.to_lowercase();

    if lower.contains("error:")
        || lower.contains("bug")
        || lower.contains("security")
        || lower.contains("critical")
        || lower.contains("vulnerability")
    {
        ReviewSeverity::Error
    } else if lower.contains("warning:")
        || lower.contains("potential issue")
        || lower.contains("should consider")
        || lower.contains("might cause")
    {
        ReviewSeverity::Warning
    } else if lower.contains("suggestion:")
        || lower.contains("could be improved")
        || lower.contains("consider")
        || lower.contains("recommend")
    {
        ReviewSeverity::Suggestion
    } else {
        ReviewSeverity::Info
    }
}

fn extract_code_suggestion(response: &str) -> Option<SharedString> {
    let mut in_code_block = false;
    let mut code_lines = Vec::new();
    let mut found_suggestion = false;

    for line in response.lines() {
        if line.trim().starts_with("```") {
            if in_code_block {
                if !code_lines.is_empty() {
                    found_suggestion = true;
                    break;
                }
            }
            in_code_block = !in_code_block;
            code_lines.clear();
        } else if in_code_block {
            code_lines.push(line);
        }
    }

    if found_suggestion && !code_lines.is_empty() {
        Some(code_lines.join("\n").into())
    } else {
        None
    }
}
