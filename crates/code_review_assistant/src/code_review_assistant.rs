mod review_panel;
mod review_settings;
mod review_thread;

use editor::Editor;
use gpui::{actions, App, Context};
use review_panel::CodeReviewPanel;
use workspace::Workspace;

pub use review_panel::CodeReviewPanel as Panel;
pub use review_settings::CodeReviewSettings;
pub use review_thread::{ReviewComment, ReviewSeverity, ReviewThread};

actions!(
    code_review,
    [
        /// Toggles focus on the code review panel.
        ToggleFocus,
        /// Closes the code review panel.
        Close,
        /// Request AI review for the current selection.
        ReviewSelection,
        /// Clear all review threads.
        ClearReviews,
    ]
);

pub fn init(cx: &mut App) {
    review_settings::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, cx| {
        register(workspace, cx);
    })
    .detach();
}

fn register(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
        workspace.toggle_panel_focus::<CodeReviewPanel>(window, cx);
    });

    workspace.register_action(|workspace, _: &ReviewSelection, window, cx| {
        if let Some(panel) = workspace.panel::<CodeReviewPanel>(cx) {
            panel.update(cx, |panel, cx| {
                panel.review_current_selection(workspace, window, cx);
            });
        }
    });

    workspace.register_action(|workspace, _: &ClearReviews, _window, cx| {
        if let Some(panel) = workspace.panel::<CodeReviewPanel>(cx) {
            panel.update(cx, |panel, cx| {
                panel.clear_threads(cx);
            });
        }
    });
}
