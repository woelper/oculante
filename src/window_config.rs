/// Framework-agnostic window configuration.
///
/// This struct captures all window settings so that the main entry point
/// can convert it to whichever framework is in use.
pub struct WindowSettings {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub decorations: bool,
    pub min_size: Option<(u32, u32)>,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            title: format!("Oculante | {}", crate::utils::app_version()),
            width: 1026,
            height: 600,
            vsync: true,
            decorations: true,
            min_size: None,
        }
    }
}

/// Build window settings from the current platform and user preferences.
pub fn build_window_settings() -> WindowSettings {
    let mut ws = WindowSettings::default();

    // Apply saved window geometry
    if let Ok(volatile_settings) = crate::settings::VolatileSettings::load()
        && volatile_settings.window_geometry != Default::default()
    {
        ws.width = volatile_settings.window_geometry.1.0;
        ws.height = volatile_settings.window_geometry.1.1;
    }

    // Apply persistent settings
    if let Ok(settings) = crate::settings::PersistentSettings::load() {
        ws.vsync = settings.vsync;
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
    }

    ws
}
