use std::borrow::Cow;
use std::cell::Cell;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::BufWriter;
use std::io::Write;
use std::mem;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use ashpd::WindowIdentifier;
use ashpd::desktop::file_chooser::Choice;
use ashpd::desktop::file_chooser::SaveFileRequest;
use bytes::Bytes;
use glib::Properties;
use glib::clone;
use gtk::CompositeTemplate;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use sourceview5::prelude::*;

use crate::engine;
use crate::expression::ExpressionExt;
use crate::gettext::gettext;
use crate::log_buffer::LogBuffer;
use crate::log_buffer::MAX_LOG_BYTES;
use crate::log_buffer::MAX_LOG_CHUNK_BYTES;
use crate::log_buffer::MAX_LOG_LINES;
use crate::model;
use crate::rt;
use crate::utils;
use crate::widget;

const ACTION_TOGGLE_SEARCH: &str = "container-log-page.toggle-search";
const ACTION_EXIT_SEARCH: &str = "container-log-page.exit-search";
const ACTION_SAVE_TO_FILE: &str = "container-log-page.save-to-file";
const ACTION_SHOW_TIMESTAMPS: &str = "container-log-page.show-timestamps";
const ACTION_START_CONTAINER: &str = "container-log-page.start-container";
const ACTION_ZOOM_OUT: &str = "container-log-page.zoom-out";
const ACTION_ZOOM_IN: &str = "container-log-page.zoom-in";
const ACTION_ZOOM_NORMAL: &str = "container-log-page.zoom-normal";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FetchLinesState {
    #[default]
    Waiting,
    Fetching,
    Finished,
}

// Preserve the daemon's nanoseconds: GLib's DateTime alone has microsecond
// precision, and the Docker logs request accepts a cursor in whole seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LogTimestamp {
    seconds: i64,
    nanoseconds: u32,
}

impl LogTimestamp {
    fn parse(value: &str) -> Option<Self> {
        let date = glib::DateTime::from_iso8601(value, None).ok()?;
        let fraction = value
            .split_once('.')
            .map(|(_, fraction)| {
                fraction
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if fraction.len() > 9 {
            return None;
        }
        let nanoseconds = fraction
            .iter()
            .fold(0, |value, digit| value * 10 + u32::from(digit - b'0'))
            * 10_u32.pow(9 - fraction.len() as u32);
        Some(Self {
            seconds: date.to_unix(),
            nanoseconds,
        })
    }

    fn from_line(line: &[u8]) -> Option<Self> {
        let end = line.iter().position(|byte| *byte == b' ')?;
        Self::parse(std::str::from_utf8(&line[..end]).ok()?)
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::ContainerLogPage)]
    #[template(resource = "/com/github/marhkb/Pods/ui/view/container_log_page.ui")]
    pub(crate) struct ContainerLogPage {
        pub(super) settings: utils::PodsSettings,
        pub(super) log_timestamps: RefCell<VecDeque<String>>,
        pub(super) log_sizes: RefCell<VecDeque<usize>>,
        pub(super) log_bytes: Cell<usize>,
        pub(super) follow: RefCell<Option<rt::PipeFinish>>,
        pub(super) fetch_until: OnceCell<LogTimestamp>,
        pub(super) latest_timestamp: Cell<Option<LogTimestamp>>,
        pub(super) fetch_lines_state: Cell<FetchLinesState>,
        pub(super) fetched_lines: RefCell<LogBuffer>,

        #[property(get, set, construct, nullable)]
        pub(super) container: glib::WeakRef<model::Container>,

        #[property(get, set)]
        pub(super) show_timestamps: Cell<bool>,
        #[property(get, set)]
        pub(super) sticky: Cell<bool>,

        #[template_child]
        pub(super) zoom_control: TemplateChild<widget::ZoomControl>,
        #[template_child]
        pub(super) timestamps_renderer: TemplateChild<sourceview5::GutterRendererText>,
        #[template_child]
        pub(super) menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub(super) search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub(super) search_widget: TemplateChild<widget::SourceViewSearchWidget>,
        #[template_child]
        pub(super) banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub(super) history_limit_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) lines_loading_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub(super) auto_scrolled_window: TemplateChild<widget::AutoScrolledWindow>,
        #[template_child]
        pub(super) scalable_text_view: TemplateChild<widget::ScalableTextView>,
        #[template_child]
        pub(super) source_buffer: TemplateChild<sourceview5::Buffer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ContainerLogPage {
        const NAME: &'static str = "PdsContainerLogPage";
        type Type = super::ContainerLogPage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();

            klass.add_binding_action(
                gdk::Key::F,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_TOGGLE_SEARCH,
            );
            klass.install_action(ACTION_TOGGLE_SEARCH, None, |widget, _, _| {
                widget.toggle_search_mode();
            });

            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                ACTION_EXIT_SEARCH,
            );
            klass.install_action(ACTION_EXIT_SEARCH, None, |widget, _, _| {
                widget.set_search_mode(false);
            });

            klass.install_action_async(ACTION_SAVE_TO_FILE, None, |widget, _, _| async move {
                widget.save_to_file().await;
            });
            klass.install_property_action(ACTION_SHOW_TIMESTAMPS, "show-timestamps");

            klass.install_action(ACTION_START_CONTAINER, None, |widget, _, _| {
                widget.start_or_resume_container();
            });

            klass.install_action(ACTION_ZOOM_OUT, None, |widget, _, _| {
                widget.imp().scalable_text_view.zoom_out();
            });
            klass.install_action(ACTION_ZOOM_IN, None, |widget, _, _| {
                widget.imp().scalable_text_view.zoom_in();
            });
            klass.install_action(ACTION_ZOOM_NORMAL, None, |widget, _, _| {
                widget.imp().scalable_text_view.zoom_normal();
            });

            klass.add_binding_action(
                gdk::Key::minus,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_OUT,
            );
            klass.add_binding_action(
                gdk::Key::KP_Subtract,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_OUT,
            );

            klass.add_binding_action(
                gdk::Key::plus,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_IN,
            );
            klass.add_binding_action(
                gdk::Key::KP_Add,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_IN,
            );
            klass.add_binding_action(
                gdk::Key::equal,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_IN,
            );

            klass.add_binding_action(
                gdk::Key::_0,
                gdk::ModifierType::CONTROL_MASK,
                ACTION_ZOOM_NORMAL,
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ContainerLogPage {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec);
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = &*self.obj();

            self.settings
                .bind("show-log-timestamps", obj, "show-timestamps")
                .build();

            self.menu_button
                .popover()
                .unwrap()
                .downcast::<gtk::PopoverMenu>()
                .unwrap()
                .add_child(&*self.zoom_control, "zoom-control");

            let adw_style_manager = adw::StyleManager::default();
            self.on_notify_dark(&adw_style_manager);
            adw_style_manager.connect_dark_notify(clone!(
                #[weak]
                obj,
                move |style_manager| {
                    obj.imp().on_notify_dark(style_manager);
                }
            ));

            <widget::ScalableTextView as ViewExt>::gutter(
                &*self.scalable_text_view,
                gtk::TextWindowType::Left,
            )
            .insert(&self.timestamps_renderer.get(), 0);

            let mut maybe_gutter_child = <widget::ScalableTextView as ViewExt>::gutter(
                &*self.scalable_text_view,
                gtk::TextWindowType::Left,
            )
            .first_child();

            while let Some(child) = maybe_gutter_child {
                if child.is::<sourceview5::GutterRenderer>() {
                    child.set_margin_start(4);
                }

                maybe_gutter_child = child.next_sibling()
            }

            Self::Type::this_expression("container")
                .chain_property::<model::Container>("status")
                .map(|status: model::ContainerStatus| status != model::ContainerStatus::Running)
                .bind(&*self.banner, "revealed", Some(obj));

            if let Some(container) = obj.container() {
                container.connect_notify_local(
                    Some("status"),
                    clone!(
                        #[weak]
                        obj,
                        move |container, _| {
                            if container.status() == model::ContainerStatus::Running {
                                obj.imp().follow_log();
                            }
                        }
                    ),
                );
            }

            self.follow_log();
        }

        fn dispose(&self) {
            if let Some(follow) = self.follow.borrow_mut().take() {
                follow.abort();
            }
        }
    }

    impl WidgetImpl for ContainerLogPage {}

    impl NavigationPageImpl for ContainerLogPage {}

    #[gtk::template_callbacks]
    impl ContainerLogPage {
        fn follow_log(&self) {
            let obj = &*self.obj();

            if let Some(follow) = self.follow.borrow_mut().take() {
                follow.abort();
            }

            let Some(api) = obj.container().and_then(|container| container.api()) else {
                return;
            };

            let resume_after = self.latest_timestamp.get();
            let mut opts = basic_opts(true, true);
            if let Some(timestamp) = resume_after {
                opts.since = timestamp.seconds;
            } else {
                // Read the initial history and follow it on the same connection.
                // A separate tail request followed by a second request can replay
                // a partial second and skip new records in multi-line frames.
                opts.tail = "512".to_owned();
            }
            self.stack.set_visible_child_name("loaded");
            let mut perform = MarkupPerform::default();

            let follow = rt::Pipe::new(api, move |api| api.logs(opts)).on_next(clone!(
                #[weak]
                obj,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move |result: anyhow::Result<engine::conn::TtyChunk>| {
                    match result {
                        Ok(chunk) => {
                            obj.imp()
                                .insert_after(chunk.into(), &mut perform, true, resume_after);
                            glib::ControlFlow::Continue
                        }
                        Err(error) => {
                            log::warn!("Stopping container log stream due to error: {error}");
                            utils::show_error_toast(
                                &obj,
                                &gettext("Could not follow log"),
                                &error.to_string(),
                            );
                            glib::ControlFlow::Break
                        }
                    }
                }
            ));
            self.follow.replace(Some(follow));
        }

        pub(super) fn insert(&self, line: Bytes, perform: &mut MarkupPerform, at_end: bool) {
            self.insert_after(line, perform, at_end, None);
        }

        fn insert_after(
            &self,
            line: Bytes,
            perform: &mut MarkupPerform,
            at_end: bool,
            resume_after: Option<LogTimestamp>,
        ) {
            let line = if line.len() > MAX_LOG_CHUNK_BYTES {
                self.history_limit_banner.set_revealed(true);
                &line[..MAX_LOG_CHUNK_BYTES]
            } else {
                line.as_ref()
            };
            let mut lines: Vec<_> = line.split(|byte| *byte == b'\n').collect();
            if lines.last().is_some_and(|line| line.is_empty()) {
                lines.pop();
            }
            if !at_end {
                lines.reverse();
            }
            for line in lines {
                if resume_after.is_some_and(|cursor| {
                    LogTimestamp::from_line(line).is_some_and(|timestamp| timestamp <= cursor)
                }) {
                    continue;
                }
                self.insert_line(line, perform, at_end);
            }
        }

        fn insert_line(&self, line: &[u8], perform: &mut MarkupPerform, at_end: bool) {
            let line_buffer = perform.decode(line.as_ref());
            let (timestamp, log_message) = line_buffer.split_once(' ').unwrap();

            if let Some(timestamp) = LogTimestamp::parse(timestamp) {
                self.fetch_until.get_or_init(|| timestamp);
                if at_end {
                    self.latest_timestamp.set(Some(timestamp));
                }
            }

            let source_buffer = &*self.source_buffer;
            source_buffer.insert_markup(
                &mut if at_end {
                    self.source_buffer.end_iter()
                } else {
                    self.source_buffer.start_iter()
                },
                &if self.log_timestamps.borrow().is_empty() {
                    Cow::Borrowed(log_message)
                } else if at_end {
                    Cow::Owned(format!("\n{log_message}"))
                } else {
                    Cow::Owned(format!("{log_message}\n"))
                },
            );

            let mut timestamps = self.log_timestamps.borrow_mut();
            let mut sizes = self.log_sizes.borrow_mut();
            if at_end {
                timestamps.push_back(timestamp.to_owned());
                sizes.push_back(line.len());
            } else {
                timestamps.push_front(timestamp.to_owned());
                sizes.push_front(line.len());
            }
            self.log_bytes.set(self.log_bytes.get() + line.len());

            while timestamps.len() > MAX_LOG_LINES || self.log_bytes.get() > MAX_LOG_BYTES {
                let size = if at_end {
                    timestamps.pop_front();
                    let mut start = source_buffer.start_iter();
                    let mut end = source_buffer
                        .iter_at_line(1)
                        .unwrap_or_else(|| source_buffer.end_iter());
                    source_buffer.delete(&mut start, &mut end);
                    sizes.pop_front()
                } else {
                    timestamps.pop_back();
                    let mut start = source_buffer
                        .iter_at_line(timestamps.len() as i32)
                        .unwrap_or_else(|| source_buffer.end_iter());
                    start.backward_char();
                    let mut end = source_buffer.end_iter();
                    source_buffer.delete(&mut start, &mut end);
                    sizes.pop_back()
                };
                self.log_bytes.set(
                    self.log_bytes
                        .get()
                        .saturating_sub(size.unwrap_or_default()),
                );
                self.history_limit_banner.set_revealed(true);
            }
        }

        fn on_notify_dark(&self, style_manager: &adw::StyleManager) {
            self.source_buffer.set_style_scheme(
                sourceview5::StyleSchemeManager::default()
                    .scheme(if style_manager.is_dark() {
                        "Adwaita-dark"
                    } else {
                        "Adwaita"
                    })
                    .as_ref(),
            );
        }

        #[template_callback]
        fn on_timestamps_renderer_query_data(&self, _: &glib::Object, line: u32) {
            if let Some(timestamp) = self.log_timestamps.borrow().get(line as usize) {
                let date_time = format!(
                    "<span foreground=\"#865e3c\">{}</span>",
                    glib::markup_escape_text(timestamp)
                );
                self.timestamps_renderer.set_markup(&date_time);

                let (width, _) = self.timestamps_renderer.measure_markup(&date_time);
                self.timestamps_renderer
                    .set_width_request(width.max(self.timestamps_renderer.width_request()));
            }
        }

        #[template_callback]
        fn on_scroll(
            &self,
            _dx: f64,
            dy: f64,
            scroll: gtk::EventControllerScroll,
        ) -> glib::Propagation {
            if scroll.current_event_state() == gdk::ModifierType::CONTROL_MASK {
                let text_view = &*self.scalable_text_view;
                if dy.is_sign_negative() {
                    text_view.zoom_in();
                } else {
                    text_view.zoom_out();
                }
            }

            glib::Propagation::Proceed
        }

        #[template_callback]
        fn on_scrolled_up(&self) {
            let obj = &*self.obj();

            match self.fetch_lines_state.get() {
                FetchLinesState::Waiting => {
                    let Some(until) = self.fetch_until.get().copied() else {
                        return;
                    };

                    let Some(api) = obj.container().and_then(|container| container.api()) else {
                        return;
                    };

                    self.lines_loading_revealer.set_reveal_child(true);
                    self.fetch_lines_state.set(FetchLinesState::Fetching);
                    self.history_limit_banner.set_revealed(true);

                    rt::Pipe::new(api, move |api| {
                        api.logs(engine::opts::LogsOpts {
                            // Include the partial first second, then discard the
                            // records already shown using their precise timestamps.
                            until: until.seconds + 1,
                            tail: MAX_LOG_LINES.to_string(),
                            ..basic_opts(false, true)
                        })
                    })
                    .on_next(clone!(
                        #[weak]
                        obj,
                        #[upgrade_or]
                        glib::ControlFlow::Break,
                        move |result| {
                            let imp = obj.imp();
                            imp.fetch_lines_state.set(FetchLinesState::Fetching);

                            match result {
                                Ok(chunk) => {
                                    let chunk = Bytes::from(chunk);
                                    let mut history = imp.fetched_lines.borrow_mut();
                                    for line in chunk.split(|byte| *byte == b'\n') {
                                        if line.is_empty()
                                            || LogTimestamp::from_line(line)
                                                .is_some_and(|timestamp| timestamp >= until)
                                        {
                                            continue;
                                        }
                                        let clipped = line.len() > MAX_LOG_CHUNK_BYTES;
                                        let line = &line[..line.len().min(MAX_LOG_CHUNK_BYTES)];
                                        if history.push_back(Bytes::copy_from_slice(line))
                                            || clipped
                                        {
                                            imp.history_limit_banner.set_revealed(true);
                                        }
                                    }
                                    glib::ControlFlow::Continue
                                }
                                Err(e) => {
                                    log::warn!("Stopping container log stream due to error: {e}");
                                    glib::ControlFlow::Break
                                }
                            }
                        }
                    ))
                    .on_finish(clone!(
                        #[weak]
                        obj,
                        move || {
                            let imp = obj.imp();
                            imp.lines_loading_revealer.set_reveal_child(false);
                            imp.fetch_lines_state.set(FetchLinesState::Finished);

                            imp.move_lines_to_buffer();
                        }
                    ));
                }
                FetchLinesState::Finished => self.move_lines_to_buffer(),
                _ => {}
            }
        }

        fn move_lines_to_buffer(&self) {
            let mut perform = MarkupPerform::default();

            let mut lines = self.fetched_lines.borrow_mut();

            let had_lines = !lines.is_empty();

            for _ in 0..128 {
                match lines.pop_back() {
                    Some(line) => self.insert(line, &mut perform, false),
                    None => break,
                }
            }

            if had_lines {
                let adj = self.auto_scrolled_window.vadjustment();
                if adj.value() < 30.0 {
                    adj.set_value(adj.value() + 30.0);
                }
            }
        }

        #[template_callback]
        fn on_source_buffer_cursor_moved(&self) {
            self.timestamps_renderer.queue_draw();
        }

        #[template_callback]
        fn on_search_bar_search_mode_enabled(&self) {
            if self.search_bar.is_search_mode() {
                self.search_widget.grab_focus();
            } else {
                self.search_widget.set_text("");
            }
        }
    }
}

glib::wrapper! {
    pub(crate) struct ContainerLogPage(ObjectSubclass<imp::ContainerLogPage>)
        @extends gtk::Widget, adw::NavigationPage,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::Container> for ContainerLogPage {
    fn from(container: &model::Container) -> Self {
        glib::Object::builder()
            .property("container", container)
            .build()
    }
}

impl ContainerLogPage {
    #[cfg(test)]
    pub(crate) fn check_log_window_for_test(&self) {
        let imp = self.imp();
        let mut perform = MarkupPerform::default();
        for index in 0..MAX_LOG_LINES + 128 {
            imp.insert(
                Bytes::from(format!("2026-11-24T05:00:00.000000001Z log {index:06}\n")),
                &mut perform,
                true,
            );
        }
        let buffer = &*imp.source_buffer;
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        assert!(text.starts_with("log 000128\n"));
        assert!(text.ends_with("log 010127"));
        assert_eq!(
            buffer.line_count() as usize,
            imp.log_timestamps.borrow().len()
        );
        assert!(!buffer.enables_undo());
        assert!(!buffer.can_undo());

        // Exercise the byte limit with the real GtkTextBuffer delete/insert path,
        // including lines inserted at the top while reading older history.
        let payload = "x".repeat(1024);
        for _ in 0..(MAX_LOG_BYTES / payload.len()) + 128 {
            imp.insert(
                Bytes::from(format!("2026-11-24T05:00:00.000000002Z {payload}\n")),
                &mut perform,
                true,
            );
        }
        assert!(imp.log_bytes.get() <= MAX_LOG_BYTES);
        assert!(buffer.char_count() as usize <= MAX_LOG_BYTES);
        assert_eq!(
            buffer.line_count() as usize,
            imp.log_timestamps.borrow().len()
        );
        assert!(imp.history_limit_banner.is_revealed());
        let latest = imp.latest_timestamp.get();
        imp.insert(
            Bytes::from_static(b"2026-11-24T04:59:59Z older <tag>&\n"),
            &mut perform,
            false,
        );
        assert!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .starts_with("older <tag>&\n")
        );
        assert_eq!(imp.latest_timestamp.get(), latest);
        assert_eq!(
            buffer.line_count() as usize,
            imp.log_timestamps.borrow().len()
        );
        assert!(!buffer.can_undo());
    }

    pub(crate) async fn save_to_file(&self) {
        let Some(container) = self.container() else {
            return;
        };

        let request = SaveFileRequest::default()
            .identifier(WindowIdentifier::from_native(&self.native().unwrap()).await)
            .current_name(format!("{}.log", container.name()).as_str())
            .choice(Choice::boolean(
                "timestamps",
                &gettext("Include timestamps"),
                false,
            ))
            .modal(true);

        utils::show_save_file_dialog(
            request,
            self,
            clone!(
                #[weak(rename_to = obj)]
                self,
                move |files| {
                    let Some(uri) = files.uris().first() else {
                        return;
                    };
                    let file = gio::File::for_uri(uri.as_str());

                    let Some(path) = file.path() else { return };

                    let file = match std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(path)
                    {
                        Ok(file) => file,
                        Err(error) => {
                            utils::show_error_toast(
                                &obj,
                                &gettext("Could not save logs"),
                                &error.to_string(),
                            );
                            return;
                        }
                    };

                    obj.action_set_enabled(ACTION_SAVE_TO_FILE, false);
                    let writer = Rc::new(RefCell::new(BufWriter::new(file)));
                    let failed = Rc::new(Cell::new(false));
                    let mut perform = PlainTextPerform::default();
                    let mut parser = vte::Parser::new();

                    let timestamps = files
                        .choices()
                        .iter()
                        .any(|(name, value)| name == "timestamps" && value == "true");

                    rt::Pipe::new(container.api().unwrap(), move |container| {
                        container.logs(basic_opts(false, timestamps))
                    })
                    .on_next(clone!(
                        #[weak]
                        obj,
                        #[strong]
                        writer,
                        #[strong]
                        failed,
                        #[upgrade_or]
                        glib::ControlFlow::Break,
                        move |result: anyhow::Result<engine::conn::TtyChunk>| {
                            match result {
                                Ok(line) => {
                                    parser.advance(&mut perform, line.as_ref());

                                    let line = perform.move_out_buffer();
                                    if !line.is_empty() {
                                        let mut writer = writer.borrow_mut();
                                        match writer.write_all(line.as_bytes()) {
                                            Ok(_) => glib::ControlFlow::Continue,
                                            Err(e) => {
                                                failed.set(true);
                                                log::warn!("Error on saving logs: {e}");
                                                utils::show_error_toast(
                                                    &obj,
                                                    &gettext("Could not save logs"),
                                                    &e.to_string(),
                                                );
                                                glib::ControlFlow::Break
                                            }
                                        }
                                    } else {
                                        glib::ControlFlow::Continue
                                    }
                                }
                                Err(e) => {
                                    failed.set(true);
                                    log::warn!("Error on retrieving logs: {e}");
                                    utils::show_error_toast(
                                        &obj,
                                        &gettext("Could not retrieve logs"),
                                        &e.to_string(),
                                    );
                                    glib::ControlFlow::Break
                                }
                            }
                        }
                    ))
                    .on_finish(clone!(
                        #[weak]
                        obj,
                        move || {
                            obj.action_set_enabled(ACTION_SAVE_TO_FILE, true);
                            if failed.get() {
                                return;
                            }
                            match writer.borrow_mut().flush() {
                                Ok(()) => utils::show_toast(&obj, gettext("Log has been saved")),
                                Err(error) => utils::show_error_toast(
                                    &obj,
                                    &gettext("Could not save logs"),
                                    &error.to_string(),
                                ),
                            }
                        }
                    ));
                }
            ),
        )
        .await;
    }

    pub(crate) fn set_search_mode(&self, value: bool) {
        self.imp().search_bar.set_search_mode(value);
    }

    pub(crate) fn toggle_search_mode(&self) {
        self.set_search_mode(!self.imp().search_bar.is_search_mode());
    }

    pub(crate) fn start_or_resume_container(&self) {
        let Some(container) = self.container() else {
            return;
        };

        if container.status().can_start() {
            container.start(clone!(
                #[weak(rename_to = obj)]
                self,
                move |result| {
                    if let Err(e) = result {
                        utils::show_error_toast(
                            &obj,
                            &gettext("Could not start container"),
                            &e.to_string(),
                        );
                    }
                }
            ));
        } else if container.status().can_resume() {
            container.resume(clone!(
                #[weak(rename_to = obj)]
                self,
                move |result| {
                    if let Err(e) = result {
                        utils::show_error_toast(
                            &obj,
                            &gettext("Could not resume container"),
                            &e.to_string(),
                        );
                    }
                }
            ));
        }
    }
}

fn basic_opts(follow: bool, timestamps: bool) -> engine::opts::LogsOpts {
    engine::opts::LogsOpts {
        follow,
        stdout: true,
        stderr: true,
        timestamps,
        ..Default::default()
    }
}

#[derive(Debug)]
enum MarkupAttribute {
    Bold,
    Foreground(&'static str),
    Background(&'static str),
}

impl MarkupAttribute {
    fn open_tag(&'_ self) -> Cow<'_, str> {
        match self {
            Self::Bold => Cow::Borrowed("<b>"),
            Self::Foreground(value) => Cow::Owned(format!("<span foreground=\"{value}\">")),
            Self::Background(value) => Cow::Owned(format!("<span background=\"{value}\">")),
        }
    }

    fn close_tag(&self) -> &'static str {
        match self {
            Self::Bold => "</b>",
            Self::Foreground(_) | Self::Background(_) => "</span>",
        }
    }
}

#[derive(Debug, Default)]
pub struct MarkupPerform {
    buffer: String,
    attributes: Vec<MarkupAttribute>,
}

impl MarkupPerform {
    fn move_out_buffer(&mut self) -> String {
        let mut buffer = String::new();
        mem::swap(&mut self.buffer, &mut buffer);
        buffer
    }

    fn begin_line(&mut self) {
        self.attributes.iter().for_each(|attr| {
            self.buffer.push_str(attr.open_tag().as_ref());
        });
    }

    fn end_line(&mut self) {
        self.attributes.iter().rev().for_each(|attr| {
            self.buffer.push_str(attr.close_tag());
        });
    }

    fn reset_all(&mut self) {
        while let Some(attr) = self.attributes.pop() {
            self.buffer.push_str(attr.close_tag());
        }
    }

    fn reset<F: Fn(&MarkupAttribute) -> bool>(&mut self, op: F) {
        let mut t = Vec::new();
        while let Some(attr) = self.attributes.pop() {
            self.buffer.push_str(attr.close_tag());
            if op(&attr) {
                t.insert(0, attr);
            }
        }

        mem::swap(&mut t, &mut self.attributes);

        self.begin_line();
    }

    /// Decode the specified bytes. Return true if finished.
    fn decode(&mut self, ansi_encoded_bytes: &[u8]) -> String {
        let mut parser = vte::Parser::new();

        self.begin_line();

        let line = String::from_utf8_lossy(ansi_encoded_bytes);
        let (timestamp, message) = line
            .split_once(' ')
            .filter(|(timestamp, _)| glib::DateTime::from_iso8601(timestamp, None).is_ok())
            .unwrap_or(("", &line));

        parser.advance(self, message.as_bytes());

        self.end_line();

        format!("{timestamp} {}", self.move_out_buffer())
    }
}

impl vte::Perform for MarkupPerform {
    fn print(&mut self, c: char) {
        match c {
            '&' => self.buffer.push_str("&amp;"),
            '<' => self.buffer.push_str("&lt;"),
            '>' => self.buffer.push_str("&gt;"),
            _ => self.buffer.push(c),
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        for param in params.iter() {
            param.iter().copied().for_each(|param| {
                match param {
                    0 => self.reset_all(),
                    39 => {
                        // Reset foreground
                        self.reset(|attr| !matches!(attr, MarkupAttribute::Foreground(_)));
                    }
                    49 => {
                        // Reset background
                        self.reset(|attr| !matches!(attr, MarkupAttribute::Background(_)));
                    }
                    _ => {
                        if let Some(attr) = ansi_escape_to_markup_attribute(param) {
                            // Repeated SGR colour/bold sequences must not grow the
                            // active attribute stack for the lifetime of a log stream.
                            self.reset(|old| {
                                std::mem::discriminant(old) != std::mem::discriminant(&attr)
                            });
                            self.buffer.push_str(attr.open_tag().as_ref());
                            self.attributes.push(attr);
                        }
                    }
                }
            });
        }
    }
}

fn ansi_escape_to_markup_attribute(item: u16) -> Option<MarkupAttribute> {
    Some(match item {
        1 => MarkupAttribute::Bold,

        30 => MarkupAttribute::Foreground("#000000"),
        31 => MarkupAttribute::Foreground("#e01b24"),
        32 => MarkupAttribute::Foreground("#33d17a"),
        33 => MarkupAttribute::Foreground("#f6d32d"),
        34 => MarkupAttribute::Foreground("#3584e4"),
        35 => MarkupAttribute::Foreground("#d4267e"),
        36 => MarkupAttribute::Foreground("#00f7f7"),
        37 => MarkupAttribute::Foreground("#ffffff"),

        40 => MarkupAttribute::Background("#000000"),
        41 => MarkupAttribute::Background("#e01b24"),
        42 => MarkupAttribute::Background("#33d17a"),
        43 => MarkupAttribute::Background("#f6d32d"),
        44 => MarkupAttribute::Background("#3584e4"),
        45 => MarkupAttribute::Background("#d4267e"),
        46 => MarkupAttribute::Background("#00f7f7"),
        47 => MarkupAttribute::Background("#ffffff"),

        90 => MarkupAttribute::Foreground("#3d3846"),
        91 => MarkupAttribute::Foreground("#f66151"),
        92 => MarkupAttribute::Foreground("#8ff0a4"),
        93 => MarkupAttribute::Foreground("#f9f06b"),
        94 => MarkupAttribute::Foreground("#99c1f1"),
        95 => MarkupAttribute::Foreground("#c061cb"),
        96 => MarkupAttribute::Foreground("#33c7de"),
        97 => MarkupAttribute::Foreground("#f66151"),

        100 => MarkupAttribute::Background("#3d3846"),
        101 => MarkupAttribute::Background("#f66151"),
        102 => MarkupAttribute::Background("#8ff0a4"),
        103 => MarkupAttribute::Background("#f9f06b"),
        104 => MarkupAttribute::Background("#99c1f1"),
        105 => MarkupAttribute::Background("#c061cb"),
        106 => MarkupAttribute::Background("#33c7de"),
        109 => MarkupAttribute::Background("#f66151"),

        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct PlainTextPerform(String);

impl PlainTextPerform {
    fn move_out_buffer(&mut self) -> String {
        let mut buffer = String::new();
        mem::swap(&mut self.0, &mut buffer);
        buffer
    }
}

impl vte::Perform for PlainTextPerform {
    fn print(&mut self, c: char) {
        self.0.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            self.0.push(char::from(byte));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_or_plain_output_does_not_panic_or_become_markup() {
        let mut perform = MarkupPerform::default();
        assert_eq!(perform.decode(b"<tag>&plain"), " &lt;tag&gt;&amp;plain");
        assert_eq!(
            perform.decode(b"2026-11-24T05:00:00.000000000Z <tag>&value"),
            "2026-11-24T05:00:00.000000000Z &lt;tag&gt;&amp;value",
        );
        assert_eq!(perform.decode(b""), " ");
    }

    #[test]
    fn repeated_terminal_colours_keep_a_bounded_style_stack() {
        let mut perform = MarkupPerform::default();
        for _ in 0..10_000 {
            perform.decode(b"2026-11-24T05:00:00Z \x1b[31mred\x1b[1mbold");
            assert_eq!(perform.attributes.len(), 2);
        }
    }

    #[test]
    fn exported_logs_preserve_line_breaks_and_tabs() {
        let mut perform = PlainTextPerform::default();
        let mut parser = vte::Parser::new();
        parser.advance(&mut perform, b"one\ntwo\tvalue\r\n\x1b[31mred\x1b[0m\n");
        assert_eq!(perform.move_out_buffer(), "one\ntwo\tvalue\r\nred\n");
    }

    #[test]
    fn exported_logs_preserve_characters_and_escapes_across_frames() {
        let mut perform = PlainTextPerform::default();
        let mut parser = vte::Parser::new();
        let mut output = String::new();
        for byte in "中文\x1b[31mred\x1b[0m\n".as_bytes() {
            parser.advance(&mut perform, &[*byte]);
            output.push_str(&perform.move_out_buffer());
        }
        assert_eq!(output, "中文red\n");
    }

    #[test]
    fn log_resume_distinguishes_records_within_a_microsecond() {
        let cursor = LogTimestamp::parse("2026-11-24T05:00:00.000000002Z").unwrap();
        let frame = b"2026-11-24T05:00:00.000000001Z old\n2026-11-24T05:00:00.000000002Z seen\n2026-11-24T05:00:00.000000003Z new";
        let lines = frame
            .split(|byte| *byte == b'\n')
            .filter(|line| LogTimestamp::from_line(line).is_none_or(|timestamp| timestamp > cursor))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![b"2026-11-24T05:00:00.000000003Z new".as_slice()]
        );
        assert_eq!(
            LogTimestamp::parse("2026-11-24T05:00:00.12Z"),
            LogTimestamp::parse("2026-11-24T05:00:00.120000000Z"),
        );
        assert!(LogTimestamp::from_line(b"plain output").is_none());
    }
}
