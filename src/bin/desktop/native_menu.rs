//! Native macOS presentation for the guest Menu Manager.
//!
//! AppKit owns only presentation and keyboard-equivalent discovery.  Actions
//! are queued back to the GUI runner, which routes them through the guest's
//! ordinary mouseDown -> FindWindow -> MenuSelect path.

use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::sync::{Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSProcessInfo, NSString};
use systemless::menu_model::{GuestMenu, GuestMenuSnapshot};

static COMMANDS: OnceLock<Mutex<VecDeque<(i16, i16)>>> = OnceLock::new();

#[repr(C)]
struct ProcessSerialNumber {
    high_long_of_psn: u32,
    low_long_of_psn: u32,
}

type GetCurrentProcessFn = unsafe extern "C" fn(*mut ProcessSerialNumber) -> i32;
type SetProcessNameFn = unsafe extern "C" fn(*const ProcessSerialNumber, *const c_char) -> i32;

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// Set the name registered with the macOS process manager. AppKit renders the
/// special first menu-bar slot from this identity rather than the NSMenuItem's
/// title. These compatibility symbols remain available to unbundled Cocoa
/// applications but are loaded dynamically so Systemless still starts if a
/// future macOS release removes them.
fn set_process_display_name(name: &str) -> bool {
    let Ok(name) = CString::new(name) else {
        return false;
    };
    // RTLD_DEFAULT on Darwin. Both functions are supplied by the system
    // frameworks already loaded by AppKit/winit.
    let default_handle = (-2_isize) as *mut c_void;
    let get_current = unsafe { dlsym(default_handle, c"GetCurrentProcess".as_ptr()) };
    let set_name = unsafe { dlsym(default_handle, c"CPSSetProcessName".as_ptr()) };
    if get_current.is_null() || set_name.is_null() {
        return false;
    }
    let get_current: GetCurrentProcessFn = unsafe { std::mem::transmute(get_current) };
    let set_name: SetProcessNameFn = unsafe { std::mem::transmute(set_name) };
    let mut psn = ProcessSerialNumber {
        high_long_of_psn: 0,
        low_long_of_psn: 0,
    };
    unsafe { get_current(&mut psn) == 0 && set_name(&psn, name.as_ptr()) == 0 }
}

fn commands() -> &'static Mutex<VecDeque<(i16, i16)>> {
    COMMANDS.get_or_init(|| Mutex::new(VecDeque::new()))
}

declare_class!(
    struct GuestMenuTarget;

    // SAFETY: NSObject has no subclassing requirements.  AppKit invokes menu
    // targets on the main thread and this object has no Drop implementation.
    unsafe impl ClassType for GuestMenuTarget {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "SystemlessGuestMenuTarget";
    }

    impl DeclaredClass for GuestMenuTarget {
        type Ivars = ();
    }

    unsafe impl NSObjectProtocol for GuestMenuTarget {}

    unsafe impl GuestMenuTarget {
        #[method(systemlessGuestMenuItemSelected:)]
        fn menu_item_selected(&self, sender: &NSMenuItem) {
            // NSInteger is 64-bit on every macOS target supported by winit.
            // The upper word preserves negative classic menu IDs verbatim.
            let packed = unsafe { sender.tag() } as u32;
            let menu_id = (packed >> 16) as u16 as i16;
            let item = packed as u16 as i16;
            commands().lock().unwrap().push_back((menu_id, item));
        }
    }
);

impl GuestMenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(());
        unsafe { msg_send_id![super(this), init] }
    }
}

pub struct NativeMenuBridge {
    app_name: String,
    target: Option<Retained<GuestMenuTarget>>,
    main_menu: Option<Retained<NSMenu>>,
    guest_menus: Vec<(Retained<NSMenu>, isize)>,
    last_snapshot: GuestMenuSnapshot,
    installed: bool,
}

impl NativeMenuBridge {
    pub fn new(app_name: String) -> Self {
        Self {
            app_name,
            target: None,
            main_menu: None,
            guest_menus: Vec::new(),
            last_snapshot: GuestMenuSnapshot::default(),
            installed: false,
        }
    }

    /// Set the fallback application-menu name from the exact executable that
    /// the guest loader selected inside the archive or disk image.
    pub fn set_app_name(&mut self, app_name: String) {
        if !set_process_display_name(&app_name) {
            eprintln!(
                "[SYSTEMLESS] macOS did not expose the process-name compatibility API; \
                 using the native menu-item fallback"
            );
        }
        self.app_name = app_name;
    }

    pub fn drain_commands(&self) -> Vec<(i16, i16)> {
        commands().lock().unwrap().drain(..).collect()
    }

    pub fn sync(&mut self, snapshot: GuestMenuSnapshot) {
        self.strip_host_items();
        if self.installed && snapshot == self.last_snapshot {
            return;
        }

        let roots: Vec<&GuestMenu> = snapshot
            .menus
            .iter()
            .filter(|menu| menu.visible_in_menu_bar)
            .collect();
        // Do not replace winit's default application menu during the short
        // interval before the guest calls InsertMenu/DrawMenuBar.
        if roots.is_empty() && !self.installed {
            self.last_snapshot = snapshot;
            return;
        }

        let mtm = MainThreadMarker::new().expect("menu sync must run on the main thread");
        if self.target.is_none() {
            self.target = Some(GuestMenuTarget::new(mtm));
        }
        let main_menu = new_menu("Systemless", mtm);
        let mut root_items = Vec::new();
        let mut guest_process_name = None;
        let mut guest_menus = Vec::new();
        for (root_index, guest_menu) in roots.into_iter().enumerate() {
            let looks_like_apple_menu = root_index == 0
                && guest_menu.items.first().is_some_and(|item| {
                    item.text
                        .trim_start()
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("about"))
                });
            let root_title = if looks_like_apple_menu {
                guest_application_name(guest_menu).unwrap_or(self.app_name.as_str())
            } else {
                guest_menu.title.as_str()
            };
            let root_item = new_item(root_title, None, "", mtm);
            unsafe { root_item.setEnabled(guest_menu.enabled) };
            let submenu = self.build_menu(
                &snapshot,
                guest_menu,
                &mut Vec::new(),
                &mut guest_menus,
                mtm,
            );
            unsafe { submenu.setTitle(&NSString::from_str(root_title)) };
            root_item.setSubmenu(Some(&submenu));
            // setSubmenu synchronizes the parent item to the NSMenu's hidden
            // internal title. Restore the guest-visible title afterwards.
            unsafe { root_item.setTitle(&NSString::from_str(root_title)) };
            main_menu.addItem(&root_item);
            if looks_like_apple_menu {
                guest_process_name = Some(root_title.to_owned());
            }
            root_items.push((root_item, root_title.to_owned()));
        }

        let app = NSApplication::sharedApplication(mtm);
        if let Some(title) = guest_process_name.as_deref() {
            let _ = set_process_display_name(title);
            unsafe { NSProcessInfo::processInfo().setProcessName(&NSString::from_str(title)) };
        }
        app.setMainMenu(Some(&main_menu));
        // AppKit replaces the first main-menu item's title with the process
        // name and synchronizes the remaining items from their NSMenu titles
        // while installing a new main menu. Apply every guest-visible title
        // afterwards.
        for (root_item, title) in root_items {
            unsafe { root_item.setTitle(&NSString::from_str(&title)) };
        }
        self.guest_menus = guest_menus;
        self.strip_host_items();
        self.main_menu = Some(main_menu);
        self.last_snapshot = snapshot;
        self.installed = true;
    }

    fn build_menu(
        &self,
        snapshot: &GuestMenuSnapshot,
        menu: &GuestMenu,
        ancestors: &mut Vec<i16>,
        guest_menus: &mut Vec<(Retained<NSMenu>, isize)>,
        mtm: MainThreadMarker,
    ) -> Retained<NSMenu> {
        let native = new_menu(&menu.title, mtm);
        if ancestors.contains(&menu.id) {
            return native;
        }
        ancestors.push(menu.id);

        for item in &menu.items {
            if item.separator {
                native.addItem(&NSMenuItem::separatorItem(mtm));
                continue;
            }

            if let Some(submenu_id) = item.submenu_id {
                let native_item = new_item(&item.text, None, "", mtm);
                unsafe { native_item.setEnabled(menu.enabled && item.enabled) };
                if let Some(submenu) = snapshot
                    .menus
                    .iter()
                    .find(|candidate| candidate.id == submenu_id && candidate.hierarchical)
                {
                    let child = self.build_menu(snapshot, submenu, ancestors, guest_menus, mtm);
                    native_item.setSubmenu(Some(&child));
                }
                native.addItem(&native_item);
                continue;
            }

            let key = item
                .key_equivalent
                .filter(|key| !key.is_control())
                .map(|key| key.to_string())
                .unwrap_or_default();
            let native_item = new_item(
                &item.text,
                Some(sel!(systemlessGuestMenuItemSelected:)),
                &key,
                mtm,
            );
            let packed = ((menu.id as u16 as u32) << 16) | item.number as u16 as u32;
            unsafe {
                native_item.setTarget(Some(
                    &**self
                        .target
                        .as_ref()
                        .expect("native menu target initialized before building menus"),
                ));
                native_item.setTag(packed as isize);
                native_item.setEnabled(menu.enabled && item.enabled);
                native_item.setState(if item.checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
            native.addItem(&native_item);
        }

        ancestors.pop();
        guest_menus.push((native.clone(), menu.items.len() as isize));
        native
    }

    fn strip_host_items(&self) {
        for (menu, guest_count) in &self.guest_menus {
            // AppKit appends text-service commands to a conventional Edit
            // menu. They are host features, not Menu Manager items, and must
            // never be observable as guest commands.
            unsafe {
                while menu.numberOfItems() > *guest_count {
                    menu.removeItemAtIndex(*guest_count);
                }
            }
        }
    }
}

fn guest_application_name(menu: &GuestMenu) -> Option<&str> {
    let about = menu.items.first()?.text.trim();
    let (prefix, name) = about.split_at_checked(6)?;
    if !prefix.eq_ignore_ascii_case("About ") {
        return None;
    }
    let name = name
        .strip_suffix('…')
        .or_else(|| name.strip_suffix("..."))
        .unwrap_or(name)
        .trim();
    (!name.is_empty()).then_some(name)
}

fn new_menu(title: &str, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let title = NSString::from_str(title);
    let menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &title) };
    unsafe { menu.setAutoenablesItems(false) };
    menu
}

fn new_item(
    title: &str,
    action: Option<objc2::runtime::Sel>,
    key: &str,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    let key = NSString::from_str(key);
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &title, action, &key) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::menu_model::GuestMenuItem;

    fn apple_menu(about_title: &str) -> GuestMenu {
        GuestMenu {
            id: 1,
            title: "Apple".to_owned(),
            enabled: true,
            hierarchical: false,
            visible_in_menu_bar: true,
            items: vec![GuestMenuItem {
                number: 1,
                text: about_title.to_owned(),
                enabled: true,
                checked: false,
                key_equivalent: None,
                submenu_id: None,
                separator: false,
            }],
        }
    }

    #[test]
    fn application_name_accepts_classic_ellipsis_forms() {
        assert_eq!(
            guest_application_name(&apple_menu("About Example App...")),
            Some("Example App")
        );
        assert_eq!(
            guest_application_name(&apple_menu("About Example App…")),
            Some("Example App")
        );
    }

    #[test]
    fn application_name_accepts_about_item_without_ellipsis() {
        assert_eq!(
            guest_application_name(&apple_menu("About Example App")),
            Some("Example App")
        );
    }

    #[test]
    fn application_name_rejects_non_about_items() {
        assert_eq!(guest_application_name(&apple_menu("System Info")), None);
        assert_eq!(guest_application_name(&apple_menu("About")), None);
    }
}
