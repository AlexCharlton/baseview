use cocoa::{
    appkit::{NSApp, NSApplication, NSMenu, NSMenuItem},
    base::nil,
    foundation::{NSProcessInfo, NSString},
};
use objc::{sel, sel_impl};

macro_rules! ns_string {
    ($s:expr) => {
        cocoa::foundation::NSString::alloc(nil).init_str($s)
    };
}

// TODO configurable menus
pub fn initialize() {
    unsafe {
        let menubar = NSMenu::new(nil);
        let app_menu_item = NSMenuItem::new(nil);
        menubar.addItem_(app_menu_item);

        let app_menu = NSMenu::new(nil);
        let process_name = NSProcessInfo::processInfo(nil).processName();

        // Quit application menu item
        let quit_item_title = ns_string!("Quit ").stringByAppendingString_(process_name);
        let quit_item = NSMenuItem::new(nil).initWithTitle_action_keyEquivalent_(
            quit_item_title,
            sel!(terminate:),
            ns_string!("q"),
        );
        app_menu.addItem_(quit_item);
        app_menu_item.setSubmenu_(app_menu);

        let app = NSApp();
        app.setMainMenu_(menubar);
    }
}
