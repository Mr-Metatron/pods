//! Mobile search, saved connections and engine information using existing views.

use super::*;
use crate::model::prelude::*;

fn check_horizontal(widget: &impl IsA<gtk::Widget>, window: &adw::Window) {
    for child in descendants(widget)
        .into_iter()
        .filter(|child| child.is_mapped())
    {
        if child.is::<gtk::Button>() || child.is::<gtk::MenuButton>() || child.is::<gtk::Text>() {
            let bounds = child.compute_bounds(window).unwrap();
            assert!(
                bounds.x() >= -1.0 && bounds.x() + bounds.width() <= window.width() as f32 + 1.0,
                "{} clipped in {}px: {bounds:?}",
                child.type_().name(),
                window.width()
            );
        }
    }
}

fn show(window: &adw::Window, widget: &impl IsA<gtk::Widget>, directory: &Path, name: &str) {
    window.set_content(Some(widget));
    for (width, height) in [(360, 651), (366, 300), (1024, 768)] {
        resize(window, width, height);
        check_horizontal(widget, window);
        capture(window, directory, &format!("{name}-{width}x{height}"));
        for scroll in descendants(widget)
            .into_iter()
            .filter_map(|child| child.downcast::<gtk::ScrolledWindow>().ok())
            .filter(|scroll| scroll.is_mapped())
        {
            let adjustment = scroll.vadjustment();
            let bottom = (adjustment.upper() - adjustment.page_size()).max(0.0);
            adjustment.set_value(bottom);
            settle();
            assert!((adjustment.value() - bottom).abs() < 1.0);
        }
        check_horizontal(widget, window);
        capture(window, directory, &format!("{name}-{width}x{height}-end"));
    }
}

pub(super) fn run(window: &adw::Window, fixture: &fixture::Fixture, directory: &Path) {
    let client = fixture.client();
    let search: view::SearchPanel = glib::Object::builder().property("client", client).build();
    window.set_content(Some(&search));
    resize(window, 366, 651);
    let entry: gtk::SearchEntry = find(&search);
    entry.set_text("mobile");
    let stack = find_id(&search, "main_stack")
        .downcast::<gtk::Stack>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while stack.visible_child_name().as_deref() != Some("results") {
        assert!(
            Instant::now() < deadline,
            "global search results did not load"
        );
        settle();
    }
    show(window, &search, directory, "global-search-results");
    entry.set_text("no-matching-mobile-resource");
    let resource_rows = || {
        [
            "containers_list_box",
            "pods_list_box",
            "images_list_box",
            "volumes_list_box",
        ]
        .map(|id| {
            let list = find_id(&search, id).downcast::<gtk::ListBox>().unwrap();
            (0..)
                .take_while(|index| list.row_at_index(*index).is_some())
                .count()
        })
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while resource_rows() != [0; 4] {
        assert!(Instant::now() < deadline, "unmatched query still has rows");
        settle();
    }
    settle();
    eprintln!(
        "Global search unmatched query: rows={:?}, actual stack={:?}",
        resource_rows(),
        stack.visible_child_name()
    );
    capture(window, directory, "global-search-unmatched-existing-state");
    if stack.visible_child_name().as_deref() != Some("no-results") {
        assert_eq!(stack.visible_child_name().as_deref(), Some("search"));
        // The existing search-state transition can leave the initial placeholder
        // after all rows disappear. Record that upstream defect separately. This
        // explicit template fixture checks geometry, not the broken transition.
        eprintln!(
            "No-results template-only layout check; existing transition is recorded in upstream-issues.md"
        );
        stack.set_visible_child_name("no-results");
        settle();
    }
    resize(window, 366, 300);
    check_horizontal(&search, window);
    assert!(entry.grab_focus());
    capture(
        window,
        directory,
        "global-search-no-results-template-keyboard",
    );

    client.set_info(Some(model::Info::new(
        client,
        engine::dto::Info {
            arch: Some("aarch64".into()),
            cgroup_driver: Some("systemd".into()),
            cgroup_version: Some("2".into()),
            hostname: Some("mobile-device-with-a-long-local-hostname".into()),
            kernel: Some("6.18.60-virt-mobile-layout-fixture".into()),
            mem_total: Some(4 * 1024 * 1024 * 1024),
            cpus: Some(8),
            os: Some("Alpine Linux / postmarketOS — 移动设备布局测试".into()),
            storage_driver: Some("overlay2".into()),
            storage_root_dir: Some(
                "/var/lib/docker/a-long-path-for-container-engine-storage".into(),
            ),
            version: Some("29.9.3 (UI fixture)".into()),
        },
    )));
    let info: view::InfoPanel = glib::Object::builder().property("client", client).build();
    show(window, &info, directory, "engine-information");
    assert_eq!(
        find_id(&info, "stack")
            .downcast::<gtk::Stack>()
            .unwrap()
            .visible_child_name()
            .as_deref(),
        Some("loaded")
    );

    // Load saved connections through the real persistence reader. This test has
    // its own fresh configuration directory and never activates a connection.
    let path = crate::utils::config_dir().join("connections.json");
    let previous = std::fs::read(&path).ok();
    let connections = serde_json::json!({
        "mobile-ui-local": {"uuid":"mobile-ui-local", "name":"本机 Docker — long saved connection name", "url":"unix:///run/mobile-ui-fixture.sock", "rgb":null},
        "mobile-ui-remote": {"uuid":"mobile-ui-remote", "name":"Remote development engine with a long display name", "url":"tcp://example.test:2376", "rgb":null}
    });
    std::fs::write(&path, serde_json::to_vec(&connections).unwrap()).unwrap();
    let manager = model::ConnectionManager::default();
    manager.setup(|error| {
        eprintln!("Saved UI connections have no previously selected connection: {error}")
    });
    match previous {
        Some(bytes) => std::fs::write(&path, bytes).unwrap(),
        None => std::fs::remove_file(&path).unwrap(),
    }
    assert_eq!(manager.n_items(), 2);
    let chooser: view::ConnectionChooserPage = glib::Object::builder()
        .property("connection-manager", &manager)
        .build();
    show(window, &chooser, directory, "saved-connections");
    resize(window, 366, 300);
    let toggle = find_id(&chooser, "filter_button")
        .downcast::<gtk::ToggleButton>()
        .unwrap();
    toggle.grab_focus();
    toggle.set_active(true);
    settle();
    let filter = find_id(&chooser, "filter_entry")
        .downcast::<gtk::SearchEntry>()
        .unwrap();
    capture(window, directory, "saved-connections-filter-keyboard");
    assert!(
        gtk::prelude::RootExt::focus(window)
            .is_some_and(|focus| descendants(&filter).contains(&focus)),
        "touch activation must focus the saved-connection filter"
    );
    filter.set_text("Docker");
    settle();
    check_horizontal(&chooser, window);
    toggle.set_active(false);
    settle();
    assert!(filter.text().is_empty());

    let list = client.actions();
    for (name, state) in [
        ("ongoing", model::ActionState::Ongoing),
        ("finished", model::ActionState::Finished),
        ("failed", model::ActionState::Failed),
        ("cancelled", model::ActionState::Cancelled),
    ] {
        let action = model::Action::builder::<model::VolumeCreateAction>(&list)
            .property(
                "opts",
                model::BoxedVolumeCreateOpts::from(engine::opts::VolumeCreateOpts {
                    name: Some("volume-with-a-long-name-for-mobile-action-history".into()),
                }),
            )
            .build();
        action.set_state(state);
        let row: view::ActionRow = glib::Object::builder()
            .property("action", action.upcast_ref::<model::Action>())
            .build();
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
        box_.append(&row);
        window.set_content(Some(&box_));
        resize(window, 360, 651);
        check_horizontal(&row, window);
        assert_fits(&find_id(&row, "action_button"), &row);
        capture(window, directory, &format!("action-history-{name}"));
    }
    window.set_content(gtk::Widget::NONE);
    println!(
        "Populated global search, engine information, saved connections and action rows passed"
    );
}
