//! Real form widgets at phone, keyboard-reduced and desktop sizes.

use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use adw::prelude::*;

use super::capture;
use super::descendants;
use super::find;
use super::find_id;
use super::fixture::Fixture;
use super::prepare_popup_input;
use super::resize;
use super::settle;
use super::wait_for_paint;
use crate::engine;
use crate::model;
use crate::model::prelude::*;
use crate::view;
use crate::widget;

const VIEWPORTS: [(i32, i32); 5] = [(366, 651), (360, 651), (651, 366), (366, 300), (1024, 768)];
const LONG_PATH: &str =
    "/home/mobile/容器工作目录/long-project-and-directory-name/configuration/file.tar";
const LONG_REFERENCE: &str = "registry.example.test/mobile/long-project-and-image-name:latest";

fn record_failure(failures: &mut Vec<String>, message: String) {
    eprintln!("MOBILE_FORM_LAYOUT_FAILURE {message}");
    failures.push(message);
}

fn check_bounds(
    widget: &impl IsA<gtk::Widget>,
    parent: &impl IsA<gtk::Widget>,
    label: &str,
    vertical: bool,
    failures: &mut Vec<String>,
) {
    let widget = widget.as_ref();
    let bounds = widget.compute_bounds(parent).unwrap();
    let fits_x =
        bounds.x() >= -1.0 && bounds.x() + bounds.width() <= parent.as_ref().width() as f32 + 1.0;
    let fits_y =
        bounds.y() >= -1.0 && bounds.y() + bounds.height() <= parent.as_ref().height() as f32 + 1.0;
    if !fits_x || (vertical && !fits_y) {
        record_failure(
            failures,
            format!(
                "{label}: {} {:?} in {}x{}",
                widget.type_().name(),
                bounds,
                parent.as_ref().width(),
                parent.as_ref().height()
            ),
        );
    }
}

fn reset_window(window: &adw::Window, width: i32, height: i32) {
    window.set_content(gtk::Widget::NONE);
    resize(window, width, height);
}

fn inspect_dialog(
    window: &adw::Window,
    dialog: &impl IsA<adw::Dialog>,
    name: &str,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    let dialog = dialog.as_ref();
    settle();
    wait_for_paint(window);
    eprintln!(
        "form {name}: minimum {}, dialog {}x{}, viewport {}x{}",
        dialog.measure(gtk::Orientation::Horizontal, -1).0,
        dialog.width(),
        dialog.height(),
        window.width(),
        window.height()
    );
    check_bounds(dialog, window, name, true, failures);
    // Scrollable rows can legitimately lie below the visible area. Their controls
    // must still fit horizontally; focus below verifies vertical reachability.
    for child in descendants(dialog).into_iter().filter(|w| w.is_mapped()) {
        if child.is::<gtk::Button>() || child.is::<gtk::MenuButton>() || child.is::<gtk::Text>() {
            check_bounds(&child, window, name, false, failures);
        }
    }
    capture(window, directory, name);

    let fields: Vec<gtk::Text> = descendants(dialog)
        .into_iter()
        .filter_map(|w| w.downcast().ok())
        .filter(|text: &gtk::Text| text.is_mapped() && text.is_sensitive() && text.is_editable())
        .collect();
    for text in fields {
        assert!(
            text.grab_focus(),
            "editable field must accept focus: {name}"
        );
        settle();
        wait_for_paint(window);
        check_bounds(
            &text,
            window,
            &format!("{name} focused field"),
            true,
            failures,
        );
        if let Some(scroll) = text.ancestor(gtk::ScrolledWindow::static_type()) {
            check_bounds(
                &text,
                &scroll,
                &format!("{name} focused field in scroll viewport"),
                true,
                failures,
            );
        }
    }
    if window.height() == 300 {
        capture(window, directory, &format!("{name}-focused"));
    }
    for scroll in descendants(dialog)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ScrolledWindow>().ok())
        .filter(|scroll| scroll.is_mapped())
    {
        let adjustment = scroll.vadjustment();
        adjustment.set_value((adjustment.upper() - adjustment.page_size()).max(0.0));
    }
    settle();
    wait_for_paint(window);
    capture(window, directory, &format!("{name}-end"));
}

fn close(dialog: &impl IsA<adw::Dialog>) {
    dialog.as_ref().close();
    settle();
}

fn inspect_split_menu(
    window: &adw::Window,
    dialog: &impl IsA<gtk::Widget>,
    name: &str,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    let split = find::<adw::SplitButton>(dialog);
    split.grab_focus();
    prepare_popup_input();
    split.popup();
    settle();
    let popover = split.popover().unwrap();
    let frame = popover.child().unwrap().parent().unwrap();
    let bounds = frame.compute_bounds(&frame).unwrap();
    if bounds.width() > window.width() as f32 || bounds.height() > window.height() as f32 {
        record_failure(
            failures,
            format!("{name}: split button menu frame {bounds:?} exceeds viewport"),
        );
    }
    capture(&popover, directory, name);
    split.popdown();
    settle();
}

fn key_value_rows(window: &adw::Window, directory: &Path, failures: &mut Vec<String>) {
    let value = model::KeyVal::from((
        "LONG_ENVIRONMENT_VARIABLE_NAME",
        "长值 / a long editable value",
    ));
    let row = view::KeyValRow::from(&value);
    let key = find_id(&row, "key_entry").downcast::<gtk::Entry>().unwrap();
    let entry = find_id(&row, "value_entry")
        .downcast::<gtk::Entry>()
        .unwrap();
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.append(&row);
    let group = adw::PreferencesGroup::new();
    group.add(&list);
    let page = adw::PreferencesPage::new();
    page.add(&group);
    for (width, height) in VIEWPORTS {
        reset_window(window, width, height);
        window.set_content(Some(&page));
        settle();
        wait_for_paint(window);
        let name = format!("form-key-value-{width}x{height}");
        eprintln!(
            "{name}: row minimum {}, fields {} / {}, viewport {}x{}",
            row.measure(gtk::Orientation::Horizontal, -1).0,
            key.width(),
            entry.width(),
            window.width(),
            window.height()
        );
        if window.width() != width {
            record_failure(
                failures,
                format!("{name}: form expands viewport to {}", window.width()),
            );
        }
        for child in [&key, &entry] {
            check_bounds(child, window, &name, true, failures);
            if child.width() < 80 {
                record_failure(
                    failures,
                    format!("{name}: input width {} is below 80", child.width()),
                );
            }
        }
        check_bounds(&find::<gtk::Button>(&row), window, &name, true, failures);
        entry.grab_focus();
        settle();
        capture(window, directory, &name);
    }
    key.set_text("MOBILE_EDITED_KEY");
    entry.set_text("中文 edited value");
    assert_eq!(value.key(), "MOBILE_EDITED_KEY");
    assert_eq!(value.value(), "中文 edited value");
    let key_bounds = key.compute_bounds(&row).unwrap();
    let value_bounds = entry.compute_bounds(&row).unwrap();
    assert_eq!(
        key_bounds.y(),
        value_bounds.y(),
        "desktop fields stay in one row"
    );
    window.set_content(gtk::Widget::NONE);
}

fn action_dialogs(
    window: &adw::Window,
    fixture: &Fixture,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    for (width, height) in VIEWPORTS {
        reset_window(window, width, height);
        for (name, state) in [
            ("ongoing", model::ActionState::Ongoing),
            ("finished", model::ActionState::Finished),
            ("failed", model::ActionState::Failed),
            ("cancelled", model::ActionState::Cancelled),
        ] {
            // Building the model does not call VolumeCreateAction::exec or mutate
            // a daemon. Use a real artifact action so both final buttons appear.
            let action: model::VolumeCreateAction =
                model::Action::builder(&fixture.client().actions())
                    .property(
                        "opts",
                        model::BoxedVolumeCreateOpts::from(engine::opts::VolumeCreateOpts {
                            name: Some("long-volume-name-for-mobile-layout".into()),
                        }),
                    )
                    .build();
            if state == model::ActionState::Failed {
                action.set_failed(&format!(
                    "无法完成操作 / simulated error with a long path: {LONG_PATH}"
                ));
            } else {
                if state == model::ActionState::Finished {
                    action.set_artifact(Some(fixture.volume().upcast_ref()));
                }
                action.set_state(state);
            }
            let dialog = view::ActionDialog::from(&action);
            dialog.present(Some(window));
            inspect_dialog(
                window,
                &dialog,
                &format!("form-action-{name}-{width}x{height}"),
                directory,
                failures,
            );
            for button in descendants(&dialog)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .filter(|button| button.is_mapped())
            {
                let name = button.action_name().unwrap_or_default();
                if name == "action-dialog.edit" || name == "action-dialog.view-artifact" {
                    check_bounds(
                        &button,
                        window,
                        "action final buttons after scrolling",
                        true,
                        failures,
                    );
                }
            }
            close(&dialog);
        }
    }
}

fn container_forms(
    window: &adw::Window,
    fixture: &Fixture,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    let container = fixture.container();
    for (width, height) in [(360, 651), (366, 651), (366, 300), (1024, 768)] {
        reset_window(window, width, height);
        let opts = engine::opts::ContainerCreateOpts {
            name: "mobile-container-with-a-long-name".into(),
            image: LONG_REFERENCE.into(),
            cmd: Some(vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo 中文长参数".into(),
            ]),
            env: [("LONG_ENVIRONMENT_VARIABLE_NAME".into(), LONG_PATH.into())].into(),
            labels: [("com.example.long-label-name".into(), "中文标签值".into())].into(),
            memory_limit: Some(512_000_000),
            ports: vec![engine::dto::PortBinding {
                target_port: 65535,
                host_ip: "2001:db8:1234:5678:9012:3456:7890:abcd".into(),
                host_port: Some(65535),
                protocol: engine::dto::PortMappingProtocol::Tcp,
            }],
            ..Default::default()
        };
        let dialog = view::ContainerCreateOptsDialog::new(fixture.client(), Some(opts.into()));
        dialog.present(Some(window));
        settle();
        wait_for_paint(window);
        check_creation_tabs(window, &dialog, directory, failures);
        find_id(&dialog, "health_check_command_entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap()
            .set_text("/bin/sh -c 'printf \"中文 health check with a long argument\"'");
        let stack = find_id(&dialog, "stack")
            .downcast::<adw::ViewStack>()
            .unwrap();
        for page in ["details", "integration", "health"] {
            stack.set_visible_child_name(page);
            settle();
            for expander in descendants(&dialog)
                .into_iter()
                .filter_map(|w| w.downcast::<adw::ExpanderRow>().ok())
                .filter(|row| row.is_mapped() && row.enables_expansion())
            {
                expander.set_expanded(true);
            }
            inspect_dialog(
                window,
                &dialog,
                &format!("form-create-{page}-{width}x{height}"),
                directory,
                failures,
            );
        }
        inspect_split_menu(
            window,
            &dialog,
            &format!("form-create-menu-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);

        let dialog = view::ContainerCommitOptsDialog::new(&container, None);
        for index in 0..3 {
            dialog.add_change(Some(model::Value::from(
                format!("ENV MOBILE_{index}={LONG_PATH}").as_str(),
            )));
        }
        dialog.present(Some(window));
        for id in [
            "author_entry_row",
            "comment_entry_row",
            "repo_entry_row",
            "tag_entry_row",
        ] {
            find_id(&dialog, id)
                .downcast::<adw::EntryRow>()
                .unwrap()
                .set_text("mobile-long-form-input-中文内容");
        }
        inspect_dialog(
            window,
            &dialog,
            &format!("form-commit-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);

        let dialog = view::ContainerCopyFromOptsDialog::new(&container, LONG_PATH, LONG_PATH);
        dialog.present(Some(window));
        inspect_dialog(
            window,
            &dialog,
            &format!("form-copy-from-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);

        let dialog = view::ContainerCopyToOptsDialog::new(&container, false, LONG_PATH, LONG_PATH);
        dialog.present(Some(window));
        inspect_dialog(
            window,
            &dialog,
            &format!("form-copy-to-{width}x{height}"),
            directory,
            failures,
        );
        inspect_split_menu(
            window,
            &dialog,
            &format!("form-copy-to-menu-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);
    }
}

fn image_volume_forms(
    window: &adw::Window,
    fixture: &Fixture,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    let image = fixture.image();
    let tag = model::RepoTag::new(&image.repo_tags(), LONG_REFERENCE.into());
    for (width, height) in [(366, 651), (366, 300), (1024, 768)] {
        reset_window(window, width, height);
        let dialog = view::ImagePullOptsDialog::new(
            fixture.client(),
            Some(
                engine::opts::ImagePullOpts {
                    reference: LONG_REFERENCE.into(),
                }
                .into(),
            ),
        );
        dialog.present(Some(window));
        inspect_dialog(
            window,
            &dialog,
            &format!("form-pull-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);

        let dialog = view::ImageBuildOptsDialog::new(
            fixture.client(),
            Some(
                engine::opts::ImageBuildOpts {
                    path: LONG_PATH.into(),
                    dockerfile: LONG_PATH.into(),
                    tag: LONG_REFERENCE.into(),
                    labels: [("com.example.long-label-name".into(), "中文标签值".into())].into(),
                }
                .into(),
            ),
        );
        dialog.present(Some(window));
        // Exercise text entered by the user without changing how upstream restores
        // saved build options when the dialog is constructed.
        find_id(&dialog, "container_file_path_entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap()
            .set_text(LONG_PATH);
        let stack = find_id(&dialog, "stack")
            .downcast::<adw::ViewStack>()
            .unwrap();
        for page in ["details", "integration"] {
            stack.set_visible_child_name(page);
            inspect_dialog(
                window,
                &dialog,
                &format!("form-build-{page}-{width}x{height}"),
                directory,
                failures,
            );
        }
        close(&dialog);

        let dialog = view::RepoTagAddDialog::default();
        dialog.present(Some(window));
        let entry = find_id(&dialog, "entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap();
        for (state, text) in [
            ("valid", LONG_REFERENCE),
            ("invalid", "example/长标签没有冒号-long-invalid-tag"),
        ] {
            entry.set_text(text);
            inspect_dialog(
                window,
                &dialog,
                &format!("form-tag-{state}-{width}x{height}"),
                directory,
                failures,
            );
        }
        close(&dialog);

        let dialog = view::RepoTagPushOptsDialog::new(&tag, None);
        dialog.present(Some(window));
        find_id(&dialog, "username_entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap()
            .set_text("mobile-test-user-with-a-long-name");
        for id in ["password_entry_row", "token_entry_row"] {
            find_id(&dialog, id)
                .downcast::<adw::PasswordEntryRow>()
                .unwrap()
                .set_text("synthetic-test-value-with-no-real-credentials");
        }
        find_id(&dialog, "login_switch")
            .downcast::<gtk::Switch>()
            .unwrap()
            .set_active(true);
        let toggles = find_id(&dialog, "auth_toggle_group")
            .downcast::<adw::ToggleGroup>()
            .unwrap();
        for auth in ["basic", "token"] {
            toggles.set_active_name(Some(auth));
            inspect_dialog(
                window,
                &dialog,
                &format!("form-push-{auth}-{width}x{height}"),
                directory,
                failures,
            );
        }
        close(&dialog);

        let dialog = view::VolumeCreateOptsDialog::new(
            fixture.client(),
            Some(
                engine::opts::VolumeCreateOpts {
                    name: Some("mobile-long-volume-name".into()),
                }
                .into(),
            ),
        );
        dialog.present(Some(window));
        inspect_dialog(
            window,
            &dialog,
            &format!("form-volume-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);
    }
}

fn prune_and_shared_rows(
    window: &adw::Window,
    fixture: &Fixture,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    for (width, height) in [(366, 651), (366, 300), (1024, 768)] {
        reset_window(window, width, height);
        let dialogs: Vec<(&str, adw::Dialog)> = vec![
            (
                "containers",
                view::ContainersPruneOptsDialog::new(fixture.client(), None).upcast(),
            ),
            (
                "images",
                view::ImagesPruneOptsDialog::new(fixture.client(), None).upcast(),
            ),
            (
                "volumes",
                view::VolumesPruneOptsDialog::new(fixture.client(), None).upcast(),
            ),
        ];
        for (kind, dialog) in dialogs {
            dialog.present(Some(window));
            for row in descendants(&dialog)
                .into_iter()
                .filter_map(|w| w.downcast::<widget::DateTimeRow>().ok())
            {
                if row.is_visible() {
                    row.set_enable_expansion(true);
                    row.set_expanded(true);
                }
            }
            inspect_dialog(
                window,
                &dialog,
                &format!("form-prune-{kind}-{width}x{height}"),
                directory,
                failures,
            );
            close(&dialog);
        }

        let mount = model::Mount::new(fixture.client(), None);
        mount.set_host_path(LONG_PATH);
        mount.set_container_path(LONG_PATH);
        let mount_row = view::MountRow::from(&mount);
        mount_row.set_expanded(true);
        let device = model::Device::default();
        device.set_host_path(LONG_PATH);
        device.set_container_path(LONG_PATH);
        let device_row = view::DeviceRow::from(&device);
        device_row.set_expanded(true);
        let group = adw::PreferencesGroup::new();
        group.add(&mount_row);
        group.add(&device_row);
        let page = adw::PreferencesPage::new();
        page.add(&group);
        let dialog = adw::Dialog::builder().child(&page).build();
        dialog.present(Some(window));
        inspect_dialog(
            window,
            &dialog,
            &format!("form-mount-device-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);
    }
}

fn connection_forms(
    window: &adw::Window,
    fixture: &Fixture,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    for (width, height) in [(366, 651), (366, 300), (1024, 768)] {
        reset_window(window, width, height);
        let dialog = view::ConnectionCreationDialog::from(fixture.manager());
        dialog.present(Some(window));
        find_id(&dialog, "name_entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap()
            .set_text("本机 Docker / a long mobile connection name");
        find_id(&dialog, "custom_url_entry_row")
            .downcast::<adw::EntryRow>()
            .unwrap()
            .set_text("unix:///run/user/1000/long-directory-name/docker.sock");
        find_id(&dialog, "color_switch")
            .downcast::<gtk::Switch>()
            .unwrap()
            .set_active(true);
        inspect_dialog(
            window,
            &dialog,
            &format!("form-connection-input-{width}x{height}"),
            directory,
            failures,
        );
        dialog
            .activate_action("connection-creation-dialog.show-custom-info-dialog", None)
            .unwrap();
        let navigation = find::<adw::NavigationView>(&dialog);
        assert_eq!(
            navigation.visible_page().unwrap().tag().as_deref(),
            Some("custom-connection-info")
        );
        inspect_dialog(
            window,
            &dialog,
            &format!("form-custom-info-{width}x{height}"),
            directory,
            failures,
        );
        assert!(navigation.pop());
        settle();
        assert_eq!(
            navigation.visible_page().unwrap().tag().as_deref(),
            Some("new-connection")
        );
        close(&dialog);

        view::show_connection_error(
            window,
            "Docker socket / 连接权限",
            &format!(
                "Permission denied / 无法访问 Docker Unix socket：{LONG_PATH}。请确认当前用户有权访问。"
            ),
        );
        let dialog = window.visible_dialog().unwrap();
        inspect_dialog(
            window,
            &dialog,
            &format!("form-connection-error-{width}x{height}"),
            directory,
            failures,
        );
        close(&dialog);
    }
}

fn check_creation_tabs(
    window: &adw::Window,
    dialog: &view::ContainerCreateOptsDialog,
    directory: &Path,
    failures: &mut Vec<String>,
) {
    let group = find::<adw::ToggleGroup>(dialog);
    let scroll = group
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scroll.vadjustment();
    eprintln!(
        "creation tabs before scrolling: mapped={}, bounds={:?}, scroll={}/{}/{}",
        group.is_mapped(),
        group.compute_bounds(window),
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size()
    );
    if !group.is_mapped() {
        capture(
            window,
            directory,
            &format!(
                "form-create-tabs-initial-position-{}x{}",
                window.width(),
                window.height()
            ),
        );
    }
    // The tabs are inside the existing PreferencesPage. Reach them by scrolling
    // to its beginning before checking their text and hit areas.
    adjustment.set_value(0.0);
    settle();
    wait_for_paint(window);
    let labels: Vec<gtk::Label> = descendants(&group)
        .into_iter()
        .filter_map(|w| w.downcast().ok())
        .filter(|label: &gtk::Label| label.is_mapped() && !label.label().is_empty())
        .collect();
    assert_eq!(labels.len(), 3);
    for label in labels {
        if label.layout().is_ellipsized() {
            record_failure(
                failures,
                format!(
                    "container creation navigation: {:?} is ellipsized at {}x{}",
                    label.label(),
                    window.width(),
                    window.height()
                ),
            );
        }
    }
    let buttons: Vec<gtk::ToggleButton> = descendants(&group)
        .into_iter()
        .filter_map(|w| w.downcast().ok())
        .filter(|button: &gtk::ToggleButton| button.is_mapped())
        .collect();
    assert_eq!(buttons.len(), 3);
    for button in buttons {
        assert!(button.compute_bounds(&button).unwrap().width() >= 44.0);
        check_bounds(
            &button,
            window,
            "container creation navigation",
            true,
            failures,
        );
    }
}

pub(super) fn run(window: &adw::Window, fixture: &Fixture, directory: &Path) {
    let mut failures = Vec::new();
    key_value_rows(window, directory, &mut failures);
    action_dialogs(window, fixture, directory, &mut failures);
    container_forms(window, fixture, directory, &mut failures);
    image_volume_forms(window, fixture, directory, &mut failures);
    prune_and_shared_rows(window, fixture, directory, &mut failures);
    connection_forms(window, fixture, directory, &mut failures);
    assert!(
        failures.is_empty(),
        "mobile form layout failures:\n{}",
        failures.join("\n")
    );
    println!("Mobile forms, action states, long inputs and keyboard-reduced focus checks passed");
}

/// The ordinary layout fixture has no PdsWindow actions. Check the existing
/// Cancel action once in the real window, where win.close is installed.
pub(super) fn cancel_in_main_window(window: &view::Window, fixture: &Fixture, directory: &Path) {
    resize(window, 366, 651);
    let dialog = view::ContainerCreateOptsDialog::new(fixture.client(), None);
    dialog.present(Some(window));
    settle();
    wait_for_paint(window);
    let cancel = find_id(&dialog, "cancel_button")
        .downcast::<gtk::Button>()
        .unwrap();
    assert!(cancel.is_mapped() && cancel.is_sensitive());
    assert_eq!(cancel.action_name().as_deref(), Some("win.close"));
    let mut failures = Vec::new();
    check_bounds(&cancel, window, "real window Cancel", true, &mut failures);
    capture(window, directory, "form-cancel-real-window-before");
    cancel.emit_clicked();
    settle();
    assert!(
        window.is_visible() && window.is_mapped(),
        "Cancel must keep the main window visible"
    );
    wait_for_paint(window);
    assert!(window.visible_dialog().is_none());
    capture(window, directory, "form-cancel-real-window-after");
    window.set_visible(false);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    println!("Real PdsWindow Cancel closes the form and keeps the main window");
}

/// This separate entry point also runs on actual 366×651 and 366×300 outputs.
/// A native GtkColorDialog has its own toplevel, independent of its parent's size.
pub(super) fn native_color_dialog(
    window: &adw::Window,
    manager: &model::ConnectionManager,
    directory: &Path,
) {
    let mode = std::env::var("PODS_UI_TEST_COLOR_ONLY").unwrap();
    let phone_output = mode.starts_with("phone");
    let output_height = if mode == "phone-short" { 300 } else { 651 };
    if phone_output {
        // Reserve the phone output for the parent application; measure the chooser
        // in its own normal toplevel without forcing its geometry or presentation.
        window.fullscreen();
    }
    reset_window(window, 366, output_height);
    let connection = view::ConnectionCreationDialog::from(manager);
    connection.present(Some(window));
    settle();
    wait_for_paint(window);
    find_id(&connection, "color_switch")
        .downcast::<gtk::Switch>()
        .unwrap()
        .set_active(true);
    let button = find_id(&connection, "color_dialog_button")
        .downcast::<gtk::ColorDialogButton>()
        .unwrap();
    settle();
    wait_for_paint(window);
    let scroll = button
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scroll.vadjustment();
    adjustment.set_value((adjustment.upper() - adjustment.page_size()).max(0.0));
    settle();
    wait_for_paint(window);
    let trigger = find::<gtk::Button>(&button);
    assert!(trigger.grab_focus());
    settle();
    wait_for_paint(window);
    eprintln!(
        "color trigger: mapped={}, sensitive={}, dialog={}, bounds={:?}",
        trigger.is_mapped(),
        trigger.is_sensitive(),
        button.dialog().is_some(),
        trigger.compute_bounds(window)
    );
    capture(window, directory, "native-color-parent-before");
    assert!(trigger.is_mapped() && trigger.is_sensitive() && button.dialog().is_some());
    let original_color = button.rgba();
    let previous: Vec<_> = gtk::Window::list_toplevels()
        .into_iter()
        .filter(|window| window.is_mapped())
        .collect();
    let describe_windows = || {
        for widget in gtk::Window::list_toplevels() {
            let window = widget.downcast::<gtk::Window>().unwrap();
            eprintln!(
                "color toplevel: class={}, title={:?}, visible={}, mapped={}",
                window.type_().name(),
                window.title(),
                window.is_visible(),
                window.is_mapped()
            );
        }
    };
    describe_windows();
    trigger.emit_clicked();
    let deadline = Instant::now() + Duration::from_secs(5);
    let chooser = loop {
        settle();
        if let Some(chooser) = gtk::Window::list_toplevels()
            .into_iter()
            .filter(|widget| !previous.contains(widget))
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|window| window.is_mapped())
        {
            break chooser;
        }
        if Instant::now() >= deadline {
            describe_windows();
            capture(window, directory, "native-color-parent-timeout");
            panic!("native color chooser did not appear");
        }
    };
    wait_for_paint(&chooser);
    let surface = chooser.surface().unwrap();
    let monitor = gtk::prelude::WidgetExt::display(&chooser)
        .monitor_at_surface(&surface)
        .unwrap();
    let output = monitor.geometry();
    eprintln!(
        "native color chooser: class={}, minimum={}x{}, content={}x{}, surface={}x{}, output={}x{}, parent={}x{}",
        chooser.type_().name(),
        chooser.measure(gtk::Orientation::Horizontal, -1).0,
        chooser
            .measure(gtk::Orientation::Vertical, chooser.width())
            .0,
        chooser.width(),
        chooser.height(),
        surface.width(),
        surface.height(),
        output.width(),
        output.height(),
        window.width(),
        window.height()
    );
    capture(&chooser, directory, "native-color-dialog");
    let mut failures = Vec::new();
    for child in descendants(&chooser)
        .into_iter()
        .filter(|widget| widget.is_mapped())
    {
        if child.is::<gtk::Button>() || child.is::<gtk::Text>() {
            check_bounds(
                &child,
                &chooser,
                "native color chooser",
                true,
                &mut failures,
            );
        }
    }
    // GTK's native palette can be wider than its window. Verify the existing
    // scroller makes the rightmost color column reachable on a phone output.
    let mut palettes = 0;
    for scroll in descendants(&chooser)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
        .filter(|scroll| scroll.is_mapped())
    {
        let swatches: Vec<_> = descendants(&scroll)
            .into_iter()
            .filter(|widget| widget.type_().name() == "GtkColorSwatch" && widget.is_mapped())
            .collect();
        if swatches.is_empty() {
            continue;
        }
        palettes += 1;
        let horizontal = scroll.hadjustment();
        let vertical = scroll.vadjustment();
        eprintln!(
            "native color palette: swatches={}, horizontal={}/{}/{}, vertical={}/{}/{}, policy={:?}, kinetic={}",
            swatches.len(),
            horizontal.value(),
            horizontal.upper(),
            horizontal.page_size(),
            vertical.value(),
            vertical.upper(),
            vertical.page_size(),
            scroll.hscrollbar_policy(),
            scroll.is_kinetic_scrolling()
        );
        let rightmost_x = swatches
            .iter()
            .map(|widget| widget.compute_bounds(&scroll).unwrap().x())
            .fold(f32::NEG_INFINITY, f32::max);
        let rightmost: Vec<_> = swatches
            .into_iter()
            .filter(|widget| {
                (widget.compute_bounds(&scroll).unwrap().x() - rightmost_x).abs() < 1.0
            })
            .collect();
        horizontal.set_value((horizontal.upper() - horizontal.page_size()).max(0.0));
        settle();
        wait_for_paint(&chooser);
        for swatch in rightmost {
            let bounds = swatch.compute_bounds(&scroll).unwrap();
            let target = if bounds.y() < 0.0 {
                vertical.value() + f64::from(bounds.y())
            } else {
                vertical.value()
                    + (f64::from(bounds.y() + bounds.height()) - vertical.page_size()).max(0.0)
            };
            if (target - vertical.value()).abs() > 0.5 {
                vertical.set_value(target);
                settle();
                wait_for_paint(&chooser);
            }
            eprintln!(
                "native color palette rightmost: bounds={:?}, focusable={}, sensitive={}",
                swatch.compute_bounds(&scroll),
                swatch.is_focusable(),
                swatch.is_sensitive()
            );
            check_bounds(
                &swatch,
                &scroll,
                "native color palette rightmost column",
                true,
                &mut failures,
            );
        }
        capture(&chooser, directory, "native-color-palette-end");
    }
    assert!(
        palettes > 0,
        "native color palette has a scrolling viewport"
    );
    let add_color = descendants(&chooser)
        .into_iter()
        .find(|widget| widget.widget_name() == "add-color-button")
        .expect("native custom-color button");
    let palette_scroll = add_color
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    palette_scroll.hadjustment().set_value(0.0);
    let adjustment = palette_scroll.vadjustment();
    adjustment.set_value((adjustment.upper() - adjustment.page_size()).max(0.0));
    settle();
    wait_for_paint(&chooser);
    check_bounds(
        &add_color,
        &palette_scroll,
        "native custom-color button",
        true,
        &mut failures,
    );
    assert!(add_color.is_mapped() && add_color.is_sensitive());
    assert!(add_color.grab_focus());
    // GtkColorSwatch handles Space in its existing key controller instead of
    // installing the GtkWidget activation signal used by ordinary buttons.
    let controllers = add_color.observe_controllers();
    let key = (0..controllers.n_items())
        .find_map(|position| {
            controllers
                .item(position)
                .and_then(|object| object.downcast::<gtk::EventControllerKey>().ok())
        })
        .expect("native color swatch key controller");
    assert!(key.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::space,
            &0_u32,
            &gtk::gdk::ModifierType::empty()
        ],
    ));
    settle();
    wait_for_paint(&chooser);
    let editor = descendants(&chooser)
        .into_iter()
        .find(|widget| widget.type_().name() == "GtkColorEditor" && widget.is_mapped())
        .expect("native color editor after activating Custom +");
    capture(&chooser, directory, "native-color-editor");
    let entry = find::<gtk::Entry>(&editor);
    entry.set_text("#2a7fff");
    assert!(entry.grab_focus());
    settle();
    wait_for_paint(&chooser);
    let text = find::<gtk::Text>(&entry);
    let scroll = entry
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    eprintln!(
        "native color editor focus before scrolling: field={:?}, viewport={}x{}, horizontal={}/{}/{}",
        text.compute_bounds(&scroll),
        scroll.width(),
        scroll.height(),
        scroll.hadjustment().value(),
        scroll.hadjustment().upper(),
        scroll.hadjustment().page_size()
    );
    // GTK scrolls the text caret into view, while its native editor can keep
    // part of the field edge outside the viewport. Check the actual color value
    // at both caret positions, then use the existing scrollbar to reveal the
    // complete input area without weakening the original bounds assertion.
    for position in [0, entry.text().len() as i32] {
        entry.set_position(position);
        settle();
        wait_for_paint(&chooser);
        let (caret, _) = text.compute_cursor_extents(position as usize);
        let origin = text
            .compute_point(&scroll, &gtk::graphene::Point::new(caret.x(), caret.y()))
            .unwrap();
        eprintln!(
            "native color editor caret {position}: x={}, y={}, width={}, height={}",
            origin.x(),
            origin.y(),
            caret.width(),
            caret.height()
        );
        if origin.x() < -1.0
            || origin.x() + caret.width().max(1.0) > scroll.width() as f32 + 1.0
            || origin.y() < -1.0
            || origin.y() + caret.height() > scroll.height() as f32 + 1.0
        {
            record_failure(
                &mut failures,
                format!("native color value caret {position} outside the scrolling viewport"),
            );
        }
    }
    capture(&chooser, directory, "native-color-editor-focused");
    let horizontal = scroll.hadjustment();
    horizontal.set_value((horizontal.upper() - horizontal.page_size()).max(0.0));
    scroll.vadjustment().set_value(scroll.vadjustment().lower());
    settle();
    wait_for_paint(&chooser);
    check_bounds(
        &text,
        &scroll,
        "native color editor complete input after scrolling",
        true,
        &mut failures,
    );
    check_bounds(
        &text,
        &chooser,
        "native color editor focused value in window",
        true,
        &mut failures,
    );
    capture(&chooser, directory, "native-color-editor-input-revealed");
    eprintln!(
        "native color editor: content={}x{}, output={}x{}, focused value={:?}",
        chooser.width(),
        chooser.height(),
        output.width(),
        output.height(),
        text.compute_bounds(&scroll)
    );
    if phone_output {
        assert_eq!((output.width(), output.height()), (366, output_height));
        // GDK surfaces can include transparent shadow margins. The actual window
        // frame and its controls must fit the output; log the surface separately.
        let frame = chooser.compute_bounds(&chooser).unwrap();
        if frame.width() > output.width() as f32 || frame.height() > output.height() as f32 {
            record_failure(
                &mut failures,
                format!(
                    "native color chooser frame {}x{} exceeds phone output {}x{}",
                    frame.width(),
                    frame.height(),
                    output.width(),
                    output.height()
                ),
            );
        }
    }
    if let Some(dialog) = chooser.downcast_ref::<gtk::Dialog>() {
        let cancel = dialog
            .widget_for_response(gtk::ResponseType::Cancel)
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        assert!(cancel.is_mapped() && cancel.is_sensitive());
        check_bounds(
            &cancel,
            &chooser,
            "native color editor Cancel",
            true,
            &mut failures,
        );
        cancel.emit_clicked();
    } else {
        chooser.close();
    }
    settle();
    assert!(!chooser.is_visible());
    assert_eq!(
        button.rgba(),
        original_color,
        "cancel keeps the selected color"
    );
    close(&connection);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
