//! Opt-in terminal checks against one disposable, explicitly selected container.
//! The guest runner creates and removes the isolated daemon and container.

use std::cell::RefCell;
use std::os::unix::fs::FileTypeExt;
use std::rc::Rc;

use vte4::TerminalExt;

use super::*;

fn contents(terminal: &vte4::Terminal) -> String {
    let stream = gio::MemoryOutputStream::new_resizable();
    terminal
        .write_contents_sync(&stream, vte4::WriteFlags::Default, gio::Cancellable::NONE)
        .unwrap();
    stream.close(gio::Cancellable::NONE).unwrap();
    String::from_utf8_lossy(&stream.steal_as_bytes()).into_owned()
}

fn wait_for(description: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        settle();
    }
    settle();
}

fn commit(terminal: &vte4::Terminal, text: &str) {
    // Exercise the same VTE commit signal used by text input. Physical input
    // methods and touch event delivery are separate device checks.
    terminal.emit_by_name::<()>("commit", &[&text, &(text.len() as u32)]);
}

fn wait_output(window: &adw::Window, terminal: &vte4::Terminal, directory: &Path, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !contents(terminal).contains(marker) {
        if Instant::now() >= deadline {
            std::fs::write(directory.join("terminal-failure.txt"), contents(terminal)).unwrap();
            capture(window, directory, "terminal-failure");
            panic!("terminal did not return {marker}; captured its actual output");
        }
        settle();
    }
    settle();
}

fn touch_button(page: &view::ContainerTerminalPage, action: &str) -> gtk::Button {
    descendants(&find_id(page, "touch_controls"))
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.action_name().as_deref() == Some(action))
        .unwrap()
}

#[test]
#[ignore = "requires an isolated guest Docker socket, one disposable container and a display"]
fn live_terminal_keeps_input_and_resize() {
    assert_eq!(
        std::env::var("PODS_TEST_ISOLATED_DOCKER").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("GSETTINGS_BACKEND").as_deref(), Ok("memory"));
    let host = std::env::var("PODS_TEST_DOCKER_HOST").expect("explicit isolated Docker socket");
    let socket = Path::new(
        host.strip_prefix("unix://")
            .expect("local Unix socket only"),
    );
    assert!(socket.is_absolute());
    assert!(socket != Path::new("/run/docker.sock") && socket != Path::new("/var/run/docker.sock"));
    assert!(socket.metadata().unwrap().file_type().is_socket());
    let container_id =
        std::env::var("PODS_UI_TEST_CONTAINER_ID").expect("one disposable container ID");
    assert!(container_id.len() == 64 && container_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let process_status = std::fs::read_to_string("/proc/self/status").unwrap();
    let uid = process_status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .unwrap();
    assert!(uid.split_whitespace().all(|value| value != "0"));
    eprintln!("Live terminal guest UIDs:{uid}; socket={host}; container={container_id}");

    adw::init().unwrap();
    sourceview5::init();
    gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR).unwrap();
    gettextrs::textdomain(config::GETTEXT_PACKAGE).unwrap();
    let directory = std::env::var_os("PODS_UI_TEST_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pods-live-terminal"));
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
    app.activate();
    app.main_window().set_visible(false);
    let window = adw::Window::builder()
        .application(&app)
        .decorated(false)
        .build();
    let display = gtk::gdk::Display::default().unwrap();
    eprintln!("Live terminal GTK backend: {}", display.type_().name());

    let manager = model::ConnectionManager::default();
    let connection = model::Connection::new(
        "live-terminal",
        "Isolated terminal test",
        &host,
        None,
        &manager,
    );
    let client: model::Client = glib::Object::builder()
        .property("connection", &connection)
        .property(
            "engine",
            model::Engine::from(engine::Engine::Docker(
                bollard::Docker::connect_with_host(&host).unwrap(),
            )),
        )
        .build();
    let refresh_error = Rc::new(RefCell::new(None));
    client.containers().refresh({
        let refresh_error = refresh_error.clone();
        move |error| {
            refresh_error.replace(Some(error.to_string()));
        }
    });
    wait_for("isolated container refresh", || {
        client.containers().initialized() || refresh_error.borrow().is_some()
    });
    assert!(
        refresh_error.borrow().is_none(),
        "refresh: {:?}",
        refresh_error.borrow()
    );
    let container = client.containers().get_container(&container_id).unwrap();
    let inspection = Rc::new(RefCell::new(None));
    let api = container.api().unwrap();
    crate::rt::Promise::new(async move { api.inspect().await }).defer({
        let inspection = inspection.clone();
        move |result| {
            inspection.replace(Some(result));
        }
    });
    wait_for("isolated container inspection", || {
        inspection.borrow().is_some()
    });
    container.update_from_inspection(inspection.borrow_mut().take().unwrap().unwrap());
    assert_eq!(container.status(), model::ContainerStatus::Running);

    let navigation = adw::NavigationView::new();
    let home = adw::NavigationPage::new(&gtk::Box::new(gtk::Orientation::Vertical, 0), "Home");
    navigation.add(&home);
    window.set_content(Some(&navigation));
    resize(&window, 366, 651);
    let page = view::ContainerTerminalPage::from(&container);
    navigation.push(&page);
    settle();
    let terminal: view::ContainerTerminal = find(&page);
    let vte: vte4::Terminal = find(&terminal);
    wait_for("attached shell prompt", || {
        !contents(&vte).trim().is_empty()
    });
    assert!(
        vte.pty().is_none(),
        "container stream must use the existing no-PTY path"
    );

    commit(&vte, "printf '%s%s\\n' 'PODS_' 'COMMIT_OK'\r");
    wait_output(&window, &vte, &directory, "PODS_COMMIT_OK");
    eprintln!("VTE commit reached the real container shell");

    for (index, (width, height)) in [(360, 651), (366, 651), (651, 366), (366, 300), (1024, 768)]
        .into_iter()
        .enumerate()
    {
        resize(&window, width, height);
        assert_fits(&vte, &page);
        let rows = vte.row_count();
        let columns = vte.column_count();
        commit(&vte, &format!("printf 'PODS_SIZE_{index} '; stty size\r"));
        let expected = format!("PODS_SIZE_{index} {rows} {columns}");
        wait_output(&window, &vte, &directory, &expected);
        capture(
            &window,
            &directory,
            &format!("live-terminal-{width}x{height}"),
        );
        std::fs::write(
            directory.join(format!("live-terminal-{width}x{height}.txt")),
            contents(&vte),
        )
        .unwrap();
        eprintln!(
            "terminal {width}x{height}: VTE and container stty agree on {rows} rows, {columns} columns"
        );
    }

    resize(&window, 366, 651);
    vte.grab_focus();
    prepare_popup_input();
    display
        .clipboard()
        .set_text("printf '%s%s\\n' 'PODS_' 'PASTE_OK'");
    touch_button(&page, "container-terminal-page.paste").emit_clicked();
    wait_output(&window, &vte, &directory, "PASTE_OK");
    commit(&vte, "\r");
    wait_output(&window, &vte, &directory, "PODS_PASTE_OK");
    capture(&window, &directory, "live-terminal-paste");
    eprintln!("Clipboard paste reached the shell through the existing VTE paste path");

    for (name, action, length, expected) in [
        ("ESC", "escape", 1, "1b"),
        ("TAB", "tab", 1, "09"),
        ("UP", "previous-command", 3, "1b 5b 41"),
        ("DOWN", "next-command", 3, "1b 5b 42"),
        ("CTRL_C", "interrupt", 1, "03"),
    ] {
        commit(
            &vte,
            &format!(
                "stty -echo -icanon -isig min 1 time 0; printf '%s%s\\n' 'PODS_READY_' '{name}'; od -An -tx1 -N {length}; stty sane; printf '%s%s\\n' 'PODS_DONE_' '{name}'\r"
            ),
        );
        wait_output(&window, &vte, &directory, &format!("PODS_READY_{name}"));
        touch_button(&page, &format!("container-terminal-page.{action}")).emit_clicked();
        wait_output(&window, &vte, &directory, &format!("PODS_DONE_{name}"));
        let output = contents(&vte);
        assert!(
            output
                .lines()
                .any(|line| line.split_whitespace().collect::<Vec<_>>().join(" ") == expected),
            "{name} must deliver its exact escape/control bytes to the container"
        );
        std::fs::write(directory.join(format!("terminal-key-{name}.txt")), output).unwrap();
        eprintln!("Touch action {action}: real container received {expected}");
    }

    commit(&vte, "sleep 30\r");
    wait_output(&window, &vte, &directory, "sleep 30");
    touch_button(&page, "container-terminal-page.interrupt").emit_clicked();
    commit(&vte, "printf '%s%s\\n' 'PODS_' 'INTERRUPT_OK'\r");
    wait_output(&window, &vte, &directory, "PODS_INTERRUPT_OK");
    eprintln!("Ctrl+C interrupted the running foreground command");

    commit(&vte, "printf '%s%s\\n' 'PODS_' 'COPY_OK'\r");
    wait_output(&window, &vte, &directory, "PODS_COPY_OK");
    vte.select_all();
    terminal
        .activate_action("container-terminal.copy", None)
        .unwrap();
    let copied = Rc::new(RefCell::new(None));
    display
        .clipboard()
        .read_text_async(gio::Cancellable::NONE, {
            let copied = copied.clone();
            move |result| {
                copied.replace(Some(result));
            }
        });
    wait_for("terminal clipboard copy", || copied.borrow().is_some());
    assert!(
        copied
            .borrow_mut()
            .take()
            .unwrap()
            .unwrap()
            .unwrap()
            .contains("PODS_COPY_OK")
    );
    vte.unselect_all();
    capture(&window, &directory, "live-terminal-input-complete");
    std::fs::write(directory.join("live-terminal-complete.txt"), contents(&vte)).unwrap();
    terminal.terminate();
    settle();
    window.close();
    app.quit();
    println!("Live terminal commit, resize, clipboard, touch-key bytes and interrupt passed");
}
