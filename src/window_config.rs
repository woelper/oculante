/// Framework-agnostic window configuration.
///
/// This struct captures all window settings so that the main entry point
/// can convert it to whichever framework is in use (notan today, eframe later).

pub struct WindowSettings {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub vsync: bool,
    pub lazy_loop: bool,
    pub high_dpi: bool,
    pub decorations: bool,
    pub always_on_top: bool,
    pub multisampling: u8,
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
    pub app_id: String,
    pub icon_data: Option<&'static [u8]>,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            title: format!("Oculante | {}", env!("CARGO_PKG_VERSION")),
            width: 1026,
            height: 600,
            resizable: true,
            vsync: true,
            lazy_loop: true,
            high_dpi: true,
            decorations: true,
            always_on_top: true,
            multisampling: 0,
            min_size: None,
            max_size: None,
            app_id: "oculante".into(),
            icon_data: None,
        }
    }
}

/// Build window settings from the current platform and user preferences.
pub fn build_window_settings() -> WindowSettings {
    let icon_data: &'static [u8] = include_bytes!("../icon.ico");

    let mut ws = WindowSettings {
        icon_data: Some(icon_data),
        ..Default::default()
    };

    // Platform-specific high DPI — NetBSD/FreeBSD don't support it
    #[cfg(any(target_os = "netbsd", target_os = "freebsd"))]
    {
        ws.high_dpi = false;
    }

    // Apply saved window geometry
    if let Ok(volatile_settings) = crate::settings::VolatileSettings::load() {
        if volatile_settings.window_geometry != Default::default() {
            ws.width = volatile_settings.window_geometry.1 .0;
            ws.height = volatile_settings.window_geometry.1 .1;
        }
    }

    // Apply persistent settings
    if let Ok(settings) = crate::settings::PersistentSettings::load() {
        ws.vsync = settings.vsync;
        ws.lazy_loop = !settings.force_redraw;
        ws.decorations = !settings.borderless;
        ws.min_size = Some(settings.min_window_size);

        if settings.zen_mode {
            ws.title.push_str(&format!(
                "          '{}' to disable zen mode",
                crate::shortcuts::lookup(
                    &settings.shortcuts,
                    &crate::shortcuts::InputEvent::ZenMode
                )
            ));
        }

        // LIBHEIF_SECURITY_LIMITS needs to be set before a libheif context is created
        #[cfg(feature = "heif")]
        settings.decoders.heif.maybe_limits();
    }

    ws
}
