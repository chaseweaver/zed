use channel::{Channel, ChannelMembership, ChannelStore, MessageParams};
use client::UserId;
use collections::HashMap;
use editor::{AnchorRangeExt, Editor};
use gpui::{
    elements::ChildView, AnyElement, AsyncAppContext, Element, Entity, ModelHandle, Task, View,
    ViewContext, ViewHandle, WeakViewHandle,
};
use language::{language_settings::SoftWrap, Buffer, BufferSnapshot, LanguageRegistry};
use lazy_static::lazy_static;
use project::search::SearchQuery;
use std::{sync::Arc, time::Duration};

const MENTIONS_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(50);

lazy_static! {
    static ref MENTIONS_SEARCH: SearchQuery = SearchQuery::regex(
        "@[-_\\w]+",
        false,
        false,
        Default::default(),
        Default::default()
    )
    .unwrap();
}

pub struct MessageEditor {
    pub editor: ViewHandle<Editor>,
    channel_store: ModelHandle<ChannelStore>,
    users: HashMap<String, UserId>,
    mentions: Vec<UserId>,
    mentions_task: Option<Task<()>>,
    channel: Option<Arc<Channel>>,
}

impl MessageEditor {
    pub fn new(
        language_registry: Arc<LanguageRegistry>,
        channel_store: ModelHandle<ChannelStore>,
        editor: ViewHandle<Editor>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        editor.update(cx, |editor, cx| {
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
        });

        let buffer = editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .expect("message editor must be singleton");

        cx.subscribe(&buffer, Self::on_buffer_event).detach();

        let markdown = language_registry.language_for_name("Markdown");
        cx.app_context()
            .spawn(|mut cx| async move {
                let markdown = markdown.await?;
                buffer.update(&mut cx, |buffer, cx| {
                    buffer.set_language(Some(markdown), cx)
                });
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

        Self {
            editor,
            channel_store,
            users: HashMap::default(),
            channel: None,
            mentions: Vec::new(),
            mentions_task: None,
        }
    }

    pub fn set_channel(&mut self, channel: Arc<Channel>, cx: &mut ViewContext<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_placeholder_text(format!("Message #{}", channel.name), cx);
        });
        self.channel = Some(channel);
        self.refresh_users(cx);
    }

    pub fn refresh_users(&mut self, cx: &mut ViewContext<Self>) {
        if let Some(channel) = &self.channel {
            let members = self.channel_store.update(cx, |store, cx| {
                store.get_channel_member_details(channel.id, cx)
            });
            cx.spawn(|this, mut cx| async move {
                let members = members.await?;
                this.update(&mut cx, |this, cx| this.set_members(members, cx))?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }
    }

    pub fn set_members(&mut self, members: Vec<ChannelMembership>, _: &mut ViewContext<Self>) {
        self.users.clear();
        self.users.extend(
            members
                .into_iter()
                .map(|member| (member.user.github_login.clone(), member.user.id)),
        );
    }

    pub fn take_message(&mut self, cx: &mut ViewContext<Self>) -> MessageParams {
        self.editor.update(cx, |editor, cx| {
            let highlights = editor.text_highlights::<Self>(cx);
            let text = editor.text(cx);
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let mentions = if let Some((_, ranges)) = highlights {
                ranges
                    .iter()
                    .map(|range| range.to_offset(&snapshot))
                    .zip(self.mentions.iter().copied())
                    .collect()
            } else {
                Vec::new()
            };

            editor.clear(cx);
            self.mentions.clear();

            MessageParams { text, mentions }
        })
    }

    fn on_buffer_event(
        &mut self,
        buffer: ModelHandle<Buffer>,
        event: &language::Event,
        cx: &mut ViewContext<Self>,
    ) {
        if let language::Event::Reparsed | language::Event::Edited = event {
            let buffer = buffer.read(cx).snapshot();
            self.mentions_task = Some(cx.spawn(|this, cx| async move {
                cx.background().timer(MENTIONS_DEBOUNCE_INTERVAL).await;
                Self::find_mentions(this, buffer, cx).await;
            }));
        }
    }

    async fn find_mentions(
        this: WeakViewHandle<MessageEditor>,
        buffer: BufferSnapshot,
        mut cx: AsyncAppContext,
    ) {
        let (buffer, ranges) = cx
            .background()
            .spawn(async move {
                let ranges = MENTIONS_SEARCH.search(&buffer, None).await;
                (buffer, ranges)
            })
            .await;

        this.update(&mut cx, |this, cx| {
            let mut anchor_ranges = Vec::new();
            let mut mentioned_user_ids = Vec::new();
            let mut text = String::new();

            this.editor.update(cx, |editor, cx| {
                let multi_buffer = editor.buffer().read(cx).snapshot(cx);
                for range in ranges {
                    text.clear();
                    text.extend(buffer.text_for_range(range.clone()));
                    if let Some(username) = text.strip_prefix("@") {
                        if let Some(user_id) = this.users.get(username) {
                            let start = multi_buffer.anchor_after(range.start);
                            let end = multi_buffer.anchor_after(range.end);

                            mentioned_user_ids.push(*user_id);
                            anchor_ranges.push(start..end);
                        }
                    }
                }

                editor.clear_highlights::<Self>(cx);
                editor.highlight_text::<Self>(
                    anchor_ranges,
                    theme::current(cx).chat_panel.rich_text.mention_highlight,
                    cx,
                )
            });

            this.mentions = mentioned_user_ids;
            this.mentions_task.take();
        })
        .ok();
    }
}

impl Entity for MessageEditor {
    type Event = ();
}

impl View for MessageEditor {
    fn render(&mut self, cx: &mut ViewContext<'_, '_, Self>) -> AnyElement<Self> {
        ChildView::new(&self.editor, cx).into_any()
    }

    fn focus_in(&mut self, _: gpui::AnyViewHandle, cx: &mut ViewContext<Self>) {
        if cx.is_self_focused() {
            cx.focus(&self.editor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use client::{Client, User, UserStore};
    use gpui::{TestAppContext, WindowHandle};
    use language::{Language, LanguageConfig};
    use rpc::proto;
    use settings::SettingsStore;
    use util::{http::FakeHttpClient, test::marked_text_ranges};

    #[gpui::test]
    async fn test_message_editor(cx: &mut TestAppContext) {
        let editor = init_test(cx);
        let editor = editor.root(cx);

        editor.update(cx, |editor, cx| {
            editor.set_members(
                vec![
                    ChannelMembership {
                        user: Arc::new(User {
                            github_login: "a-b".into(),
                            id: 101,
                            avatar: None,
                        }),
                        kind: proto::channel_member::Kind::Member,
                        role: proto::ChannelRole::Member,
                    },
                    ChannelMembership {
                        user: Arc::new(User {
                            github_login: "C_D".into(),
                            id: 102,
                            avatar: None,
                        }),
                        kind: proto::channel_member::Kind::Member,
                        role: proto::ChannelRole::Member,
                    },
                ],
                cx,
            );

            editor.editor.update(cx, |editor, cx| {
                editor.set_text("Hello, @a-b! Have you met @C_D?", cx)
            });
        });

        cx.foreground().advance_clock(MENTIONS_DEBOUNCE_INTERVAL);

        editor.update(cx, |editor, cx| {
            let (text, ranges) = marked_text_ranges("Hello, «@a-b»! Have you met «@C_D»?", false);
            assert_eq!(
                editor.take_message(cx),
                MessageParams {
                    text,
                    mentions: vec![(ranges[0].clone(), 101), (ranges[1].clone(), 102)],
                }
            );
        });
    }

    fn init_test(cx: &mut TestAppContext) -> WindowHandle<MessageEditor> {
        cx.foreground().forbid_parking();

        cx.update(|cx| {
            let http = FakeHttpClient::with_404_response();
            let client = Client::new(http.clone(), cx);
            let user_store = cx.add_model(|cx| UserStore::new(client.clone(), http, cx));
            cx.set_global(SettingsStore::test(cx));
            theme::init((), cx);
            language::init(cx);
            editor::init(cx);
            client::init(&client, cx);
            channel::init(&client, user_store, cx);
        });

        let language_registry = Arc::new(LanguageRegistry::test());
        language_registry.add(Arc::new(Language::new(
            LanguageConfig {
                name: "Markdown".into(),
                ..Default::default()
            },
            Some(tree_sitter_markdown::language()),
        )));

        let editor = cx.add_window(|cx| {
            MessageEditor::new(
                language_registry,
                ChannelStore::global(cx),
                cx.add_view(|cx| Editor::auto_height(4, None, cx)),
                cx,
            )
        });
        cx.foreground().run_until_parked();
        editor
    }
}
