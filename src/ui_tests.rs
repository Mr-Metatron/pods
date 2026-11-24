//! GTK integration checks using the installed application resources and real widgets.
//!
//! Run under a display server with GSETTINGS_BACKEND=memory, an isolated XDG config
//! directory and the installed schema directory. These checks activate the real main
//! window and exercise navigation, populated pages, dialogs and mobile widgets
//! with real models backed by an isolated local HTTP fixture. The phone's actual
//! input method, touch hardware and graphics driver remain device checks.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::config;
use crate::engine;
use crate::model;
use crate::view;
use crate::widget;

mod auxiliary;
mod fixture;
mod forms;
mod pages;
mod terminal;

fn settle() {
    let context = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(150);
    while Instant::now() < until {
        while context.pending() && Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn prepare_popup_input() {
    if let Some(helper) = std::env::var_os("PODS_UI_TEST_INPUT_HELPER") {
        assert!(
            std::process::Command::new(helper)
                .status()
                .expect("run the guest's input helper")
                .success()
        );
        settle();
    }
}

fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = vec![widget.as_ref().clone()];
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        result.extend(descendants(&current));
        child = current.next_sibling();
    }
    result
}

fn find<T: IsA<gtk::Widget>>(widget: &impl IsA<gtk::Widget>) -> T {
    descendants(widget)
        .into_iter()
        .find_map(|widget| widget.downcast::<T>().ok())
        .expect("expected widget in actual template")
}

fn find_id(widget: &impl IsA<gtk::Widget>, id: &str) -> gtk::Widget {
    descendants(widget)
        .into_iter()
        .find(|widget| widget.buildable_id().as_deref() == Some(id))
        .unwrap_or_else(|| panic!("missing template child {id}"))
}

#[track_caller]
fn wait_for_paint(window: &impl IsA<gtk::Window>) {
    let window = window.as_ref();
    let painted = std::rc::Rc::new(std::cell::Cell::new(0));
    let clock = window.frame_clock().expect("mapped window frame clock");
    let handler = clock.connect_after_paint({
        let painted = painted.clone();
        let window = window.clone();
        move |clock| {
            painted.set(painted.get() + 1);
            if painted.get() < 2 {
                window.queue_draw();
                clock.request_phase(gtk::gdk::FrameClockPhase::AFTER_PAINT);
            }
        }
    });
    let context = glib::MainContext::default();
    let started = Instant::now();
    let deadline = started + Duration::from_secs(5);
    let mut longest_dispatch = Duration::ZERO;
    // Request each frame once. Repeated invalidation while waiting is unnecessary
    // and can obscure whether GTK is processing layout or the test is redrawing.
    window.queue_draw();
    clock.request_phase(gtk::gdk::FrameClockPhase::AFTER_PAINT);
    while painted.get() < 2 && Instant::now() < deadline {
        let dispatch_started = Instant::now();
        context.iteration(false);
        longest_dispatch = longest_dispatch.max(dispatch_started.elapsed());
        std::thread::sleep(Duration::from_millis(1));
    }
    clock.disconnect(handler);
    if started.elapsed() > Duration::from_secs(1) {
        eprintln!(
            "GTK paint wait at {}: elapsed={:?}, longest dispatch={longest_dispatch:?}, frames={}, mapped={}, size={}x{}",
            std::panic::Location::caller(),
            started.elapsed(),
            painted.get(),
            window.is_mapped(),
            window.width(),
            window.height()
        );
    }
    assert!(
        painted.get() >= 2,
        "timed out waiting for GTK layout frames: frames={}, mapped={}, size={}x{}, elapsed={:?}, longest dispatch={longest_dispatch:?}",
        painted.get(),
        window.is_mapped(),
        window.width(),
        window.height(),
        started.elapsed()
    );
}

#[track_caller]
fn resize(window: &impl IsA<gtk::Window>, width: i32, height: i32) {
    let window = window.as_ref();
    let mut requested = window.surface().map_or((width, height), |surface| {
        (
            width + (surface.width() - window.width()).max(0),
            height + (surface.height() - window.height()).max(0),
        )
    });
    for _ in 0..4 {
        window.set_default_size(requested.0, requested.1);
        window.present();
        settle();
        // A toplevel can have its new allocation before the split view applies
        // its breakpoint in the next layout frame. Wait for completed painting
        // instead of treating the outer size alone as a settled widget tree.
        wait_for_paint(window);
        let allocated = (window.width(), window.height());
        if allocated == (width, height) {
            return;
        }
        eprintln!(
            "viewport {width}x{height}: requested {requested:?}, allocated {allocated:?}, surface {:?}",
            window
                .surface()
                .map(|surface| (surface.width(), surface.height()))
        );
        // GTK's default size can include the window's client decorations. Calibrate
        // the outer request so assertions below measure the actual content viewport.
        requested.0 = (requested.0 + width - allocated.0).max(1);
        requested.1 = (requested.1 + height - allocated.1).max(1);
    }
    assert_eq!((window.width(), window.height()), (width, height));
}

fn assert_fits(widget: &impl IsA<gtk::Widget>, parent: &impl IsA<gtk::Widget>) {
    let bounds = widget.as_ref().compute_bounds(parent).unwrap();
    assert!(bounds.x() >= -1.0, "left edge clipped: {bounds:?}");
    assert!(bounds.y() >= -1.0, "top edge clipped: {bounds:?}");
    assert!(
        bounds.x() + bounds.width() <= parent.as_ref().width() as f32 + 1.0,
        "right edge clipped: {bounds:?} in {}",
        parent.as_ref().width()
    );
    assert!(
        bounds.y() + bounds.height() <= parent.as_ref().height() as f32 + 1.0,
        "bottom edge clipped: {bounds:?} in {}",
        parent.as_ref().height()
    );
}

fn assert_scrolled_action_bounds(
    window: &impl IsA<gtk::Window>,
    scroll: &gtk::ScrolledWindow,
    buttons: &[gtk::Button],
    name: &str,
) {
    let window = window.as_ref();
    let viewport = scroll.child().expect("action scrolling viewport");
    let adjustment = scroll.vadjustment();
    eprintln!(
        "{name}: scroll value={}, upper={}, page-size={}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size()
    );
    assert!(
        (adjustment.value() + adjustment.page_size() - adjustment.upper()).abs() < 1.0,
        "actions must be reachable at the end of the existing scroll area"
    );
    for button in buttons {
        let bounds = button.compute_bounds(button).unwrap();
        let minimum = button
            .measure(
                gtk::Orientation::Vertical,
                bounds.width() as i32 + button.margin_start() + button.margin_end(),
            )
            .0;
        // measure includes explicit widget margins; compute_bounds describes
        // the border box. Welcome's existing New Connection button has a margin.
        let allocated = bounds.height() + (button.margin_top() + button.margin_bottom()) as f32;
        eprintln!(
            "{name} action {}: mapped={}, sensitive={}, border height={}, allocated with margins={allocated}, minimum={minimum}, viewport bounds={:?}",
            button.action_name().unwrap(),
            button.is_mapped(),
            button.is_sensitive(),
            bounds.height(),
            button.compute_bounds(&viewport)
        );
        assert!(button.is_mapped());
        assert!(button.is_sensitive());
        assert!(allocated >= minimum as f32);
        assert_fits(button, &viewport);
        assert_fits(button, scroll);
        assert_fits(button, window);
        let mut ancestor = button.parent();
        while let Some(widget) = ancestor {
            if widget.overflow() == gtk::Overflow::Hidden {
                assert_fits(button, &widget);
            }
            ancestor = widget.parent();
        }
    }
}

fn check_scrolled_actions(
    window: &impl IsA<gtk::Window>,
    buttons: &[gtk::Button],
    directory: &Path,
    name: &str,
) {
    let window = window.as_ref();
    let scroll = buttons
        .first()
        .expect("at least one action button")
        .ancestor(gtk::ScrolledWindow::static_type())
        .expect("action's existing scrolling container")
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scroll.vadjustment();
    let previous = adjustment.value();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    wait_for_paint(window);
    capture(window, directory, name);
    assert_scrolled_action_bounds(window, &scroll, buttons, name);
    adjustment.set_value(previous);
    settle();
    wait_for_paint(window);
}

fn capture(widget: &impl IsA<gtk::Widget>, directory: &Path, name: &str) {
    let widget = widget.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let deadline = Instant::now() + Duration::from_secs(5);
    let node = loop {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, widget.width() as f64, widget.height() as f64);
        if let Some(node) = snapshot.to_node() {
            break node;
        }
        assert!(
            Instant::now() < deadline,
            "empty snapshot {name}: {} {}x{}, mapped={}, visible={}",
            widget.type_().name(),
            widget.width(),
            widget.height(),
            widget.is_mapped(),
            widget.is_visible()
        );
        // A Wayland surface may be allocated before its first frame is painted.
        widget.queue_draw();
        settle();
    };
    widget
        .native()
        .unwrap()
        .renderer()
        .unwrap()
        .render_texture(&node, None)
        .save_to_png(directory.join(format!("{name}.png")))
        .unwrap();
}

#[test]
#[ignore = "requires a display, installed resources and GSETTINGS_BACKEND=memory"]
fn mobile_widgets_fit_and_keep_controls_reachable() {
    assert_eq!(std::env::var("GSETTINGS_BACKEND").as_deref(), Ok("memory"));
    adw::init().unwrap();
    let display = gtk::gdk::Display::default().unwrap();
    eprintln!(
        "GTK backend: {}, input seat: {:?}",
        display.type_().name(),
        display.default_seat().map(|seat| seat.capabilities())
    );
    sourceview5::init();
    gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR).unwrap();
    gettextrs::textdomain(config::GETTEXT_PACKAGE).unwrap();
    if let Ok(expected) = std::env::var("PODS_UI_TEST_REGEX_LABEL") {
        assert_eq!(gettextrs::gettext("Regular Expression"), expected);
    }

    let directory = std::env::var_os("PODS_UI_TEST_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pods-ui-tests"));
    std::fs::create_dir_all(&directory).unwrap();
    let config_directory = tempfile::tempdir_in(&directory).unwrap();
    crate::APPLICATION_OPTS
        .set(crate::ApplicationOptions {
            config_dir: config_directory.path().to_path_buf(),
            unix_socket_path: directory.join("missing-podman.sock"),
        })
        .unwrap();

    for path in [
        config::RESOURCES_FILE,
        config::APPDATA_RESOURCES_FILE,
        config::UI_RESOURCES_FILE,
    ] {
        gio::resources_register(&gio::Resource::load(path).unwrap());
    }
    crate::init();
    gtk::Settings::default()
        .unwrap()
        .set_property("gtk-enable-animations", false);

    let app = crate::Application::default();
    app.set_flags(gio::ApplicationFlags::NON_UNIQUE);
    app.register(gio::Cancellable::NONE).unwrap();
    if std::env::var_os("PODS_UI_TEST_COLOR_ONLY").is_some() {
        let window = adw::Window::builder()
            .application(&app)
            .decorated(false)
            .build();
        forms::native_color_dialog(&window, &model::ConnectionManager::default(), &directory);
        window.destroy();
        app.quit();
        return;
    }
    let image_settings = gio::Settings::new(&format!("{}.view.panels.images", config::APP_ID));
    assert_eq!(image_settings.string("sort-attribute").as_str(), "id");
    app.activate();
    let main_window = app.main_window();
    main_window.set_decorated(false);
    settle();
    let main_stack = find_id(&main_window, "main_stack")
        .downcast::<gtk::Stack>()
        .unwrap();
    assert_eq!(main_stack.visible_child_name().as_deref(), Some("welcome"));
    let welcome = main_stack.visible_child().unwrap();
    let new_connection = descendants(&welcome)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.action_name().as_deref() == Some("win.create-connection"))
        .expect("welcome New Connection action");
    for (width, height) in [(366, 651), (651, 366), (366, 300), (360, 651), (1024, 768)] {
        resize(&main_window, width, height);
        assert_fits(&welcome, &main_window);
        capture(
            &main_window,
            &directory,
            &format!("startup-{width}x{height}"),
        );
        check_scrolled_actions(
            &main_window,
            std::slice::from_ref(&new_connection),
            &directory,
            &format!("startup-actions-scroll-end-{width}x{height}"),
        );
    }
    println!("Application activation and main-window startup passed with the installed schema");
    main_window.set_visible(false);

    let window = adw::Window::builder()
        .application(&app)
        .decorated(false)
        .build();
    let fixture = fixture::Fixture::new(&directory);
    forms::cancel_in_main_window(&main_window, &fixture, &directory);
    let client = fixture.client();
    let manager = fixture.manager();
    let mut failed_sections = Vec::new();
    let sections: [(&str, fn(&adw::Window, &fixture::Fixture, &Path)); 4] = [
        ("forms", forms::run),
        ("auxiliary", auxiliary::run),
        ("empty", pages::empty),
        ("pages", pages::run),
    ];
    let selected_section = std::env::var("PODS_UI_TEST_SECTION").ok();
    if let Some(selected) = &selected_section {
        assert!(sections.iter().any(|(name, _)| name == selected));
    }
    for (name, check) in sections {
        if selected_section
            .as_ref()
            .is_some_and(|selected| selected != name)
        {
            continue;
        }
        eprintln!("Starting mobile UI section: {name}");
        // Preserve each failure's panic output while still exercising the other
        // independent UI sections. The combined test fails below if any failed.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            check(&window, &fixture, &directory);
        }))
        .is_err()
        {
            failed_sections.push(name);
        }
        if let Some(dialog) = window.visible_dialog() {
            dialog.force_close();
            settle();
        }
        window.set_content(gtk::Widget::NONE);
    }
    if selected_section.is_some() {
        assert!(
            failed_sections.is_empty(),
            "Mobile UI sections failed: {failed_sections:?}"
        );
        window.destroy();
        main_window.destroy();
        fixture.assert_requests();
        return;
    }

    eprintln!("Starting shared search, terminal controls, suggestions and connection help");
    let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
    buffer.set_text("alpha Alpha\n日志一\n日志二\n");
    let source_view = sourceview5::View::with_buffer(&buffer);
    let search: widget::SourceViewSearchWidget = glib::Object::builder()
        .property("source-view", &source_view)
        .build();
    let search_bar = gtk::SearchBar::new();
    search_bar.set_child(Some(&search));
    search_bar.set_search_mode(true);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&search_bar);
    content.append(&source_view);
    window.set_content(Some(&content));
    let entry: widget::TextSearchEntry = find(&search);
    entry.set_regex(false);
    entry.set_case_sensitive(false);
    entry.set_whole_word(false);
    search.set_text("日志");
    settle();
    search.search_forward();
    assert!(entry.info().contains('2'));
    entry.set_info("第 12345678 项，共 99999999 项 / 12345678 of 99999999");
    for (width, height) in [(366, 651), (360, 300), (651, 366), (1024, 768), (366, 300)] {
        resize(&window, width, height);
        assert_eq!(entry.compact(), search.width() <= 500);
        if width <= 366 {
            assert!(
                entry.compact(),
                "phone search should expose compact controls"
            );
        }
        let text: gtk::Text = find(&entry);
        assert!(
            text.width() >= 80,
            "typing area too narrow: {}",
            text.width()
        );
        assert_fits(&text, &window);
        for button in descendants(&search)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .filter(|button| button.is_mapped())
        {
            assert_fits(&button, &window);
        }
        capture(&window, &directory, &format!("search-{width}x{height}"));
    }
    let options = find_id(&entry, "options_button")
        .downcast::<gtk::MenuButton>()
        .unwrap();
    prepare_popup_input();
    options.popup();
    settle();
    let popover = options.popover().unwrap();
    let toggles: Vec<gtk::CheckButton> = descendants(&popover)
        .into_iter()
        .filter_map(|widget| widget.downcast().ok())
        .collect();
    assert_eq!(toggles.len(), 3);
    for toggle in toggles {
        toggle.set_active(true);
    }
    assert!(entry.regex() && entry.case_sensitive() && entry.whole_word());
    capture(&popover, &directory, "search-options");
    options.popdown();
    resize(&window, 1024, 768);
    assert!(
        !entry.compact(),
        "desktop search should retain inline options"
    );
    let inline = find_id(&entry, "options_box");
    for toggle in descendants(&inline)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::ToggleButton>().ok())
    {
        assert!(toggle.is_active());
    }

    let container = model::Container::new_from_summary(
        &client.containers(),
        engine::dto::ContainerSummary {
            created: 0,
            health_status: engine::dto::HealthStatus::default(),
            id: "ui-test-container".into(),
            image_id: "ui-test-image".into(),
            image_name: Some("localhost/example/very-long-image-name:latest".into()),
            is_infra: false,
            mounts: Default::default(),
            name: "中文容器 / a very long container name for a small viewport".into(),
            pod_id: None,
            ports: Vec::new(),
            status: engine::dto::ContainerStatus::Exited,
        },
    );
    let terminal_page = view::ContainerTerminalPage::from(&container);
    window.set_content(Some(&terminal_page));
    let controls = find_id(&terminal_page, "touch_controls");
    let terminal: view::ContainerTerminal = find(&terminal_page);
    let start = descendants(&terminal)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| {
            button.action_name().as_deref() == Some("container-terminal.start-or-resume")
        })
        .expect("not-running terminal Start action");
    for (width, height) in [(366, 651), (651, 366), (366, 300), (360, 651), (1024, 768)] {
        resize(&window, width, height);
        capture(&window, &directory, &format!("terminal-{width}x{height}"));
        check_scrolled_actions(
            &window,
            std::slice::from_ref(&start),
            &directory,
            &format!("terminal-start-scroll-end-{width}x{height}"),
        );
        eprintln!(
            "terminal minimum widths: header={}, controls={}, terminal={}",
            find::<adw::HeaderBar>(&terminal_page)
                .measure(gtk::Orientation::Horizontal, -1)
                .0,
            controls.measure(gtk::Orientation::Horizontal, -1).0,
            terminal.measure(gtk::Orientation::Horizontal, -1).0
        );
        assert_eq!(controls.is_visible(), width <= 668);
        if controls.is_visible() {
            for button in descendants(&controls)
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            {
                let bounds = button.compute_bounds(&button).unwrap();
                assert!(
                    bounds.height() >= 44.0 && bounds.width() >= 44.0,
                    "touch target too small: bounds {bounds:?}, content {}x{}",
                    button.width(),
                    button.height()
                );
                let center_x = (bounds.x() + bounds.width() / 2.0) as f64;
                let center_y = (bounds.y() + bounds.height() / 2.0) as f64;
                for (x, y) in [
                    (center_x - 21.5, center_y),
                    (center_x + 21.5, center_y),
                    (center_x, center_y - 21.5),
                    (center_x, center_y + 21.5),
                ] {
                    assert!(
                        button.contains(x, y),
                        "44-pixel target must accept pointer input"
                    );
                }
                assert_fits(&button, &window);
                assert!(!button.gets_focus_on_click());
            }
        }
    }
    let input = terminal.capture_test_input(|| {
        for action in [
            "escape",
            "tab",
            "interrupt",
            "previous-command",
            "next-command",
        ] {
            terminal_page
                .activate_action(&format!("container-terminal-page.{action}"), None)
                .unwrap();
        }
    });
    assert_eq!(
        input,
        vec![
            b"\x1b".to_vec(),
            b"\t".to_vec(),
            b"\x03".to_vec(),
            b"\x1b[A".to_vec(),
            b"\x1b[B".to_vec()
        ]
    );

    let suggestion: view::ImageSuggestionEntryRow = glib::Object::new();
    for index in 0..20 {
        suggestion
            .search_results()
            .append(&model::ImageSearchResponse::from(
                engine::dto::ImageSearchResponseItem {
                    automated: None,
                    description: Some("用于验证窄屏滚动与选择的镜像说明".into()),
                    index: None,
                    is_official: false,
                    name: Some(format!("example/long-image-name-{index}")),
                    stars: 20 - index,
                },
            ));
    }
    let group = adw::PreferencesGroup::new();
    group.add(&suggestion);
    let preferences = adw::PreferencesPage::new();
    preferences.add(&group);
    window.set_content(Some(&preferences));
    resize(&window, 366, 300);
    suggestion.set_text("example");
    suggestion
        .upcast_ref::<widget::SuggestionEntryRow>()
        .set_visible_stack_page(widget::SuggestionEntryVisibleStackPage::Results);
    suggestion.grab_focus();
    let popover: gtk::Popover = find(&suggestion);
    prepare_popup_input();
    popover.popup();
    settle();
    let scroll: gtk::ScrolledWindow = find(&popover);
    eprintln!(
        "suggestion popup: popover {}x{}, scroll {}x{}, scroll vertical {:?}, stack vertical {:?}",
        popover.width(),
        popover.height(),
        scroll.width(),
        scroll.height(),
        scroll.measure(gtk::Orientation::Vertical, scroll.width()),
        find::<gtk::Stack>(&popover).measure(gtk::Orientation::Vertical, scroll.width())
    );
    capture(&popover, &directory, "suggestion-constrained-height");
    assert!(scroll.measure(gtk::Orientation::Vertical, 300).0 < 200);
    assert!(scroll.height() <= 200);
    let list: gtk::ListView = find(&popover);
    list.model().unwrap().select_item(0, true);
    list.emit_by_name::<()>("activate", &[&0u32]);
    settle();
    assert!(suggestion.text().ends_with(":latest"));
    assert!(!popover.is_visible());

    let dialog = view::ConnectionCreationDialog::from(manager);
    for (width, height) in [(366, 651), (366, 300), (651, 366)] {
        resize(&window, width, height);
        dialog.present(Some(&window));
        settle();
        assert_fits(&dialog, &window);
        capture(
            &window,
            &directory,
            &format!("dialog-connection-{width}x{height}"),
        );
        let help_buttons: Vec<gtk::MenuButton> = descendants(&dialog)
            .into_iter()
            .filter_map(|widget| widget.downcast().ok())
            .filter(|button: &gtk::MenuButton| button.popover().is_some())
            .collect();
        assert_eq!(help_buttons.len(), 2);
        for (index, button) in help_buttons.into_iter().enumerate() {
            button.grab_focus();
            settle();
            prepare_popup_input();
            button.popup();
            settle();
            let help = button.popover().unwrap();
            let scroll: gtk::ScrolledWindow = find(&help);
            // Native Wayland popovers include transparent shadow margins in their
            // surface allocation. Measure the visible content frame for clipping.
            let frame = help.child().unwrap().parent().unwrap();
            let frame_bounds = frame.compute_bounds(&frame).unwrap();
            eprintln!(
                "connection help {index} at {width}x{height}: popup {}x{}, frame {}x{}, scroll {}x{}",
                help.width(),
                help.height(),
                frame_bounds.width(),
                frame_bounds.height(),
                scroll.width(),
                scroll.height()
            );
            assert!(
                scroll.width() >= 200,
                "help text must retain a readable width"
            );
            let adjustment = scroll.vadjustment();
            adjustment.set_value(0.0);
            settle();
            capture(
                &help,
                &directory,
                &format!("connection-help-{index}-{width}x{height}"),
            );
            assert!(frame_bounds.width() <= width as f32 && frame_bounds.height() <= height as f32);
            assert_eq!(scroll.hscrollbar_policy(), gtk::PolicyType::Never);
            let bottom = adjustment.upper() - adjustment.page_size();
            adjustment.set_value(bottom);
            settle();
            assert!((adjustment.value() - bottom).abs() < 1.0);
            capture(
                &help,
                &directory,
                &format!("connection-help-{index}-{width}x{height}-end"),
            );
            help.popdown();
            settle();
        }
        dialog.close();
        settle();
    }

    assert!(
        failed_sections.is_empty(),
        "Mobile UI sections failed: {failed_sections:?}; see their preceding failure details"
    );
    println!(
        "GTK simulated viewport checks passed; screenshots: {}",
        directory.display()
    );
    window.destroy();
    main_window.destroy();
    fixture.assert_requests();
    // Keep the application and synthetic fixture models alive until all windows close.
    drop((app, fixture, container));
}
