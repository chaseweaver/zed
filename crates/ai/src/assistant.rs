use crate::{
    assistant_settings::{AssistantDockPosition, AssistantSettings},
    OpenAIRequest, OpenAIResponseStreamEvent, RequestMessage, Role,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use collections::{HashMap, HashSet};
use editor::{
    display_map::ToDisplayPoint,
    scroll::{
        autoscroll::{Autoscroll, AutoscrollStrategy},
        ScrollAnchor,
    },
    Anchor, DisplayPoint, Editor, ExcerptId, ExcerptRange, MultiBuffer,
};
use fs::Fs;
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, Stream, StreamExt};
use gpui::{
    actions,
    elements::*,
    executor::Background,
    geometry::vector::vec2f,
    platform::{CursorStyle, MouseButton},
    Action, AppContext, AsyncAppContext, ClipboardItem, Entity, ModelContext, ModelHandle,
    Subscription, Task, View, ViewContext, ViewHandle, WeakViewHandle, WindowContext,
};
use isahc::{http::StatusCode, Request, RequestExt};
use language::{language_settings::SoftWrap, Buffer, LanguageRegistry};
use serde::Deserialize;
use settings::SettingsStore;
use std::{borrow::Cow, cell::RefCell, cmp, fmt::Write, io, rc::Rc, sync::Arc, time::Duration};
use util::{post_inc, truncate_and_trailoff, ResultExt, TryFutureExt};
use workspace::{
    dock::{DockPosition, Panel},
    item::Item,
    pane, Pane, Workspace,
};

const OPENAI_API_URL: &'static str = "https://api.openai.com/v1";

actions!(
    assistant,
    [NewContext, Assist, QuoteSelection, ToggleFocus, ResetKey]
);

pub fn init(cx: &mut AppContext) {
    settings::register::<AssistantSettings>(cx);
    cx.add_action(
        |workspace: &mut Workspace, _: &NewContext, cx: &mut ViewContext<Workspace>| {
            if let Some(this) = workspace.panel::<AssistantPanel>(cx) {
                this.update(cx, |this, cx| this.add_context(cx))
            }

            workspace.focus_panel::<AssistantPanel>(cx);
        },
    );
    cx.add_action(AssistantEditor::assist);
    cx.capture_action(AssistantEditor::cancel_last_assist);
    cx.add_action(AssistantEditor::quote_selection);
    cx.capture_action(AssistantEditor::copy);
    cx.add_action(AssistantPanel::save_api_key);
    cx.add_action(AssistantPanel::reset_api_key);
}

pub enum AssistantPanelEvent {
    ZoomIn,
    ZoomOut,
    Focus,
    Close,
    DockPositionChanged,
}

pub struct AssistantPanel {
    width: Option<f32>,
    height: Option<f32>,
    pane: ViewHandle<Pane>,
    api_key: Rc<RefCell<Option<String>>>,
    api_key_editor: Option<ViewHandle<Editor>>,
    has_read_credentials: bool,
    languages: Arc<LanguageRegistry>,
    fs: Arc<dyn Fs>,
    subscriptions: Vec<Subscription>,
}

impl AssistantPanel {
    pub fn load(
        workspace: WeakViewHandle<Workspace>,
        cx: AsyncAppContext,
    ) -> Task<Result<ViewHandle<Self>>> {
        cx.spawn(|mut cx| async move {
            // TODO: deserialize state.
            workspace.update(&mut cx, |workspace, cx| {
                cx.add_view::<Self, _>(|cx| {
                    let weak_self = cx.weak_handle();
                    let pane = cx.add_view(|cx| {
                        let mut pane = Pane::new(
                            workspace.weak_handle(),
                            workspace.project().clone(),
                            workspace.app_state().background_actions,
                            Default::default(),
                            cx,
                        );
                        pane.set_can_split(false, cx);
                        pane.set_can_navigate(false, cx);
                        pane.on_can_drop(move |_, _| false);
                        pane.set_render_tab_bar_buttons(cx, move |pane, cx| {
                            let weak_self = weak_self.clone();
                            Flex::row()
                                .with_child(Pane::render_tab_bar_button(
                                    0,
                                    "icons/plus_12.svg",
                                    false,
                                    Some(("New Context".into(), Some(Box::new(NewContext)))),
                                    cx,
                                    move |_, cx| {
                                        let weak_self = weak_self.clone();
                                        cx.window_context().defer(move |cx| {
                                            if let Some(this) = weak_self.upgrade(cx) {
                                                this.update(cx, |this, cx| this.add_context(cx));
                                            }
                                        })
                                    },
                                    None,
                                ))
                                .with_child(Pane::render_tab_bar_button(
                                    1,
                                    if pane.is_zoomed() {
                                        "icons/minimize_8.svg"
                                    } else {
                                        "icons/maximize_8.svg"
                                    },
                                    pane.is_zoomed(),
                                    Some((
                                        "Toggle Zoom".into(),
                                        Some(Box::new(workspace::ToggleZoom)),
                                    )),
                                    cx,
                                    move |pane, cx| pane.toggle_zoom(&Default::default(), cx),
                                    None,
                                ))
                                .into_any()
                        });
                        let buffer_search_bar = cx.add_view(search::BufferSearchBar::new);
                        pane.toolbar()
                            .update(cx, |toolbar, cx| toolbar.add_item(buffer_search_bar, cx));
                        pane
                    });

                    let mut this = Self {
                        pane,
                        api_key: Rc::new(RefCell::new(None)),
                        api_key_editor: None,
                        has_read_credentials: false,
                        languages: workspace.app_state().languages.clone(),
                        fs: workspace.app_state().fs.clone(),
                        width: None,
                        height: None,
                        subscriptions: Default::default(),
                    };

                    let mut old_dock_position = this.position(cx);
                    this.subscriptions = vec![
                        cx.observe(&this.pane, |_, _, cx| cx.notify()),
                        cx.subscribe(&this.pane, Self::handle_pane_event),
                        cx.observe_global::<SettingsStore, _>(move |this, cx| {
                            let new_dock_position = this.position(cx);
                            if new_dock_position != old_dock_position {
                                old_dock_position = new_dock_position;
                                cx.emit(AssistantPanelEvent::DockPositionChanged);
                            }
                        }),
                    ];

                    this
                })
            })
        })
    }

    fn handle_pane_event(
        &mut self,
        _pane: ViewHandle<Pane>,
        event: &pane::Event,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            pane::Event::ZoomIn => cx.emit(AssistantPanelEvent::ZoomIn),
            pane::Event::ZoomOut => cx.emit(AssistantPanelEvent::ZoomOut),
            pane::Event::Focus => cx.emit(AssistantPanelEvent::Focus),
            pane::Event::Remove => cx.emit(AssistantPanelEvent::Close),
            _ => {}
        }
    }

    fn add_context(&mut self, cx: &mut ViewContext<Self>) {
        let focus = self.has_focus(cx);
        let editor = cx
            .add_view(|cx| AssistantEditor::new(self.api_key.clone(), self.languages.clone(), cx));
        self.subscriptions
            .push(cx.subscribe(&editor, Self::handle_assistant_editor_event));
        self.pane.update(cx, |pane, cx| {
            pane.add_item(Box::new(editor), true, focus, None, cx)
        });
    }

    fn handle_assistant_editor_event(
        &mut self,
        _: ViewHandle<AssistantEditor>,
        event: &AssistantEditorEvent,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            AssistantEditorEvent::TabContentChanged => self.pane.update(cx, |_, cx| cx.notify()),
        }
    }

    fn save_api_key(&mut self, _: &menu::Confirm, cx: &mut ViewContext<Self>) {
        if let Some(api_key) = self
            .api_key_editor
            .as_ref()
            .map(|editor| editor.read(cx).text(cx))
        {
            if !api_key.is_empty() {
                cx.platform()
                    .write_credentials(OPENAI_API_URL, "Bearer", api_key.as_bytes())
                    .log_err();
                *self.api_key.borrow_mut() = Some(api_key);
                self.api_key_editor.take();
                cx.focus_self();
                cx.notify();
            }
        } else {
            cx.propagate_action();
        }
    }

    fn reset_api_key(&mut self, _: &ResetKey, cx: &mut ViewContext<Self>) {
        cx.platform().delete_credentials(OPENAI_API_URL).log_err();
        self.api_key.take();
        self.api_key_editor = Some(build_api_key_editor(cx));
        cx.focus_self();
        cx.notify();
    }
}

fn build_api_key_editor(cx: &mut ViewContext<AssistantPanel>) -> ViewHandle<Editor> {
    cx.add_view(|cx| {
        let mut editor = Editor::single_line(
            Some(Arc::new(|theme| theme.assistant.api_key_editor.clone())),
            cx,
        );
        editor.set_placeholder_text("sk-000000000000000000000000000000000000000000000000", cx);
        editor
    })
}

impl Entity for AssistantPanel {
    type Event = AssistantPanelEvent;
}

impl View for AssistantPanel {
    fn ui_name() -> &'static str {
        "AssistantPanel"
    }

    fn render(&mut self, cx: &mut ViewContext<Self>) -> AnyElement<Self> {
        let style = &theme::current(cx).assistant;
        if let Some(api_key_editor) = self.api_key_editor.as_ref() {
            Flex::column()
                .with_child(
                    Text::new(
                        "Paste your OpenAI API key and press Enter to use the assistant",
                        style.api_key_prompt.text.clone(),
                    )
                    .aligned(),
                )
                .with_child(
                    ChildView::new(api_key_editor, cx)
                        .contained()
                        .with_style(style.api_key_editor.container)
                        .aligned(),
                )
                .contained()
                .with_style(style.api_key_prompt.container)
                .aligned()
                .into_any()
        } else {
            ChildView::new(&self.pane, cx).into_any()
        }
    }

    fn focus_in(&mut self, _: gpui::AnyViewHandle, cx: &mut ViewContext<Self>) {
        if cx.is_self_focused() {
            if let Some(api_key_editor) = self.api_key_editor.as_ref() {
                cx.focus(api_key_editor);
            } else {
                cx.focus(&self.pane);
            }
        }
    }
}

impl Panel for AssistantPanel {
    fn position(&self, cx: &WindowContext) -> DockPosition {
        match settings::get::<AssistantSettings>(cx).dock {
            AssistantDockPosition::Left => DockPosition::Left,
            AssistantDockPosition::Bottom => DockPosition::Bottom,
            AssistantDockPosition::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, _: DockPosition) -> bool {
        true
    }

    fn set_position(&mut self, position: DockPosition, cx: &mut ViewContext<Self>) {
        settings::update_settings_file::<AssistantSettings>(self.fs.clone(), cx, move |settings| {
            let dock = match position {
                DockPosition::Left => AssistantDockPosition::Left,
                DockPosition::Bottom => AssistantDockPosition::Bottom,
                DockPosition::Right => AssistantDockPosition::Right,
            };
            settings.dock = Some(dock);
        });
    }

    fn size(&self, cx: &WindowContext) -> f32 {
        let settings = settings::get::<AssistantSettings>(cx);
        match self.position(cx) {
            DockPosition::Left | DockPosition::Right => {
                self.width.unwrap_or_else(|| settings.default_width)
            }
            DockPosition::Bottom => self.height.unwrap_or_else(|| settings.default_height),
        }
    }

    fn set_size(&mut self, size: f32, cx: &mut ViewContext<Self>) {
        match self.position(cx) {
            DockPosition::Left | DockPosition::Right => self.width = Some(size),
            DockPosition::Bottom => self.height = Some(size),
        }
        cx.notify();
    }

    fn should_zoom_in_on_event(event: &AssistantPanelEvent) -> bool {
        matches!(event, AssistantPanelEvent::ZoomIn)
    }

    fn should_zoom_out_on_event(event: &AssistantPanelEvent) -> bool {
        matches!(event, AssistantPanelEvent::ZoomOut)
    }

    fn is_zoomed(&self, cx: &WindowContext) -> bool {
        self.pane.read(cx).is_zoomed()
    }

    fn set_zoomed(&mut self, zoomed: bool, cx: &mut ViewContext<Self>) {
        self.pane.update(cx, |pane, cx| pane.set_zoomed(zoomed, cx));
    }

    fn set_active(&mut self, active: bool, cx: &mut ViewContext<Self>) {
        if active {
            if self.api_key.borrow().is_none() && !self.has_read_credentials {
                self.has_read_credentials = true;
                let api_key = if let Some((_, api_key)) = cx
                    .platform()
                    .read_credentials(OPENAI_API_URL)
                    .log_err()
                    .flatten()
                {
                    String::from_utf8(api_key).log_err()
                } else {
                    None
                };
                if let Some(api_key) = api_key {
                    *self.api_key.borrow_mut() = Some(api_key);
                } else if self.api_key_editor.is_none() {
                    self.api_key_editor = Some(build_api_key_editor(cx));
                    cx.notify();
                }
            }

            if self.pane.read(cx).items_len() == 0 {
                self.add_context(cx);
            }
        }
    }

    fn icon_path(&self) -> &'static str {
        "icons/speech_bubble_12.svg"
    }

    fn icon_tooltip(&self) -> (String, Option<Box<dyn Action>>) {
        ("Assistant Panel".into(), Some(Box::new(ToggleFocus)))
    }

    fn should_change_position_on_event(event: &Self::Event) -> bool {
        matches!(event, AssistantPanelEvent::DockPositionChanged)
    }

    fn should_activate_on_event(_: &Self::Event) -> bool {
        false
    }

    fn should_close_on_event(event: &AssistantPanelEvent) -> bool {
        matches!(event, AssistantPanelEvent::Close)
    }

    fn has_focus(&self, cx: &WindowContext) -> bool {
        self.pane.read(cx).has_focus()
            || self
                .api_key_editor
                .as_ref()
                .map_or(false, |editor| editor.is_focused(cx))
    }

    fn is_focus_event(event: &Self::Event) -> bool {
        matches!(event, AssistantPanelEvent::Focus)
    }
}

enum AssistantEvent {
    MessagesEdited { ids: Vec<ExcerptId> },
    SummaryChanged,
    StreamedCompletion,
}

struct Assistant {
    buffer: ModelHandle<MultiBuffer>,
    messages: Vec<Message>,
    messages_metadata: HashMap<ExcerptId, MessageMetadata>,
    summary: Option<String>,
    pending_summary: Task<Option<()>>,
    completion_count: usize,
    pending_completions: Vec<PendingCompletion>,
    languages: Arc<LanguageRegistry>,
    model: String,
    token_count: Option<usize>,
    max_token_count: usize,
    pending_token_count: Task<Option<()>>,
    api_key: Rc<RefCell<Option<String>>>,
    _subscriptions: Vec<Subscription>,
}

impl Entity for Assistant {
    type Event = AssistantEvent;
}

impl Assistant {
    fn new(
        api_key: Rc<RefCell<Option<String>>>,
        language_registry: Arc<LanguageRegistry>,
        cx: &mut ModelContext<Self>,
    ) -> Self {
        let model = "gpt-3.5-turbo";
        let buffer = cx.add_model(|_| MultiBuffer::new(0));
        let mut this = Self {
            messages: Default::default(),
            messages_metadata: Default::default(),
            summary: None,
            pending_summary: Task::ready(None),
            completion_count: Default::default(),
            pending_completions: Default::default(),
            languages: language_registry,
            token_count: None,
            max_token_count: tiktoken_rs::model::get_context_size(model),
            pending_token_count: Task::ready(None),
            model: model.into(),
            _subscriptions: vec![cx.subscribe(&buffer, Self::handle_buffer_event)],
            api_key,
            buffer,
        };
        this.insert_message_after(ExcerptId::max(), Role::User, cx);
        this.count_remaining_tokens(cx);
        this
    }

    fn handle_buffer_event(
        &mut self,
        _: ModelHandle<MultiBuffer>,
        event: &editor::multi_buffer::Event,
        cx: &mut ModelContext<Self>,
    ) {
        match event {
            editor::multi_buffer::Event::ExcerptsAdded { .. }
            | editor::multi_buffer::Event::ExcerptsRemoved { .. }
            | editor::multi_buffer::Event::Edited => self.count_remaining_tokens(cx),
            editor::multi_buffer::Event::ExcerptsEdited { ids } => {
                cx.emit(AssistantEvent::MessagesEdited { ids: ids.clone() });
            }
            _ => {}
        }
    }

    fn count_remaining_tokens(&mut self, cx: &mut ModelContext<Self>) {
        let messages = self
            .messages
            .iter()
            .filter_map(|message| {
                Some(tiktoken_rs::ChatCompletionRequestMessage {
                    role: match self.messages_metadata.get(&message.excerpt_id)?.role {
                        Role::User => "user".into(),
                        Role::Assistant => "assistant".into(),
                        Role::System => "system".into(),
                    },
                    content: message.content.read(cx).text(),
                    name: None,
                })
            })
            .collect::<Vec<_>>();
        let model = self.model.clone();
        self.pending_token_count = cx.spawn_weak(|this, mut cx| {
            async move {
                cx.background().timer(Duration::from_millis(200)).await;
                let token_count = cx
                    .background()
                    .spawn(async move { tiktoken_rs::num_tokens_from_messages(&model, &messages) })
                    .await?;

                this.upgrade(&cx)
                    .ok_or_else(|| anyhow!("assistant was dropped"))?
                    .update(&mut cx, |this, cx| {
                        this.max_token_count = tiktoken_rs::model::get_context_size(&this.model);
                        this.token_count = Some(token_count);
                        cx.notify()
                    });
                anyhow::Ok(())
            }
            .log_err()
        });
    }

    fn remaining_tokens(&self) -> Option<isize> {
        Some(self.max_token_count as isize - self.token_count? as isize)
    }

    fn set_model(&mut self, model: String, cx: &mut ModelContext<Self>) {
        self.model = model;
        self.count_remaining_tokens(cx);
        cx.notify();
    }

    fn assist(&mut self, cx: &mut ModelContext<Self>) -> Option<(Message, Message)> {
        let messages = self
            .messages
            .iter()
            .filter_map(|message| {
                Some(RequestMessage {
                    role: self.messages_metadata.get(&message.excerpt_id)?.role,
                    content: message.content.read(cx).text(),
                })
            })
            .collect();
        let request = OpenAIRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };

        let api_key = self.api_key.borrow().clone()?;
        let stream = stream_completion(api_key, cx.background().clone(), request);
        let assistant_message = self.insert_message_after(ExcerptId::max(), Role::Assistant, cx);
        let user_message = self.insert_message_after(ExcerptId::max(), Role::User, cx);
        let task = cx.spawn_weak({
            let assistant_message = assistant_message.clone();
            |this, mut cx| async move {
                let assistant_message = assistant_message;
                let stream_completion = async {
                    let mut messages = stream.await?;

                    while let Some(message) = messages.next().await {
                        let mut message = message?;
                        if let Some(choice) = message.choices.pop() {
                            assistant_message.content.update(&mut cx, |content, cx| {
                                let text: Arc<str> = choice.delta.content?.into();
                                content.edit([(content.len()..content.len(), text)], None, cx);
                                Some(())
                            });
                            this.upgrade(&cx)
                                .ok_or_else(|| anyhow!("assistant was dropped"))?
                                .update(&mut cx, |_, cx| {
                                    cx.emit(AssistantEvent::StreamedCompletion);
                                });
                        }
                    }

                    this.upgrade(&cx)
                        .ok_or_else(|| anyhow!("assistant was dropped"))?
                        .update(&mut cx, |this, cx| {
                            this.pending_completions
                                .retain(|completion| completion.id != this.completion_count);
                            this.summarize(cx);
                        });

                    anyhow::Ok(())
                };

                let result = stream_completion.await;
                if let Some(this) = this.upgrade(&cx) {
                    this.update(&mut cx, |this, cx| {
                        if let Err(error) = result {
                            if let Some(metadata) = this
                                .messages_metadata
                                .get_mut(&assistant_message.excerpt_id)
                            {
                                metadata.error = Some(error.to_string().trim().into());
                                cx.notify();
                            }
                        }
                    });
                }
            }
        });

        self.pending_completions.push(PendingCompletion {
            id: post_inc(&mut self.completion_count),
            _task: task,
        });
        Some((assistant_message, user_message))
    }

    fn cancel_last_assist(&mut self) -> bool {
        self.pending_completions.pop().is_some()
    }

    fn remove_empty_messages<'a>(
        &mut self,
        excerpts: HashSet<ExcerptId>,
        protected_offsets: HashSet<usize>,
        cx: &mut ModelContext<Self>,
    ) {
        let mut offset = 0;
        let mut excerpts_to_remove = Vec::new();
        self.messages.retain(|message| {
            let range = offset..offset + message.content.read(cx).len();
            offset = range.end + 1;
            if range.is_empty()
                && !protected_offsets.contains(&range.start)
                && excerpts.contains(&message.excerpt_id)
            {
                excerpts_to_remove.push(message.excerpt_id);
                self.messages_metadata.remove(&message.excerpt_id);
                false
            } else {
                true
            }
        });

        if !excerpts_to_remove.is_empty() {
            self.buffer.update(cx, |buffer, cx| {
                buffer.remove_excerpts(excerpts_to_remove, cx)
            });
            cx.notify();
        }
    }

    fn cycle_message_role(&mut self, excerpt_id: ExcerptId, cx: &mut ModelContext<Self>) {
        if let Some(metadata) = self.messages_metadata.get_mut(&excerpt_id) {
            metadata.role.cycle();
            cx.notify();
        }
    }

    fn insert_message_after(
        &mut self,
        excerpt_id: ExcerptId,
        role: Role,
        cx: &mut ModelContext<Self>,
    ) -> Message {
        let content = cx.add_model(|cx| {
            let mut buffer = Buffer::new(0, "", cx);
            let markdown = self.languages.language_for_name("Markdown");
            cx.spawn_weak(|buffer, mut cx| async move {
                let markdown = markdown.await?;
                let buffer = buffer
                    .upgrade(&cx)
                    .ok_or_else(|| anyhow!("buffer was dropped"))?;
                buffer.update(&mut cx, |buffer, cx| {
                    buffer.set_language(Some(markdown), cx)
                });
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
            buffer.set_language_registry(self.languages.clone());
            buffer
        });
        let new_excerpt_id = self.buffer.update(cx, |buffer, cx| {
            buffer
                .insert_excerpts_after(
                    excerpt_id,
                    content.clone(),
                    vec![ExcerptRange {
                        context: 0..0,
                        primary: None,
                    }],
                    cx,
                )
                .pop()
                .unwrap()
        });

        let ix = self
            .messages
            .iter()
            .position(|message| message.excerpt_id == excerpt_id)
            .map_or(self.messages.len(), |ix| ix + 1);
        let message = Message {
            excerpt_id: new_excerpt_id,
            content: content.clone(),
        };
        self.messages.insert(ix, message.clone());
        self.messages_metadata.insert(
            new_excerpt_id,
            MessageMetadata {
                role,
                sent_at: Local::now(),
                error: None,
            },
        );
        message
    }

    fn summarize(&mut self, cx: &mut ModelContext<Self>) {
        if self.messages.len() >= 2 && self.summary.is_none() {
            let api_key = self.api_key.borrow().clone();
            if let Some(api_key) = api_key {
                let messages = self
                    .messages
                    .iter()
                    .take(2)
                    .filter_map(|message| {
                        Some(RequestMessage {
                            role: self.messages_metadata.get(&message.excerpt_id)?.role,
                            content: message.content.read(cx).text(),
                        })
                    })
                    .chain(Some(RequestMessage {
                        role: Role::User,
                        content:
                            "Summarize the conversation into a short title without punctuation"
                                .into(),
                    }))
                    .collect();
                let request = OpenAIRequest {
                    model: self.model.clone(),
                    messages,
                    stream: true,
                };

                let stream = stream_completion(api_key, cx.background().clone(), request);
                self.pending_summary = cx.spawn(|this, mut cx| {
                    async move {
                        let mut messages = stream.await?;

                        while let Some(message) = messages.next().await {
                            let mut message = message?;
                            if let Some(choice) = message.choices.pop() {
                                let text = choice.delta.content.unwrap_or_default();
                                this.update(&mut cx, |this, cx| {
                                    this.summary.get_or_insert(String::new()).push_str(&text);
                                    cx.emit(AssistantEvent::SummaryChanged);
                                });
                            }
                        }

                        anyhow::Ok(())
                    }
                    .log_err()
                });
            }
        }
    }
}

struct PendingCompletion {
    id: usize,
    _task: Task<()>,
}

enum AssistantEditorEvent {
    TabContentChanged,
}

struct AssistantEditor {
    assistant: ModelHandle<Assistant>,
    editor: ViewHandle<Editor>,
    scroll_bottom: ScrollAnchor,
    _subscriptions: Vec<Subscription>,
}

impl AssistantEditor {
    fn new(
        api_key: Rc<RefCell<Option<String>>>,
        language_registry: Arc<LanguageRegistry>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let assistant = cx.add_model(|cx| Assistant::new(api_key, language_registry, cx));
        let editor = cx.add_view(|cx| {
            let mut editor = Editor::for_multibuffer(assistant.read(cx).buffer.clone(), None, cx);
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor.set_show_gutter(false, cx);
            editor.set_render_excerpt_header(
                {
                    let assistant = assistant.clone();
                    move |_editor, params: editor::RenderExcerptHeaderParams, cx| {
                        enum Sender {}
                        enum ErrorTooltip {}

                        let theme = theme::current(cx);
                        let style = &theme.assistant;
                        let excerpt_id = params.id;
                        if let Some(metadata) = assistant
                            .read(cx)
                            .messages_metadata
                            .get(&excerpt_id)
                            .cloned()
                        {
                            let sender = MouseEventHandler::<Sender, _>::new(
                                params.id.into(),
                                cx,
                                |state, _| match metadata.role {
                                    Role::User => {
                                        let style = style.user_sender.style_for(state);
                                        Label::new("You", style.text.clone())
                                            .contained()
                                            .with_style(style.container)
                                    }
                                    Role::Assistant => {
                                        let style = style.assistant_sender.style_for(state);
                                        Label::new("Assistant", style.text.clone())
                                            .contained()
                                            .with_style(style.container)
                                    }
                                    Role::System => {
                                        let style = style.system_sender.style_for(state);
                                        Label::new("System", style.text.clone())
                                            .contained()
                                            .with_style(style.container)
                                    }
                                },
                            )
                            .with_cursor_style(CursorStyle::PointingHand)
                            .on_down(MouseButton::Left, {
                                let assistant = assistant.clone();
                                move |_, _, cx| {
                                    assistant.update(cx, |assistant, cx| {
                                        assistant.cycle_message_role(excerpt_id, cx)
                                    })
                                }
                            });

                            Flex::row()
                                .with_child(sender.aligned())
                                .with_child(
                                    Label::new(
                                        metadata.sent_at.format("%I:%M%P").to_string(),
                                        style.sent_at.text.clone(),
                                    )
                                    .contained()
                                    .with_style(style.sent_at.container)
                                    .aligned(),
                                )
                                .with_children(metadata.error.map(|error| {
                                    Svg::new("icons/circle_x_mark_12.svg")
                                        .with_color(style.error_icon.color)
                                        .constrained()
                                        .with_width(style.error_icon.width)
                                        .contained()
                                        .with_style(style.error_icon.container)
                                        .with_tooltip::<ErrorTooltip>(
                                            params.id.into(),
                                            error,
                                            None,
                                            theme.tooltip.clone(),
                                            cx,
                                        )
                                        .aligned()
                                }))
                                .aligned()
                                .left()
                                .contained()
                                .with_style(style.header)
                                .into_any()
                        } else {
                            Empty::new().into_any()
                        }
                    }
                },
                cx,
            );
            editor
        });

        let _subscriptions = vec![
            cx.observe(&assistant, |_, _, cx| cx.notify()),
            cx.subscribe(&assistant, Self::handle_assistant_event),
            cx.subscribe(&editor, Self::handle_editor_event),
        ];

        Self {
            assistant,
            editor,
            scroll_bottom: ScrollAnchor {
                offset: Default::default(),
                anchor: Anchor::max(),
            },
            _subscriptions,
        }
    }

    fn assist(&mut self, _: &Assist, cx: &mut ViewContext<Self>) {
        let user_message = self.assistant.update(cx, |assistant, cx| {
            let editor = self.editor.read(cx);
            let newest_selection = editor.selections.newest_anchor();
            let excerpt_id = if newest_selection.head() == Anchor::min() {
                assistant
                    .messages
                    .first()
                    .map(|message| message.excerpt_id)?
            } else if newest_selection.head() == Anchor::max() {
                assistant
                    .messages
                    .last()
                    .map(|message| message.excerpt_id)?
            } else {
                newest_selection.head().excerpt_id()
            };

            let metadata = assistant.messages_metadata.get(&excerpt_id)?;
            let user_message = if metadata.role == Role::User {
                let (_, user_message) = assistant.assist(cx)?;
                user_message
            } else {
                let user_message = assistant.insert_message_after(excerpt_id, Role::User, cx);
                user_message
            };
            Some(user_message)
        });

        if let Some(user_message) = user_message {
            self.editor.update(cx, |editor, cx| {
                let cursor = editor
                    .buffer()
                    .read(cx)
                    .snapshot(cx)
                    .anchor_in_excerpt(user_message.excerpt_id, language::Anchor::MIN);
                editor.change_selections(
                    Some(Autoscroll::Strategy(AutoscrollStrategy::Fit)),
                    cx,
                    |selections| selections.select_anchor_ranges([cursor..cursor]),
                );
            });
            self.update_scroll_bottom(cx);
        }
    }

    fn cancel_last_assist(&mut self, _: &editor::Cancel, cx: &mut ViewContext<Self>) {
        if !self
            .assistant
            .update(cx, |assistant, _| assistant.cancel_last_assist())
        {
            cx.propagate_action();
        }
    }

    fn handle_assistant_event(
        &mut self,
        _: ModelHandle<Assistant>,
        event: &AssistantEvent,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            AssistantEvent::MessagesEdited { ids } => {
                let selections = self.editor.read(cx).selections.all::<usize>(cx);
                let selection_heads = selections
                    .iter()
                    .map(|selection| selection.head())
                    .collect::<HashSet<usize>>();
                let ids = ids.iter().copied().collect::<HashSet<_>>();
                self.assistant.update(cx, |assistant, cx| {
                    assistant.remove_empty_messages(ids, selection_heads, cx)
                });
            }
            AssistantEvent::SummaryChanged => {
                cx.emit(AssistantEditorEvent::TabContentChanged);
            }
            AssistantEvent::StreamedCompletion => {
                self.editor.update(cx, |editor, cx| {
                    let snapshot = editor.snapshot(cx);
                    let scroll_bottom_row = self
                        .scroll_bottom
                        .anchor
                        .to_display_point(&snapshot.display_snapshot)
                        .row();

                    let scroll_bottom = scroll_bottom_row as f32 + self.scroll_bottom.offset.y();
                    let visible_line_count = editor.visible_line_count().unwrap_or(0.);
                    let scroll_top = scroll_bottom - visible_line_count;
                    editor
                        .set_scroll_position(vec2f(self.scroll_bottom.offset.x(), scroll_top), cx);
                });
            }
        }
    }

    fn handle_editor_event(
        &mut self,
        _: ViewHandle<Editor>,
        event: &editor::Event,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            editor::Event::ScrollPositionChanged { .. } => self.update_scroll_bottom(cx),
            _ => {}
        }
    }

    fn update_scroll_bottom(&mut self, cx: &mut ViewContext<Self>) {
        self.editor.update(cx, |editor, cx| {
            let snapshot = editor.snapshot(cx);
            let scroll_position = editor
                .scroll_manager
                .anchor()
                .scroll_position(&snapshot.display_snapshot);
            let scroll_bottom = scroll_position.y() + editor.visible_line_count().unwrap_or(0.);
            let scroll_bottom_point = cmp::min(
                DisplayPoint::new(scroll_bottom.floor() as u32, 0),
                snapshot.display_snapshot.max_point(),
            );
            let scroll_bottom_anchor = snapshot
                .buffer_snapshot
                .anchor_after(scroll_bottom_point.to_point(&snapshot.display_snapshot));
            let scroll_bottom_offset = vec2f(
                scroll_position.x(),
                scroll_bottom - scroll_bottom_point.row() as f32,
            );
            self.scroll_bottom = ScrollAnchor {
                anchor: scroll_bottom_anchor,
                offset: scroll_bottom_offset,
            };
        });
    }

    fn quote_selection(
        workspace: &mut Workspace,
        _: &QuoteSelection,
        cx: &mut ViewContext<Workspace>,
    ) {
        let Some(panel) = workspace.panel::<AssistantPanel>(cx) else {
            return;
        };
        let Some(editor) = workspace.active_item(cx).and_then(|item| item.downcast::<Editor>()) else {
            return;
        };

        let text = editor.read_with(cx, |editor, cx| {
            let range = editor.selections.newest::<usize>(cx).range();
            let buffer = editor.buffer().read(cx).snapshot(cx);
            let start_language = buffer.language_at(range.start);
            let end_language = buffer.language_at(range.end);
            let language_name = if start_language == end_language {
                start_language.map(|language| language.name())
            } else {
                None
            };
            let language_name = language_name.as_deref().unwrap_or("").to_lowercase();

            let selected_text = buffer.text_for_range(range).collect::<String>();
            if selected_text.is_empty() {
                None
            } else {
                Some(if language_name == "markdown" {
                    selected_text
                        .lines()
                        .map(|line| format!("> {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    format!("```{language_name}\n{selected_text}\n```")
                })
            }
        });

        // Activate the panel
        if !panel.read(cx).has_focus(cx) {
            workspace.toggle_panel_focus::<AssistantPanel>(cx);
        }

        if let Some(text) = text {
            panel.update(cx, |panel, cx| {
                if let Some(assistant) = panel
                    .pane
                    .read(cx)
                    .active_item()
                    .and_then(|item| item.downcast::<AssistantEditor>())
                    .ok_or_else(|| anyhow!("no active context"))
                    .log_err()
                {
                    assistant.update(cx, |assistant, cx| {
                        assistant
                            .editor
                            .update(cx, |editor, cx| editor.insert(&text, cx))
                    });
                }
            });
        }
    }

    fn copy(&mut self, _: &editor::Copy, cx: &mut ViewContext<Self>) {
        let editor = self.editor.read(cx);
        let assistant = self.assistant.read(cx);
        if editor.selections.count() == 1 {
            let selection = editor.selections.newest::<usize>(cx);
            let mut offset = 0;
            let mut copied_text = String::new();
            let mut spanned_messages = 0;
            for message in &assistant.messages {
                let message_range = offset..offset + message.content.read(cx).len() + 1;

                if message_range.start >= selection.range().end {
                    break;
                } else if message_range.end >= selection.range().start {
                    let range = cmp::max(message_range.start, selection.range().start)
                        ..cmp::min(message_range.end, selection.range().end);
                    if !range.is_empty() {
                        if let Some(metadata) = assistant.messages_metadata.get(&message.excerpt_id)
                        {
                            spanned_messages += 1;
                            write!(&mut copied_text, "## {}\n\n", metadata.role).unwrap();
                            for chunk in
                                assistant.buffer.read(cx).snapshot(cx).text_for_range(range)
                            {
                                copied_text.push_str(&chunk);
                            }
                            copied_text.push('\n');
                        }
                    }
                }

                offset = message_range.end;
            }

            if spanned_messages > 1 {
                cx.platform()
                    .write_to_clipboard(ClipboardItem::new(copied_text));
                return;
            }
        }

        cx.propagate_action();
    }

    fn cycle_model(&mut self, cx: &mut ViewContext<Self>) {
        self.assistant.update(cx, |assistant, cx| {
            let new_model = match assistant.model.as_str() {
                "gpt-4" => "gpt-3.5-turbo",
                _ => "gpt-4",
            };
            assistant.set_model(new_model.into(), cx);
        });
    }

    fn title(&self, cx: &AppContext) -> String {
        self.assistant
            .read(cx)
            .summary
            .clone()
            .unwrap_or_else(|| "New Context".into())
    }
}

impl Entity for AssistantEditor {
    type Event = AssistantEditorEvent;
}

impl View for AssistantEditor {
    fn ui_name() -> &'static str {
        "AssistantEditor"
    }

    fn render(&mut self, cx: &mut ViewContext<Self>) -> AnyElement<Self> {
        enum Model {}
        let theme = &theme::current(cx).assistant;
        let assistant = &self.assistant.read(cx);
        let model = assistant.model.clone();
        let remaining_tokens = assistant.remaining_tokens().map(|remaining_tokens| {
            let remaining_tokens_style = if remaining_tokens <= 0 {
                &theme.no_remaining_tokens
            } else {
                &theme.remaining_tokens
            };
            Label::new(
                remaining_tokens.to_string(),
                remaining_tokens_style.text.clone(),
            )
            .contained()
            .with_style(remaining_tokens_style.container)
        });

        Stack::new()
            .with_child(
                ChildView::new(&self.editor, cx)
                    .contained()
                    .with_style(theme.container),
            )
            .with_child(
                Flex::row()
                    .with_child(
                        MouseEventHandler::<Model, _>::new(0, cx, |state, _| {
                            let style = theme.model.style_for(state);
                            Label::new(model, style.text.clone())
                                .contained()
                                .with_style(style.container)
                        })
                        .with_cursor_style(CursorStyle::PointingHand)
                        .on_click(MouseButton::Left, |_, this, cx| this.cycle_model(cx)),
                    )
                    .with_children(remaining_tokens)
                    .contained()
                    .with_style(theme.model_info_container)
                    .aligned()
                    .top()
                    .right(),
            )
            .into_any()
    }

    fn focus_in(&mut self, _: gpui::AnyViewHandle, cx: &mut ViewContext<Self>) {
        if cx.is_self_focused() {
            cx.focus(&self.editor);
        }
    }
}

impl Item for AssistantEditor {
    fn tab_content<V: View>(
        &self,
        _: Option<usize>,
        style: &theme::Tab,
        cx: &gpui::AppContext,
    ) -> AnyElement<V> {
        let title = truncate_and_trailoff(&self.title(cx), editor::MAX_TAB_TITLE_LEN);
        Label::new(title, style.label.clone()).into_any()
    }

    fn tab_tooltip_text(&self, cx: &AppContext) -> Option<Cow<str>> {
        Some(self.title(cx).into())
    }

    fn as_searchable(
        &self,
        _: &ViewHandle<Self>,
    ) -> Option<Box<dyn workspace::searchable::SearchableItemHandle>> {
        Some(Box::new(self.editor.clone()))
    }
}

#[derive(Clone, Debug)]
struct Message {
    excerpt_id: ExcerptId,
    content: ModelHandle<Buffer>,
}

#[derive(Clone, Debug)]
struct MessageMetadata {
    role: Role,
    sent_at: DateTime<Local>,
    error: Option<String>,
}

async fn stream_completion(
    api_key: String,
    executor: Arc<Background>,
    mut request: OpenAIRequest,
) -> Result<impl Stream<Item = Result<OpenAIResponseStreamEvent>>> {
    request.stream = true;

    let (tx, rx) = futures::channel::mpsc::unbounded::<Result<OpenAIResponseStreamEvent>>();

    let json_data = serde_json::to_string(&request)?;
    let mut response = Request::post(format!("{OPENAI_API_URL}/chat/completions"))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .body(json_data)?
        .send_async()
        .await?;

    let status = response.status();
    if status == StatusCode::OK {
        executor
            .spawn(async move {
                let mut lines = BufReader::new(response.body_mut()).lines();

                fn parse_line(
                    line: Result<String, io::Error>,
                ) -> Result<Option<OpenAIResponseStreamEvent>> {
                    if let Some(data) = line?.strip_prefix("data: ") {
                        let event = serde_json::from_str(&data)?;
                        Ok(Some(event))
                    } else {
                        Ok(None)
                    }
                }

                while let Some(line) = lines.next().await {
                    if let Some(event) = parse_line(line).transpose() {
                        let done = event.as_ref().map_or(false, |event| {
                            event
                                .choices
                                .last()
                                .map_or(false, |choice| choice.finish_reason.is_some())
                        });
                        if tx.unbounded_send(event).is_err() {
                            break;
                        }

                        if done {
                            break;
                        }
                    }
                }

                anyhow::Ok(())
            })
            .detach();

        Ok(rx)
    } else {
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;

        #[derive(Deserialize)]
        struct OpenAIResponse {
            error: OpenAIError,
        }

        #[derive(Deserialize)]
        struct OpenAIError {
            message: String,
        }

        match serde_json::from_str::<OpenAIResponse>(&body) {
            Ok(response) if !response.error.message.is_empty() => Err(anyhow!(
                "Failed to connect to OpenAI API: {}",
                response.error.message,
            )),

            _ => Err(anyhow!(
                "Failed to connect to OpenAI API: {} {}",
                response.status(),
                body,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[gpui::test]
    fn test_inserting_and_removing_messages(cx: &mut AppContext) {
        let registry = Arc::new(LanguageRegistry::test());

        cx.add_model(|cx| {
            let mut assistant = Assistant::new(Default::default(), registry, cx);
            let message_1 = assistant.messages[0].clone();
            let message_2 = assistant.insert_message_after(ExcerptId::max(), Role::Assistant, cx);
            let message_3 = assistant.insert_message_after(message_2.excerpt_id, Role::User, cx);
            let message_4 = assistant.insert_message_after(message_2.excerpt_id, Role::User, cx);
            assistant.remove_empty_messages(
                HashSet::from_iter([message_3.excerpt_id, message_4.excerpt_id]),
                Default::default(),
                cx,
            );
            assert_eq!(assistant.messages.len(), 2);
            assert_eq!(assistant.messages[0].excerpt_id, message_1.excerpt_id);
            assert_eq!(assistant.messages[1].excerpt_id, message_2.excerpt_id);
            assistant
        });
    }
}
