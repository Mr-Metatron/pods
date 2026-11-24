//! Populated navigation and read-only pages, using the application's own models.

use super::fixture::Fixture;
use super::*;

const VIEWPORTS: [(i32, i32); 5] = [(360, 651), (366, 651), (651, 366), (366, 300), (1024, 768)];

fn wait_until(description: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        settle();
    }
    settle();
}

fn assert_horizontal_bounds(widget: &gtk::Widget, parent: &impl IsA<gtk::Widget>) {
    let bounds = widget.compute_bounds(parent).unwrap();
    assert!(
        bounds.x() >= -1.0 && bounds.x() + bounds.width() <= parent.as_ref().width() as f32 + 1.0,
        "{} {:?} extends outside {} ({}px)",
        widget.type_().name(),
        widget.buildable_id(),
        parent.as_ref().type_().name(),
        parent.as_ref().width(),
    );
}

// A root widget can fit while its children overflow. Check both the pane and
// each header/action bar, including their actual button allocations.
fn assert_chrome_fits(page: &impl IsA<gtk::Widget>, window: &adw::Window) {
    for widget in descendants(page).into_iter().filter(gtk::Widget::is_mapped) {
        if widget.is::<adw::HeaderBar>() || widget.is::<gtk::ActionBar>() {
            assert_fits(&widget, page);
            assert_fits(&widget, window);
            let (minimum, _, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
            assert!(
                minimum <= widget.width(),
                "{} requests {minimum}px but has {}px",
                widget.type_().name(),
                widget.width()
            );
            for button in descendants(&widget).into_iter().filter(|child| {
                child.is_mapped() && (child.is::<gtk::Button>() || child.is::<gtk::MenuButton>())
            }) {
                assert_fits(&button, &widget);
                assert_fits(&button, page);
            }
        }
        if widget.is::<gtk::ListBoxRow>() {
            // Rows may legitimately be above or below the scrolling viewport.
            assert_horizontal_bounds(&widget, page);
        }
        if (widget.is::<gtk::Button>()
            || widget.is::<gtk::MenuButton>()
            || widget.is::<view::ContainersCountBar>())
            && widget.ancestor(gtk::ColumnView::static_type()).is_none()
        {
            // Scrolling tables intentionally extend past their viewport. Other
            // row controls, including count badges, must remain inside the pane.
            assert_horizontal_bounds(&widget, page);
            if let Some(row) = widget.ancestor(gtk::ListBoxRow::static_type()) {
                assert_horizontal_bounds(&widget, &row);
            }
        }
    }
}

fn check_page(window: &adw::Window, page: &impl IsA<gtk::Widget>, directory: &Path, name: &str) {
    for (width, height) in VIEWPORTS {
        resize(window, width, height);
        capture(window, directory, &format!("{name}-{width}x{height}"));
        assert_fits(page, window);
        assert_chrome_fits(page, window);
        if width <= 366 {
            for row in descendants(page).into_iter().filter(|row| row.is_mapped()) {
                let text = if row.is::<view::ImageRow>() {
                    descendants(&find_id(&row, "repo_tags_list_box"))
                        .into_iter()
                        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
                        .max_by_key(|label| label.text().len())
                        .map(|label| label.upcast())
                } else if row.is::<view::VolumeRow>() || row.is::<view::ContainerVolumeRow>() {
                    Some(find_id(&row, "name_label"))
                } else {
                    None
                };
                if let Some(text) = text {
                    eprintln!(
                        "{name} {width}x{height}: {} text={}px, row={}px",
                        row.type_().name(),
                        text.width(),
                        row.width()
                    );
                    assert!(text.width() >= 180, "long row text needs readable width");
                }
            }
        }
    }
}

fn check_scroll_end(
    window: &adw::Window,
    page: &impl IsA<gtk::Widget>,
    directory: &Path,
    name: &str,
) {
    resize(window, 366, 300);
    let scroll = descendants(page)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
        .find(|scroll| scroll.is_mapped() && scroll.vadjustment().upper() > scroll.height() as f64)
        .expect("populated page should have scrollable content at reduced height");
    let adjustment = scroll.vadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    capture(window, directory, &format!("{name}-scroll-end-366x300"));
    assert_fits(&scroll, page);
    assert!(
        (adjustment.value() + adjustment.page_size() - adjustment.upper()).abs() < 1.0,
        "last content must remain reachable by scrolling"
    );
    assert_chrome_fits(page, window);
    adjustment.set_value(adjustment.lower());
    settle();
}

fn assert_back_button(page: &impl IsA<gtk::Widget>) {
    let back = descendants(page).into_iter().find_map(|widget| {
        widget.downcast::<gtk::Button>().ok().filter(|button| {
            button.is_mapped() && button.action_name().as_deref() == Some("navigation.pop")
        })
    });
    assert!(
        back.is_some(),
        "visible page must expose its existing Back action"
    );
}

fn pop_to(navigation: &adw::NavigationView, page: &impl IsA<adw::NavigationPage>) {
    assert!(navigation.pop());
    settle();
    assert_eq!(navigation.visible_page().as_ref(), Some(page.as_ref()));
}

pub(super) fn run(window: &adw::Window, fixture: &Fixture, directory: &Path) {
    let client_view: view::ClientView = glib::Object::builder()
        .property("client", fixture.client())
        .build();
    window.set_content(Some(&client_view));
    let split: adw::NavigationSplitView = find(&client_view);
    let sidebar: adw::Sidebar = find(&client_view);
    let panels = find_id(&client_view, "panels_stack")
        .downcast::<gtk::Stack>()
        .unwrap();
    let threshold = adw::LengthUnit::Sp.to_px(720.0, Some(&gtk::Settings::default().unwrap()));
    for (width, height) in [
        (360, 651),
        (366, 651),
        (651, 366),
        (667, 366),
        (668, 366),
        (719, 366),
        (720, 366),
        (721, 366),
        (1024, 768),
        (366, 300),
    ] {
        resize(window, width, height);
        sidebar.emit_by_name::<()>("activated", &[&2u32]);
        settle();
        capture(window, directory, &format!("navigation-{width}x{height}"));
        assert_eq!(split.is_collapsed(), (width as f64) < threshold);
        assert_eq!(panels.visible_child_name().as_deref(), Some("images"));
        assert!(split.shows_content());
        assert!(
            split.measure(gtk::Orientation::Horizontal, -1).0 <= split.width(),
            "navigation panes must fit at the split breakpoint"
        );
        assert_chrome_fits(&split.content().unwrap(), window);
        if split.is_collapsed() {
            assert_back_button(&split.content().unwrap());
            split.activate_action("navigation.pop", None).unwrap();
            settle();
            assert!(!split.shows_content());
        }
        assert_chrome_fits(&split.sidebar().unwrap(), window);
    }

    // Exercise selection and ordinary actions in all three populated panels.
    for (index, name) in [(0u32, "containers"), (2, "images"), (3, "volumes")] {
        sidebar.emit_by_name::<()>("activated", &[&index]);
        settle();
        let panel = panels.visible_child().unwrap();
        assert_eq!(panels.visible_child_name().as_deref(), Some(name));
        check_page(window, &panel, directory, &format!("list-{name}"));
        panel
            .activate_action(&format!("{name}-panel.enter-selection-mode"), None)
            .unwrap();
        panel
            .activate_action(&format!("{name}-panel.select-visible"), None)
            .unwrap();
        settle();
        check_page(window, &panel, directory, &format!("selection-{name}"));
        resize(window, 720, 366);
        capture(
            window,
            directory,
            &format!("selection-boundary-{name}-720x366"),
        );
        let delete_action = format!("{name}-panel.delete-selection");
        let delete = descendants(&panel)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .find(|button| button.action_name().as_deref() == Some(delete_action.as_str()))
            .expect("selected panel must have its existing delete action");
        eprintln!(
            "selection boundary {name}: delete mapped={}, bounds={:?}",
            delete.is_mapped(),
            delete.compute_bounds(window)
        );
        assert!(delete.is_mapped() && delete.height() > 0);
        assert_fits(&delete, &panel);
        assert_fits(&delete, window);
        assert_chrome_fits(&client_view, window);
        assert_chrome_fits(&split.content().unwrap(), window);
        assert_chrome_fits(&split.sidebar().unwrap(), window);
        assert!(
            split.measure(gtk::Orientation::Horizontal, -1).0 <= split.width(),
            "selected panel must fit at the split breakpoint"
        );
        panel
            .activate_action(&format!("{name}-panel.exit-selection-mode"), None)
            .unwrap();
        settle();
        if name == "containers" {
            panel
                .activate_action("containers-panel.toggle-containers-view", None)
                .unwrap();
            settle();
            check_page(window, &panel, directory, "list-containers-alternate-view");
        }
        check_panel_search(window, &panel, directory, name);
        if name != "volumes" {
            resize(window, 366, 300);
            let menu = descendants(&panel)
                .into_iter()
                .find(|widget| {
                    widget.is_mapped()
                        && (widget.is::<view::ContainerMenuButton>()
                            || widget.is::<view::ImageMenuButton>())
                })
                .expect("populated row should expose its existing action menu");
            check_menu(
                window,
                &find::<gtk::MenuButton>(&menu),
                directory,
                &format!("list-{name}-menu-366x300"),
            );
        }
        if name == "images" {
            check_short_image(window, &panel, fixture, directory);
        }
    }

    // The production detail pages are pushed into their existing navigation
    // stack. Inspection/log/process actions below use that same stack and Back.
    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    resize(window, 366, 651);
    let navigation = client_view.navigation_view();
    let list_page = navigation.visible_page().unwrap();
    let details = view::ContainerDetailsPage::from(&fixture.container());
    navigation.push(&details);
    settle();
    check_page(window, &details, directory, "details-container");
    let volume_row: view::ContainerVolumeRow = find(&details);
    let scroll = volume_row
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    for (width, height) in [(366, 651), (360, 651), (366, 300)] {
        resize(window, width, height);
        let adjustment = scroll.vadjustment();
        adjustment
            .set_value(adjustment.value() + volume_row.compute_bounds(&scroll).unwrap().y() as f64);
        settle();
        capture(
            window,
            directory,
            &format!("container-volume-row-{width}x{height}"),
        );
        assert_fits(&volume_row, &scroll);
        eprintln!(
            "container volume row {width}x{height}: name={}px, row={}px",
            find_id(&volume_row, "name_label").width(),
            volume_row.width()
        );
        assert!(find_id(&volume_row, "name_label").width() >= 180);
        if height == 300 {
            check_menu(
                window,
                &find::<gtk::MenuButton>(&volume_row),
                directory,
                "container-volume-menu-366x300",
            );
        }
    }
    check_scroll_end(window, &details, directory, "details-container");
    assert_back_button(&details);
    check_rename(window, &details, directory);
    check_delete_confirmation(window, &details, directory);

    details.show_inspection();
    wait_until("container inspection page", || {
        navigation
            .visible_page()
            .is_some_and(|page| page.is::<view::ScalableTextViewPage>())
    });
    let inspection = navigation
        .visible_page()
        .unwrap()
        .downcast::<view::ScalableTextViewPage>()
        .unwrap();
    let source: sourceview5::View = find(&inspection);
    wait_until("inspection JSON", || source.buffer().char_count() > 0);
    check_page(window, &inspection, directory, "inspection-container");
    inspection.set_search_mode(true);
    settle();
    check_page(window, &inspection, directory, "inspection-search");
    assert_back_button(&inspection);
    pop_to(navigation, &details);

    details.show_log();
    wait_until("container log page", || {
        navigation
            .visible_page()
            .is_some_and(|page| page.is::<view::ContainerLogPage>())
    });
    let logs = navigation
        .visible_page()
        .unwrap()
        .downcast::<view::ContainerLogPage>()
        .unwrap();
    let source: sourceview5::View = find(&logs);
    wait_until("loaded log text", || source.buffer().line_count() >= 24);
    check_page(window, &logs, directory, "logs-container");
    logs.set_search_mode(true);
    settle();
    check_page(window, &logs, directory, "logs-search");
    assert_back_button(&logs);
    eprintln!("Returning from loaded logs to the container list");
    pop_to(navigation, &details);
    pop_to(navigation, &list_page);
    eprintln!("Returned from loaded logs to the container list");

    for container in fixture.containers().into_iter().skip(1) {
        eprintln!("Opening {:?} container details", container.status());
        let details = view::ContainerDetailsPage::from(&container);
        navigation.push(&details);
        settle();
        for (width, height) in [(360, 651), (366, 300)] {
            resize(window, width, height);
            capture(
                window,
                directory,
                &format!("details-{:?}-{width}x{height}", container.status()),
            );
            assert_chrome_fits(&details, window);
            assert_fits(&find_id(&details, "action_center_box"), &details);
        }
        pop_to(navigation, &list_page);
    }

    let image = view::ImageDetailsPage::from(&fixture.image());
    navigation.push(&image);
    settle();
    check_page(window, &image, directory, "details-image");
    check_scroll_end(window, &image, directory, "details-image");
    assert_back_button(&image);
    pop_to(navigation, &list_page);

    let volume = view::VolumeDetailsPage::from(&fixture.volume());
    navigation.push(&volume);
    settle();
    check_page(window, &volume, directory, "details-volume");
    check_scroll_end(window, &volume, directory, "details-volume");
    assert_back_button(&volume);
    pop_to(navigation, &list_page);

    let top = view::TopPage::from(&fixture.container());
    navigation.push(&top);
    settle();
    check_top(window, &top, directory);
    pop_to(navigation, &list_page);
    println!(
        "Populated navigation, selection, details, inspection, logs and process-page layout passed"
    );
}

fn check_short_image(
    window: &adw::Window,
    panel: &gtk::Widget,
    fixture: &Fixture,
    directory: &Path,
) {
    let image = fixture.image();
    let tags = image.repo_tags();
    let original = tags
        .iter::<model::RepoTag>()
        .map(|tag| tag.unwrap().full())
        .collect();
    tags.update(std::collections::HashSet::from(["alpine:latest".into()]));
    settle();
    let row: view::ImageRow = find(panel);
    for (width, height) in [(366, 651), (1024, 768)] {
        resize(window, width, height);
        capture(window, directory, &format!("short-image-{width}x{height}"));
        let text = find_id(&row, "repo_tags_list_box").parent().unwrap();
        let counts: view::ContainersCountBar = find(&row);
        let text_bounds = text.compute_bounds(&row).unwrap();
        let count_bounds = counts.compute_bounds(&row).unwrap();
        assert!(
            count_bounds.y() < text_bounds.y() + text_bounds.height()
                && count_bounds.y() + count_bounds.height() > text_bounds.y(),
            "short text and counts should remain on the same line when they fit"
        );
        assert_chrome_fits(panel, window);
    }
    tags.update(original);
    settle();
}

fn check_panel_search(window: &adw::Window, panel: &gtk::Widget, directory: &Path, name: &str) {
    let search: gtk::SearchBar = find(panel);
    let entry: gtk::SearchEntry = find(&search);
    search.set_search_mode(true);
    entry.set_text("没有匹配项 / a deliberately unmatched search term");
    let stack = find_id(panel, "filter_stack")
        .downcast::<gtk::Stack>()
        .unwrap();
    wait_until("empty filtered list", || {
        stack.visible_child_name().as_deref() == Some("empty")
    });
    let status = stack.visible_child().unwrap();
    let action = format!("{name}-panel.show-all-{name}");
    let show_all = descendants(&status)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.action_name().as_deref() == Some(action.as_str()))
        .expect("filtered empty-state Show All action");
    for (width, height) in [(360, 651), (366, 300), (651, 366)] {
        resize(window, width, height);
        capture(
            window,
            directory,
            &format!("filter-{name}-{width}x{height}"),
        );
        assert_chrome_fits(panel, window);
        assert_fits(&entry, panel);
        check_scrolled_actions(
            window,
            std::slice::from_ref(&show_all),
            directory,
            &format!("filter-{name}-scroll-end-{width}x{height}"),
        );
    }
    panel
        .activate_action(&format!("{name}-panel.show-all-{name}"), None)
        .unwrap();
    wait_until("restored populated list", || {
        stack.visible_child_name().as_deref()
            == Some(if name == "containers" {
                "containers"
            } else {
                "list"
            })
    });
}

fn check_menu(window: &adw::Window, menu: &gtk::MenuButton, directory: &Path, name: &str) {
    menu.grab_focus();
    prepare_popup_input();
    menu.popup();
    settle();
    let popover = menu.popover().unwrap();
    capture(&popover, directory, name);
    let frame = popover.child().unwrap().parent().unwrap();
    let bounds = frame.compute_bounds(&frame).unwrap();
    assert!(bounds.width() <= window.width() as f32);
    assert!(bounds.height() <= window.height() as f32);
    menu.popdown();
    settle();
}

pub(super) fn empty(window: &adw::Window, _fixture: &Fixture, directory: &Path) {
    let fixture = Fixture::empty(directory);
    let client_view: view::ClientView = glib::Object::builder()
        .property("client", fixture.client())
        .build();
    window.set_content(Some(&client_view));
    let sidebar: adw::Sidebar = find(&client_view);
    let panels = find_id(&client_view, "panels_stack")
        .downcast::<gtk::Stack>()
        .unwrap();
    for (index, name) in [(0u32, "containers"), (2, "images"), (3, "volumes")] {
        sidebar.emit_by_name::<()>("activated", &[&index]);
        settle();
        let panel = panels.visible_child().unwrap();
        for (width, height) in VIEWPORTS {
            resize(window, width, height);
            let status = descendants(&panel)
                .into_iter()
                .filter_map(|widget| widget.downcast::<adw::StatusPage>().ok())
                .find(|status| status.is_mapped())
                .expect("empty panel must show its status page");
            let scroll: gtk::ScrolledWindow = find(&status);
            let adjustment = scroll.vadjustment();
            adjustment.set_value(adjustment.lower());
            settle();
            wait_for_paint(window);
            capture(window, directory, &format!("empty-{name}-{width}x{height}"));
            assert_chrome_fits(&panel, window);

            let actions: &[&str] = match name {
                "containers" => &["containers-panel.create-container"],
                "images" => &["images-panel.pull-image", "images-panel.build-image"],
                "volumes" => &["volumes-panel.create-volume"],
                _ => unreachable!(),
            };
            let buttons: Vec<gtk::Button> = actions
                .iter()
                .map(|action| {
                    descendants(&status)
                        .into_iter()
                        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
                        .find(|button| button.action_name().as_deref() == Some(*action))
                        .unwrap_or_else(|| panic!("missing empty-state action {action}"))
                })
                .collect();

            // StatusPage already scrolls its content. Check the actual clipped
            // viewport after scrolling, not just the outer panel's allocation.
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            settle();
            wait_for_paint(window);
            capture(
                window,
                directory,
                &format!("empty-{name}-scroll-end-{width}x{height}"),
            );
            if name == "images" {
                let bin = status
                    .child()
                    .unwrap()
                    .downcast::<adw::BreakpointBin>()
                    .unwrap();
                let child = bin.child().unwrap();
                let minimum = child.measure(gtk::Orientation::Vertical, bin.width()).0;
                eprintln!(
                    "empty-images action bin: height={}, child minimum={minimum}, child bounds={:?}",
                    bin.height(),
                    child.compute_bounds(&bin)
                );
                assert!(
                    bin.height() >= minimum,
                    "empty-image action bin clips its child: {}px allocated, {minimum}px required",
                    bin.height()
                );
                for button in &buttons {
                    assert_fits(button, &bin);
                }
            }
            assert_scrolled_action_bounds(
                window,
                &scroll,
                &buttons,
                &format!("empty-{name} {width}x{height}"),
            );
        }
    }
    fixture.assert_requests();
    eprintln!("Empty container, image and volume actions fit and remain reachable after scrolling");
}

fn check_rename(window: &adw::Window, details: &view::ContainerDetailsPage, directory: &Path) {
    details
        .activate_action("container-details-page.rename", None)
        .unwrap();
    settle();
    let dialog = window.visible_dialog().unwrap();
    for (width, height) in [(360, 651), (366, 300), (651, 366)] {
        resize(window, width, height);
        capture(window, directory, &format!("rename-{width}x{height}"));
        assert_fits(&dialog, window);
        let renamer: view::ContainerRenamer = find(&dialog);
        assert_horizontal_bounds(renamer.upcast_ref(), &dialog);
        let text: gtk::Text = find(&renamer);
        assert!(text.grab_focus());
        settle();
        assert_fits(&text, &dialog);
        for button in descendants(&dialog)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .filter(|button| button.is_mapped())
        {
            assert_fits(&button, &dialog);
        }
    }
    dialog.close();
    settle();
}

fn check_delete_confirmation(
    window: &adw::Window,
    details: &view::ContainerDetailsPage,
    directory: &Path,
) {
    resize(window, 366, 300);
    let observed = std::rc::Rc::new(std::cell::RefCell::new(None));
    glib::idle_add_local_once({
        let window = window.clone();
        let directory = directory.to_path_buf();
        let observed = observed.clone();
        move || {
            let dialog = window.visible_dialog().unwrap();
            settle();
            capture(&window, &directory, "delete-container-confirmation-366x300");
            observed.replace(Some((
                dialog.compute_bounds(&window).unwrap(),
                window.width(),
                window.height(),
            )));
            // The production confirmation waits for its response in a nested
            // main loop. Closing it selects the existing Cancel response.
            dialog.close();
        }
    });
    details
        .activate_action("container-details-page.delete", None)
        .unwrap();
    let (bounds, width, height) = observed.borrow().clone().unwrap();
    assert!(bounds.x() >= -1.0 && bounds.y() >= -1.0);
    assert!(bounds.x() + bounds.width() <= width as f32 + 1.0);
    assert!(bounds.y() + bounds.height() <= height as f32 + 1.0);
    settle();
}

fn check_top(window: &adw::Window, top: &view::TopPage, directory: &Path) {
    let table: gtk::ColumnView = find(top);
    let selection = table.model().unwrap();
    wait_until("process rows", || selection.n_items() == 2);
    check_page(window, top, directory, "processes");
    selection.select_item(0, true);
    settle();
    check_page(window, top, directory, "processes-selected");
    resize(window, 366, 300);
    let scroll: gtk::ScrolledWindow = find(top);
    assert_fits(&scroll, top);
    let horizontal = scroll.hadjustment();
    horizontal.set_value(horizontal.upper() - horizontal.page_size());
    settle();
    capture(window, directory, "processes-last-columns-366x300");
    assert!(
        horizontal.value() > 0.0,
        "wide process columns should remain scrollable"
    );
    assert!((horizontal.value() + horizontal.page_size() - horizontal.upper()).abs() < 1.0);
    horizontal.set_value(0.0);

    let button = find_id(top, "search_button")
        .downcast::<gtk::ToggleButton>()
        .unwrap();
    let entry: gtk::SearchEntry = find(top);
    button.set_active(true);
    settle();
    capture(window, directory, "processes-search-open-366x300");
    let focused = gtk::prelude::RootExt::focus(window)
        .is_some_and(|focus| descendants(&entry).contains(&focus));
    entry.set_text("secondary-service");
    wait_until("filtered process row", || selection.n_items() == 1);
    button.set_active(false);
    settle();
    capture(window, directory, "processes-search-closed-366x300");
    eprintln!(
        "Process search: focus after opening={focused}, text after closing={:?}, visible rows={}",
        entry.text(),
        selection.n_items()
    );
    assert!(focused, "opening process search must focus its typing area");
    assert!(
        entry.text().is_empty(),
        "closing process search must clear its filter"
    );
    assert_eq!(selection.n_items(), 2);
}
