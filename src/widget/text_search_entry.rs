use std::cell::Cell;
use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::CompositeTemplate;
use gtk::glib;

use crate::utils;

const LAYOUT_SPACING: i32 = 6;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::TextSearchEntry)]
    #[template(resource = "/com/github/marhkb/Pods/ui/widget/text_search_entry.ui")]
    pub(crate) struct TextSearchEntry {
        #[property(get, set)]
        pub(super) case_sensitive: Cell<bool>,
        #[property(get, set = Self::set_compact, explicit_notify)]
        pub(super) compact: Cell<bool>,
        #[property(get, set)]
        pub(super) info: RefCell<String>,
        #[property(get, set)]
        pub(super) regex: Cell<bool>,
        #[property(get, set)]
        pub(super) whole_word: Cell<bool>,

        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) text: TemplateChild<gtk::Text>,
        #[template_child]
        pub(super) info_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) options_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) options_button: TemplateChild<gtk::MenuButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TextSearchEntry {
        const NAME: &'static str = "PdsTextSearchEntry";
        type Type = super::TextSearchEntry;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Editable,);

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();

            klass.set_css_name("entry");
            klass.set_accessible_role(gtk::AccessibleRole::TextBox);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TextSearchEntry {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            if id <= Self::derived_properties().len() {
                self.derived_set_property(id, value, pspec);
            } else {
                self.delegate_set_property(id, value, pspec);
            }
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            if id <= Self::derived_properties().len() {
                self.derived_property(id, pspec)
            } else {
                self.delegate_get_property(id, pspec).unwrap()
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = &*self.obj();

            let settings = utils::PodsSettings::default();
            settings.bind("search-regex", obj, "regex").build();
            settings
                .bind("search-case-sensitive", obj, "case-sensitive")
                .build();
            settings
                .bind("search-whole-word", obj, "whole-word")
                .build();
        }

        fn dispose(&self) {
            utils::unparent_children(&*self.obj());
        }
    }

    impl WidgetImpl for TextSearchEntry {
        fn grab_focus(&self) -> bool {
            self.text.grab_focus()
        }

        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::WidthForHeight
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                let (min_image, nat_image, _, _) = self.image.measure(orientation, for_size);
                let (min_text, nat_text, _, _) = self.text.measure(orientation, for_size);
                let (min_options, nat_options, _, _) =
                    self.options_widget().measure(orientation, for_size);
                let (min_info, nat_info, _, _) = self.info_label.measure(orientation, for_size);

                (
                    LAYOUT_SPACING * 3 + min_image + min_options + min_text + min_info,
                    LAYOUT_SPACING * 3 + nat_image + nat_options + nat_text + nat_info,
                    -1,
                    -1,
                )
            } else {
                [
                    self.image.upcast_ref::<gtk::Widget>(),
                    self.text.upcast_ref(),
                    self.info_label.upcast_ref(),
                    self.options_widget(),
                ]
                .into_iter()
                .fold((0, 0, -1, -1), |(min, nat, _, _), child| {
                    let (child_min, child_nat, _, _) = child.measure(orientation, for_size);
                    (min.max(child_min), nat.max(child_nat), -1, -1)
                })
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let image_width = self.image.preferred_size().1.width().min(width.max(0));
            let options = self.options_widget();
            let remaining = (width - image_width - LAYOUT_SPACING * 3).max(0);
            let options_width = options.preferred_size().1.width().min(remaining);
            let remaining = remaining - options_width;
            // Keep space for typing even when the translated match count is long.
            let info_label_width = self
                .info_label
                .preferred_size()
                .1
                .width()
                .min(remaining / 3);

            let text_width = remaining - info_label_width;

            self.image
                .size_allocate(&gtk::Allocation::new(0, 0, image_width, height), baseline);

            self.text.size_allocate(
                &gtk::Allocation::new(image_width + LAYOUT_SPACING, 0, text_width, height),
                baseline,
            );

            self.info_label.size_allocate(
                &gtk::Allocation::new(
                    image_width + text_width + LAYOUT_SPACING * 2,
                    0,
                    info_label_width,
                    height,
                ),
                baseline,
            );

            options.size_allocate(
                &gtk::Allocation::new(
                    image_width + text_width + info_label_width + LAYOUT_SPACING * 3,
                    0,
                    options_width,
                    height,
                ),
                baseline,
            );
        }
    }

    impl EditableImpl for TextSearchEntry {
        fn delegate(&self) -> Option<gtk::Editable> {
            Some(self.text.clone().upcast())
        }
    }

    #[gtk::template_callbacks]
    impl TextSearchEntry {
        fn options_widget(&self) -> &gtk::Widget {
            if self.compact.get() {
                self.options_button.upcast_ref()
            } else {
                self.options_box.upcast_ref()
            }
        }

        fn set_compact(&self, compact: bool) {
            if self.compact.replace(compact) == compact {
                return;
            }

            self.options_box.set_visible(!compact);
            self.options_button.set_visible(compact);
            if !compact {
                self.options_button.popdown();
            }
            self.obj().queue_resize();
            self.obj().notify_compact();
        }

        #[template_callback]
        fn on_text_notify(&self, pspec: &glib::ParamSpec) {
            let obj = &*self.obj();
            if obj.has_property(pspec.name()) {
                obj.notify(pspec.name());
            }
        }
    }
}

glib::wrapper! {
    pub(crate) struct TextSearchEntry(ObjectSubclass<imp::TextSearchEntry>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Editable;
}
