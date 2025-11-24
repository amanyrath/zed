use gpui::App;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsSources};
use workspace::dock::DockPosition;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CodeReviewSettings {
    /// Whether to show the code review panel button in the dock.
    #[serde(default = "default_button")]
    pub button: bool,

    /// Where to dock the code review panel.
    #[serde(default = "default_dock")]
    pub dock: DockPosition,

    /// Default width of the panel in pixels.
    #[serde(default = "default_width")]
    pub default_width: f32,

    /// Number of context lines to include before and after the selection.
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,

    /// Custom system prompt to prepend to AI requests.
    #[serde(default)]
    pub custom_prompt: Option<String>,

    /// Whether to show inline annotations in the editor.
    #[serde(default = "default_show_inline_annotations")]
    pub show_inline_annotations: bool,
}

fn default_button() -> bool {
    true
}

fn default_dock() -> DockPosition {
    DockPosition::Right
}

fn default_width() -> f32 {
    360.0
}

fn default_context_lines() -> u32 {
    10
}

fn default_show_inline_annotations() -> bool {
    true
}

impl Default for CodeReviewSettings {
    fn default() -> Self {
        Self {
            button: default_button(),
            dock: default_dock(),
            default_width: default_width(),
            context_lines: default_context_lines(),
            custom_prompt: None,
            show_inline_annotations: default_show_inline_annotations(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CodeReviewSettingsContent {
    /// Settings for the code review assistant panel.
    pub code_review: Option<CodeReviewSettings>,
}

impl Settings for CodeReviewSettings {
    const KEY: Option<&'static str> = Some("code_review");

    type FileContent = CodeReviewSettingsContent;

    fn load(
        sources: SettingsSources<Self::FileContent>,
        _: &mut App,
    ) -> anyhow::Result<Self> {
        sources.json_merge()
    }
}

pub fn init(cx: &mut App) {
    CodeReviewSettings::register(cx);
}
