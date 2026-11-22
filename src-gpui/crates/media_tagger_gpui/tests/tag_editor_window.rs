//! The tag editor in a window (epic E13, `TagEditor.test.tsx`): chips and the field in one
//! frame, a press on blank space focuses the field, removal goes through the owner,
//! disabled and loading editors refuse every write, assigned tags are never suggested,
//! Enter adds the draft and a picked suggestion is handed to the owner.

use std::{rc::Rc, sync::Arc};

use gpui::{
    div, point, prelude::*, px, Context, Entity, Focusable as _, Modifiers, SharedString,
    TestAppContext, VisualTestContext, Window, WindowHandle, WindowOptions,
};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    tagging::TagEditorVariant,
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};
use media_tagger_gpui::{
    runtime::{Runtime, RuntimeParts},
    ui::layers,
    views::tag_editor::{self, TagEditor, TagEditorEvent, TagEditorProps},
    window::init_application,
};

struct Host {
    editor: Entity<TagEditor>,
    events: Vec<TagEditorEvent>,
}

impl Host {
    fn new(
        runtime: Entity<Runtime>,
        props: TagEditorProps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| TagEditor::new(runtime, props, window, cx));
        cx.subscribe(&editor, |host, _, event, cx| {
            host.events.push(event.clone());
            cx.notify();
        })
        .detach();
        Self {
            editor,
            events: Vec::new(),
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        layers::root(cx)
            .size_full()
            .p(px(20.))
            .child(div().w(px(400.)).child(self.editor.clone()))
    }
}

struct Fixture {
    executor: Arc<ManualExecutor>,
    window: WindowHandle<Host>,
    cx: VisualTestContext,
}

fn props(tags: &[&str]) -> TagEditorProps {
    TagEditorProps {
        variant: TagEditorVariant::Bulk,
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        input_label: "Add tag".into(),
        placeholder: "Type to add tag…".into(),
        remove_label: Some(Rc::new(|tag: &str| {
            SharedString::from(format!("Remove {tag}"))
        })),
        ..TagEditorProps::default()
    }
}

impl Fixture {
    fn new(props: TagEditorProps, cx: &mut TestAppContext) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(|_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: 0,
                    offset: 0,
                    items: Vec::new(),
                })
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: vec!["cat".into(), "car".into()],
                    total: 2,
                })
            });
        let executor = Arc::new(ManualExecutor::new());
        let parts = RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend,
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: executor.clone() as Arc<dyn Executor>,
        };
        let window = cx
            .update(|cx| {
                init_application(ThemeName::Dark, cx)?;
                let runtime = cx.new(|cx| Runtime::new(parts, cx));
                runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::RefreshLibrary, cx)
                });
                cx.open_window(WindowOptions::default(), |window, cx| {
                    cx.new(|cx| Host::new(runtime, props, window, cx))
                })
            })
            .expect("open a window");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| {
            window.activate_window();
            layers::focus_root(window, cx);
        });
        let mut fixture = Self {
            executor,
            window,
            cx,
        };
        fixture.settle();
        fixture
    }

    fn settle(&mut self) {
        loop {
            self.cx.run_until_parked();
            if self.executor.run_all() == 0 {
                break;
            }
        }
        self.cx.run_until_parked();
    }

    fn editor(&mut self) -> Entity<TagEditor> {
        self.window
            .update(&mut self.cx, |host, _, _| host.editor.clone())
            .unwrap()
    }

    fn events(&mut self) -> Vec<TagEditorEvent> {
        self.window
            .update(&mut self.cx, |host, _, _| host.events.clone())
            .unwrap()
    }

    /// The events that ask for a write: additions and removals.
    fn writes(&mut self) -> Vec<TagEditorEvent> {
        self.events()
            .into_iter()
            .filter(|event| !matches!(event, TagEditorEvent::DraftChanged(_)))
            .collect()
    }

    fn set_props(&mut self, props: TagEditorProps) {
        let editor = self.editor();
        self.cx.update(|window, cx| {
            editor.update(cx, |editor, cx| editor.set_props(props, window, cx))
        });
        self.settle();
    }

    fn input_focused(&mut self) -> bool {
        let editor = self.editor();
        self.cx
            .update(|window, cx| editor.read(cx).input().focus_handle(cx).is_focused(window))
    }

    fn click(&mut self, selector: &'static str) {
        let at = self
            .cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
            .center();
        self.cx.simulate_click(at, Modifiers::none());
        self.settle();
    }

    /// A press on the frame's own padding, below its content.
    fn click_blank(&mut self) {
        let frame = self.cx.debug_bounds("bulk-tag-list").expect("the frame");
        let at = point(frame.right() - px(3.), frame.bottom() - px(3.));
        self.cx.simulate_click(at, Modifiers::none());
        self.settle();
    }

    fn type_text(&mut self, text: &str) {
        self.cx.simulate_input(text);
        self.settle();
    }

    fn key(&mut self, keystrokes: &str) {
        self.cx.simulate_keystrokes(keystrokes);
        self.settle();
    }

    fn suggestions(&mut self) -> Vec<String> {
        let editor = self.editor();
        self.cx.update(|_, cx| {
            let combobox = editor.read(cx).combobox();
            if !combobox.popup() {
                return Vec::new();
            }
            combobox
                .items()
                .iter()
                .map(|item| item.value.clone())
                .collect()
        })
    }
}

fn leak(selector: String) -> &'static str {
    Box::leak(selector.into_boxed_str())
}

#[gpui::test]
fn chips_and_the_field_share_one_frame_and_blank_space_focuses_the_field(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat"]), cx);
    let frame = fx.cx.debug_bounds("bulk-tag-list").expect("the frame");
    let chip = fx
        .cx
        .debug_bounds(leak(tag_editor::chip_selector(0)))
        .expect("a chip");
    let input = fx
        .cx
        .debug_bounds(tag_editor::INPUT_SELECTOR)
        .expect("the field");
    assert!(frame.contains(&chip.center()) && frame.contains(&input.center()));
    assert!(
        input.left() > chip.right() || input.top() >= chip.bottom(),
        "the field follows"
    );
    assert!(!fx.input_focused());

    fx.click_blank();
    assert!(fx.input_focused());
}

#[gpui::test]
fn removal_goes_to_the_owner_without_focusing_the_field(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat"]), cx);
    fx.click(leak(tag_editor::remove_selector(0)));
    assert_eq!(fx.events(), [TagEditorEvent::RemoveTag("cat".into())]);
    assert!(!fx.input_focused());
}

#[gpui::test]
fn disabled_and_loading_editors_refuse_every_write(cx: &mut TestAppContext) {
    for (disabled, loading) in [(true, None), (false, Some("Loading tags..."))] {
        let mut fx = Fixture::new(
            TagEditorProps {
                disabled,
                loading_text: loading.map(SharedString::from),
                ..props(&["cat"])
            },
            cx,
        );
        fx.click_blank();
        assert!(!fx.input_focused());
        let editor = fx.editor();
        assert!(fx
            .cx
            .update(|_, cx| editor.read(cx).input().read(cx).is_disabled()));
        if loading.is_none() {
            fx.click(leak(tag_editor::remove_selector(0)));
        } else {
            assert!(fx
                .cx
                .debug_bounds(leak(tag_editor::chip_selector(0)))
                .is_none());
        }
        fx.type_text("dog");
        fx.key("enter");
        assert!(fx.events().is_empty());
    }
}

#[gpui::test]
fn assigned_tags_are_not_suggested_and_enter_adds_the_draft_or_the_picked_tag(
    cx: &mut TestAppContext,
) {
    let mut fx = Fixture::new(
        TagEditorProps {
            remove_label: None,
            ..props(&["cat"])
        },
        cx,
    );
    assert!(fx
        .cx
        .debug_bounds(leak(tag_editor::remove_selector(0)))
        .is_none());
    fx.click_blank();
    fx.type_text("ca");
    assert_eq!(fx.suggestions(), ["car"]);
    assert!(fx.cx.debug_bounds(tag_editor::LIST_SELECTOR).is_some());

    fx.key("enter");
    assert_eq!(
        fx.events().last(),
        Some(&TagEditorEvent::AddTag("ca".into()))
    );
    fx.key("down");
    fx.key("enter");
    assert_eq!(
        fx.events().last(),
        Some(&TagEditorEvent::AddTag("car".into()))
    );
    assert_eq!(
        fx.suggestions(),
        ["car"],
        "the list stays open after a pick"
    );
}

#[gpui::test]
fn disabling_closes_the_suggestions_and_enabling_recovers(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat"]), cx);
    fx.click_blank();
    fx.type_text("ca");
    assert_eq!(fx.suggestions(), ["car"]);
    fx.set_props(TagEditorProps {
        disabled: true,
        ..props(&["cat"])
    });
    assert!(fx.suggestions().is_empty());
    assert!(fx.cx.debug_bounds(tag_editor::LIST_SELECTOR).is_none());
    fx.key("down");
    fx.key("enter");
    assert!(fx.writes().is_empty());

    fx.set_props(props(&["cat"]));
    fx.click(tag_editor::INPUT_SELECTOR);
    assert_eq!(fx.suggestions(), ["car"]);
    fx.key("enter");
    assert_eq!(fx.writes(), [TagEditorEvent::AddTag("ca".into())]);
}

#[gpui::test]
fn exclusions_follow_the_assigned_tags(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&[]), cx);
    fx.click_blank();
    fx.type_text("ca");
    assert_eq!(fx.suggestions(), ["cat", "car"]);
    fx.set_props(props(&["cat"]));
    assert_eq!(fx.suggestions(), ["car"]);
    fx.set_props(props(&[]));
    assert_eq!(fx.suggestions(), ["cat", "car"]);
}

#[gpui::test]
fn escape_closes_the_suggestions_and_keeps_the_draft(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat"]), cx);
    fx.click_blank();
    fx.type_text("ca");
    fx.key("down");
    fx.key("escape");
    assert!(fx.suggestions().is_empty());
    assert!(fx.input_focused());
    let editor = fx.editor();
    assert_eq!(
        fx.cx.update(|_, cx| editor.read(cx).draft(cx).to_owned()),
        "ca"
    );
    assert!(fx
        .events()
        .iter()
        .all(|event| matches!(event, TagEditorEvent::DraftChanged(_))));
}

impl Fixture {
    /// Enter pressed and released: GPUI's keyboard click fires on the release.
    fn activate(&mut self) {
        self.cx.simulate_keystrokes("enter");
        self.cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("a key"),
        });
        self.settle();
    }
}

/// TagEditor.test.tsx "tabs between chip removal and input and dismisses suggestions
/// without losing the draft": Tab reaches the remove button first (Enter on it asks
/// the owner to remove the tag), then the field; Escape closes the suggestions and keeps
/// the draft and the focus; Shift+Tab goes back to the remove button.
#[gpui::test]
fn tab_moves_between_the_remove_button_and_the_field(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat"]), cx);
    fx.key("tab");
    assert!(!fx.input_focused());
    fx.activate();
    assert_eq!(fx.writes(), [TagEditorEvent::RemoveTag("cat".into())]);

    fx.key("tab");
    assert!(fx.input_focused());
    fx.type_text("ca");
    assert_eq!(fx.suggestions(), ["car"]);
    fx.key("down");
    fx.key("escape");
    assert!(fx.suggestions().is_empty());
    assert!(fx.input_focused());
    let editor = fx.editor();
    assert_eq!(
        fx.cx.update(|_, cx| editor.read(cx).draft(cx).to_owned()),
        "ca"
    );

    fx.key("shift-tab");
    assert!(!fx.input_focused());
    fx.activate();
    assert_eq!(
        fx.writes(),
        [
            TagEditorEvent::RemoveTag("cat".into()),
            TagEditorEvent::RemoveTag("cat".into())
        ],
        "the focus is back on the remove button"
    );
}

/// TagEditor.test.tsx "contains chips followed by the input in one field": the field
/// has no visible label of its own and no frame of its own; only the editor's frame is
/// drawn around both, and the field fills the rest of the row after the chip.
#[gpui::test]
fn the_field_sits_inside_the_frame_after_the_chips(cx: &mut TestAppContext) {
    let mut fx = Fixture::new(props(&["cat", "car"]), cx);
    let frame = fx.cx.debug_bounds("bulk-tag-list").expect("the frame");
    let last_chip = fx
        .cx
        .debug_bounds(leak(tag_editor::chip_selector(1)))
        .expect("the second chip");
    let input = fx
        .cx
        .debug_bounds(tag_editor::INPUT_SELECTOR)
        .expect("the field");
    assert!(frame.contains(&input.origin));
    assert!(input.right() <= frame.right());
    assert!(
        input.left() >= last_chip.right() || input.top() >= last_chip.bottom(),
        "the field follows the last chip"
    );
}

/// TagEditor.tsx `revealInput` only acts on a focused field: new props or a new draft
/// from the owner leave an unfocused editor's frame where it was, and the focus brings
/// the field at the end of a long list of chips into view.
#[gpui::test]
fn only_a_focused_field_is_scrolled_into_view(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..40)
        .map(|index| format!("synthetic-{index:02}"))
        .collect();
    let tags: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut fx = Fixture::new(props(&tags), cx);
    let field_in_view = |fx: &mut Fixture| {
        let frame = fx.cx.debug_bounds("bulk-tag-list").expect("the frame");
        let input = fx
            .cx
            .debug_bounds(tag_editor::INPUT_SELECTOR)
            .expect("the field");
        input.top() >= frame.top() && input.bottom() <= frame.bottom()
    };
    assert!(!fx.input_focused());
    assert!(
        !field_in_view(&mut fx),
        "the chips fill more than the frame"
    );

    let more: Vec<&str> = tags.iter().copied().chain(["synthetic-40"]).collect();
    fx.set_props(props(&more));
    let editor = fx.editor();
    fx.cx
        .update(|window, cx| editor.update(cx, |editor, cx| editor.set_draft("dra", window, cx)));
    fx.settle();
    assert!(
        !field_in_view(&mut fx),
        "an unfocused editor keeps its scroll position"
    );
    let first = fx
        .cx
        .debug_bounds(leak(tag_editor::chip_selector(0)))
        .expect("the first chip");
    let frame = fx.cx.debug_bounds("bulk-tag-list").expect("the frame");
    assert!(first.top() >= frame.top(), "the first chips stay in view");

    let editor = fx.editor();
    fx.cx.update(|window, cx| {
        let handle = editor.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    fx.settle();
    assert!(fx.input_focused());
    assert!(field_in_view(&mut fx), "the focused field is revealed");
}
