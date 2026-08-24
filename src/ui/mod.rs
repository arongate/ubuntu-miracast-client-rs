//! GTK4 + libadwaita application UI.
//!
//! Implements the main window with adaptive layout using AdwNavigationSplitView,
//! AdwBreakpoint for responsive design, and standard GNOME application patterns.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use libadwaita as adw;
use libadwaita::prelude::*;

/// Build the main application UI.
pub fn build_ui(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Miracast Client")
        .default_width(900)
        .default_height(600)
        .width_request(360)
        .height_request(294)
        .build();

    // Main content with AdwToolbarView
    let toolbar_view = adw::ToolbarView::new();

    // Header bar
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    // Main content area
    let content = build_main_content();
    toolbar_view.set_content(Some(&content));

    window.set_content(Some(&toolbar_view));
    window.present();
}

/// Build the main content with status page (initial state).
fn build_main_content() -> gtk::Widget {
    let stack = adw::ViewStack::new();

    // Devices page
    let devices_page = build_devices_page();
    stack.add_titled(&devices_page, Some("devices"), "Devices");

    // Streaming page (shown when casting)
    let streaming_page = build_streaming_page();
    stack.add_titled(&streaming_page, Some("streaming"), "Streaming");

    // Settings page
    let settings_page = build_settings_page();
    stack.add_titled(&settings_page, Some("settings"), "Settings");

    // View switcher bar at bottom
    let switcher_bar = adw::ViewSwitcherBar::new();
    switcher_bar.set_stack(Some(&stack));
    switcher_bar.set_reveal(true);

    let content_box = gtk::Box::new(Orientation::Vertical, 0);
    content_box.append(&stack);
    content_box.append(&switcher_bar);

    content_box.upcast()
}

/// Build the device discovery/selection page.
fn build_devices_page() -> gtk::Widget {
    let page = adw::StatusPage::builder()
        .icon_name("network-wireless-symbolic")
        .title("Discover Devices")
        .description("Search for Miracast receivers on your network")
        .build();

    // Scan button
    let scan_button = gtk::Button::builder()
        .label("Start Scanning")
        .halign(Align::Center)
        .css_classes(["suggested-action", "pill"])
        .build();

    scan_button.connect_clicked(|button| {
        button.set_label("Scanning...");
        button.set_sensitive(false);
        // TODO: Trigger discovery.start() via app state
    });

    page.set_child(Some(&scan_button));
    page.upcast()
}

/// Build the streaming status page.
fn build_streaming_page() -> gtk::Widget {
    let page = adw::StatusPage::builder()
        .icon_name("media-playback-start-symbolic")
        .title("Not Casting")
        .description("Select a device to start casting")
        .build();

    page.upcast()
}

/// Build the settings page.
fn build_settings_page() -> gtk::Widget {
    let page = adw::PreferencesPage::new();

    // Video group
    let video_group = adw::PreferencesGroup::builder()
        .title("Video")
        .description("Configure video streaming settings")
        .build();

    let quality_row = adw::ActionRow::builder()
        .title("Video Quality")
        .subtitle("High (10 Mbps)")
        .build();
    video_group.add(&quality_row);

    let fps_row = adw::ActionRow::builder()
        .title("Frame Rate")
        .subtitle("30 fps")
        .build();
    video_group.add(&fps_row);

    page.add(&video_group);

    // Audio group
    let audio_group = adw::PreferencesGroup::builder()
        .title("Audio")
        .build();

    let audio_row = adw::ActionRow::builder()
        .title("Stream Audio")
        .subtitle("Include audio in the cast")
        .build();

    let audio_switch = gtk::Switch::builder()
        .active(true)
        .valign(Align::Center)
        .build();
    audio_row.add_suffix(&audio_switch);
    audio_row.set_activatable_widget(Some(&audio_switch));
    audio_group.add(&audio_row);

    page.add(&audio_group);

    // Advanced group
    let advanced_group = adw::PreferencesGroup::builder()
        .title("Advanced")
        .build();

    let timeout_row = adw::ActionRow::builder()
        .title("Discovery Timeout")
        .subtitle("10 seconds")
        .build();
    advanced_group.add(&timeout_row);

    page.add(&advanced_group);

    page.upcast()
}
