//! GTK integration checks using the installed application resources and real widgets.
//!
//! Run under a display server with GSETTINGS_BACKEND=memory, an isolated XDG config
//! directory and the installed schema directory. These checks simulate viewport sizes;
//! they do not exercise a phone's compositor, input method or Docker daemon.

use std::os::unix::net::UnixListener;
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

fn settle() {
    let context = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(150);
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
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

fn resize(window: &impl IsA<gtk::Window>, width: i32, height: i32) {
    let window = window.as_ref();
    let mut requested = (width, height);
    for _ in 0..4 {
        window.set_default_size(requested.0, requested.1);
        window.present();
        settle();
        let allocated = (window.width(), window.height());
        if allocated == (width, height) {
            return;
        }
        eprintln!(
            "viewport {width}x{height}: requested {requested:?}, allocated {allocated:?}, surface {:?}",
            window.surface().map(|surface| (surface.width(), surface.height()))
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

fn capture(widget: &impl IsA<gtk::Widget>, directory: &Path, name: &str) {
    let widget = widget.as_ref();
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(widget)).snapshot(
        &snapshot,
        widget.width() as f64,
        widget.height() as f64,
    );
    let node = snapshot.to_node().expect("nonempty widget snapshot");
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
    crate::APPLICATION_OPTS
        .set(crate::ApplicationOptions {
            config_dir: directory.join("config"),
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
    let main_window = app.main_window();
    main_window.set_decorated(false);
    for (width, height) in [(366, 651), (651, 366), (366, 300), (360, 651), (1024, 768)] {
        resize(&main_window, width, height);
        capture(
            &main_window,
            &directory,
            &format!("welcome-{width}x{height}"),
        );
    }
    main_window.set_visible(false);

    let log_page: view::ContainerLogPage = glib::Object::new();
    let log_view: sourceview5::View = find(&log_page);
    assert!(
        !log_view.buffer().enables_undo(),
        "streamed logs must not retain evicted text in an undo history"
    );
    log_page.check_log_window_for_test();

    let window = adw::Window::builder()
        .application(&app)
        .decorated(false)
        .build();
    let manager = model::ConnectionManager::default();
    let transport_directory = tempfile::tempdir_in(&directory).unwrap();
    let socket_path = transport_directory.path().join("engine.sock");
    // Bollard validates the local socket before creating a client. Keep a real
    // listener alive for the fixture without running or mutating a Docker daemon.
    let _listener = UnixListener::bind(&socket_path).unwrap();
    let connection = model::Connection::new(
        "ui-test",
        "本机 Docker — a deliberately long connection name",
        &format!("unix://{}", socket_path.display()),
        None,
        &manager,
    );
    // A real Client with an isolated, unconnected transport supplies real capabilities
    // and models. No mutating daemon operations are activated by this UI test.
    let client: model::Client = glib::Object::builder()
        .property("connection", &connection)
        .property(
            "engine",
            model::Engine::from(engine::Engine::Docker(
                bollard::Docker::connect_with_host(connection.url().as_str()).unwrap(),
            )),
        )
        .build();
    let client_view: view::ClientView = glib::Object::builder().property("client", &client).build();
    window.set_content(Some(&client_view));
    let split: adw::NavigationSplitView = find(&client_view);
    for (width, height) in [(366, 651), (651, 366), (1024, 768), (366, 300)] {
        resize(&window, width, height);
        assert_eq!(split.is_collapsed(), width < 668);
        split.set_show_content(true);
        settle();
        assert_fits(&client_view, &window);
        capture(&window, &directory, &format!("navigation-{width}x{height}"));
    }

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
    for (width, height) in [(366, 651), (651, 366), (366, 300), (360, 651), (1024, 768)] {
        resize(&window, width, height);
        assert_eq!(controls.is_visible(), width <= 668);
        if controls.is_visible() {
            for button in descendants(&controls)
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            {
                assert!(button.height() >= 44);
                assert_fits(&button, &window);
                assert!(!button.gets_focus_on_click());
            }
        }
        capture(&window, &directory, &format!("terminal-{width}x{height}"));
    }
    let terminal: view::ContainerTerminal = find(&terminal_page);
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
        terminal.clipboard().set_text("echo mobile-paste");
        terminal_page
            .activate_action("container-terminal-page.paste", None)
            .unwrap();
        settle();
    });
    assert_eq!(
        input,
        vec![
            b"\x1b".to_vec(),
            b"\t".to_vec(),
            b"\x03".to_vec(),
            b"\x1b[A".to_vec(),
            b"\x1b[B".to_vec(),
            b"echo mobile-paste".to_vec()
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
    popover.popup();
    settle();
    let scroll: gtk::ScrolledWindow = find(&popover);
    assert!(scroll.measure(gtk::Orientation::Vertical, 300).0 < 200);
    assert!(scroll.height() <= 200);
    capture(&popover, &directory, "suggestion-constrained-height");
    let list: gtk::ListView = find(&popover);
    list.model().unwrap().select_item(0, true);
    list.emit_by_name::<()>("activate", &[&0u32]);
    settle();
    assert!(suggestion.text().ends_with(":latest"));
    assert!(!popover.is_visible());

    for (name, dialog) in [
        (
            "connection",
            view::ConnectionCreationDialog::from(&manager).upcast::<adw::Dialog>(),
        ),
        (
            "create-container",
            view::ContainerCreateOptsDialog::new(&client, None).upcast(),
        ),
        (
            "create-volume",
            view::VolumeCreateOptsDialog::new(&client, None).upcast(),
        ),
        (
            "pull-image",
            view::ImagePullOptsDialog::new(&client, None).upcast(),
        ),
    ] {
        for (width, height) in [(366, 651), (366, 300)] {
            resize(&window, width, height);
            dialog.present(Some(&window));
            settle();
            assert_fits(&dialog, &window);
            capture(
                &window,
                &directory,
                &format!("dialog-{name}-{width}x{height}"),
            );
            dialog.close();
            settle();
        }
    }

    println!(
        "GTK simulated viewport checks passed; screenshots: {}",
        directory.display()
    );
    window.destroy();
    main_window.destroy();
    // Keep the application and synthetic fixture models alive until all windows close.
    drop((app, client, manager, connection, container));
}
