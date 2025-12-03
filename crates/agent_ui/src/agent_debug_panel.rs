use call::{room::Event, ActiveCall};
use gpui::{
    actions, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Pixels, Render, Subscription, Task, WeakEntity,
};
use ui::{prelude::*, IconName, Label};
use workspace::{
    dock::{DockPosition, Panel, PanelEvent},
    Workspace,
};

actions!(agent_debug_panel, [ToggleFocus]);

pub struct AgentDebugPanel {
    width: Option<gpui::Pixels>,
    workspace: WeakEntity<Workspace>,
    #[allow(dead_code)]
    active_call: Option<Entity<ActiveCall>>,
    _subscriptions: Vec<Subscription>,
}

impl AgentDebugPanel {
    pub fn load(
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncWindowContext,
    ) -> Task<Result<Entity<Self>, anyhow::Error>> {
        cx.spawn(async move |cx| {
            let active_call = cx.update(|_window, cx| ActiveCall::global(cx)).ok();

            workspace.update_in(cx, |workspace, _window, cx| {
                cx.new(|cx| Self::new(workspace, active_call, cx))
            })
        })
    }

    fn new(
        workspace: &Workspace,
        active_call: Option<Entity<ActiveCall>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = Vec::new();

        if let Some(active_call) = &active_call {
            subscriptions.push(cx.subscribe(active_call, |this, _call, event, cx| {
                Self::handle_call_event(this, &_call, event, cx);
            }));
        }

        Self {
            width: Some(px(400.0)),
            workspace: workspace.weak_handle(),
            active_call,
            _subscriptions: subscriptions,
        }
    }

    fn handle_call_event(
        &mut self,
        _call: &Entity<ActiveCall>,
        event: &call::room::Event,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, Event::ParticipantAgentActivityChanged { .. }) {
            cx.notify();
        }
    }
}

impl Focusable for AgentDebugPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).focus_handle(cx))
            .unwrap_or_else(|| cx.focus_handle())
    }
}

impl EventEmitter<PanelEvent> for AgentDebugPanel {}

impl Panel for AgentDebugPanel {
    fn persistent_name() -> &'static str {
        "Agent Debug Panel"
    }

    fn panel_key() -> &'static str {
        "AgentDebugPanel"
    }

    fn position(&self, _window: &gpui::Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(
            position,
            DockPosition::Left | DockPosition::Right | DockPosition::Bottom
        )
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut gpui::Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn size(&self, _window: &gpui::Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(400.0))
    }

    fn set_size(
        &mut self,
        size: Option<gpui::Pixels>,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &gpui::Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Debug)
    }

    fn icon_tooltip(&self, _window: &gpui::Window, _cx: &App) -> Option<&'static str> {
        Some("Agent Debug Panel")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _window: &gpui::Window, _cx: &App) -> bool {
        false
    }

    fn activation_priority(&self) -> u32 {
        100
    }
}

impl Render for AgentDebugPanel {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("AgentDebugPanel")
            .size_full()
            .child(
                h_flex()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(Label::new("Agent Activity Monitor").size(ui::LabelSize::Large)),
            )
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .child(
                        Label::new("Agent Debug Panel - Coming Soon!")
                            .color(Color::Muted)
                            .size(ui::LabelSize::Small),
                    ),
            )
    }
}



