# Oculante: Notan Removal Plan

## Goal
Remove the `notan` dependency entirely and replace it with `eframe` (egui's native framework) + `glow` backend. The app must keep working between every chunk. Each chunk is a reviewable, commitable unit.

## Architecture Overview (Current → Target)

```
CURRENT                              TARGET
─────────────────────────            ─────────────────────────
notan (windowing, events,            eframe (windowing, events,
       glow, egui plugin)                   glow, egui built-in)
  ↓                                    ↓
main.rs (1359 lines)                 main.rs (app setup only)
  - init, events, update, draw        - eframe::run_native()
  - all mixed together                 ↓
                                     app.rs (eframe::App impl)
                                       - update() drives everything
                                       ↓
texture_wrapper.rs                   render.rs (new)
  - notan gfx textures                - glow textures + shader
  - notan shaders                      - image drawing logic
  - notan draw API                     ↓
                                     input.rs (new)
shortcuts.rs                           - winit-agnostic input
  - reads notan App keyboard           - shortcut matching
```

## Key Technical Decisions

- **eframe with glow backend**: Keeps existing GLSL shader as-is, minimal GPU code changes
- **glow for custom rendering**: eframe exposes `gl: &glow::Context` in its paint callback — we use this for image rendering with our custom shader
- **winit stays**: Both notan and eframe use winit, so platform support is identical
- **Lazy rendering preserved**: eframe supports `ctx.request_repaint()` for on-demand rendering
- **Future option**: Switch from glow to wgpu later if needed (the abstraction layer makes this a backend swap)

---

## Phase 1: Preparation (notan still present, app keeps working)

These chunks reduce the surface area of notan usage so that the actual swap is smaller and safer.

### Chunk 1.1: Extract input abstraction — DONE

**Status**: DONE. Created `src/input.rs` with `KeyboardState` struct. `key_pressed()` now takes `&KeyboardState` instead of `&mut App`. `shortcuts.rs` has zero notan imports. `KeyboardState` stored in `OculanteState` and populated from notan each frame. All UI code (`top_bar.rs`, `settings_ui.rs`, `info_ui.rs`) updated to use `state.keyboard_state` instead of `app.keyboard`.

### Chunk 1.2: Reduce notan coupling in rendering code

**Why**: `texture_wrapper.rs` uses notan's GPU API (Graphics, Draw, Texture, Pipeline, Buffer) extensively. A full abstraction layer is impractical because notan's Draw API (image().blend_mode().scale().translate()) has no direct equivalent — it would require implementing a mini 2D renderer. Instead, we reduce coupling where possible and accept that `texture_wrapper.rs` will be rewritten in Phase 2.

**Files to change**: `src/texture_wrapper.rs`, `Cargo.toml`

**What to do**:
1. Replace `notan::math::{Mat4, Vec4}` with direct `glam` imports (notan just re-exports glam). Add `glam` as a direct dependency.
2. The remaining notan types in `texture_wrapper.rs` (Graphics, Draw, Texture, Pipeline, Buffer, ShaderSource, TextureFilter, BlendMode, Color) are all part of the Draw/GPU API and will be replaced wholesale when we rewrite with glow in Phase 2.
3. The GLSL shader source stays in `texture_wrapper.rs` for now — it will move to the glow renderer in Phase 2.

**Status**: DONE — `glam` added as direct dep, `Mat4`/`Vec4` now imported from `glam` instead of `notan::math`.

**Verify**: App compiles and runs. Images display correctly.

### Chunk 1.3: Remove notan types from AppState — DONE

**Status**: DONE. Removed `#[derive(AppState)]`, replaced with manual `impl notan::app::AppState for OculanteState {}`. The `checker_texture: Option<Texture>` field remains (uses notan Texture, will be resolved in Phase 2 render rewrite).

### Chunk 1.4: Isolate notan from UI modules — DONE

**Status**: DONE. Added `egui = "0.31"` as direct dependency. Replaced all `use notan::egui::*` with `use egui::*` across 12 files. Only 3 notan-specific egui imports remain in the codebase: `EguiConfig`, `EguiPluginSugar` (main.rs), `EguiRegisterTexture` (edit_ui.rs).

### Chunk 1.5: Isolate window management — DONE

**Status**: DONE. Created `src/window_config.rs` with framework-agnostic `WindowSettings` struct and `build_window_settings()`. Main.rs window setup reduced from ~100 lines to 2 lines. `to_notan_window_config()` bridge converts to notan's `WindowConfig`.

---

## Phase 2: The Swap (replace notan with eframe)

At this point, notan usage is confined to:
- `main.rs`: app init, event loop, draw functions
- `texture_wrapper.rs`: GPU texture management, shaders, drawing
- `utils.rs`: texture creation helpers, window operations
- `appstate.rs`: `checker_texture: Option<Texture>`, `impl AppState`
- `ui/mod.rs`, `ui/info_ui.rs`: `App`/`Graphics` parameters
- `ui/edit_ui.rs`: `EguiRegisterTexture`
- `input.rs`: `keyboard_state_from_notan()` bridge function
- `window_config.rs`: `to_notan_window_config()` bridge function

### Chunk 2.1: Add eframe dependency, implement glow Renderer backend

**Files to change**: `Cargo.toml`, `src/render.rs`

**What to do**:
1. Add `eframe = { version = "0.31", default-features = false, features = ["glow"] }` to Cargo.toml (alongside notan, temporarily).
2. Implement a second backend for the `Renderer` struct using `glow::Context` directly:
   - `create_texture` → `gl.create_texture()`, `gl.tex_image_2d()`
   - `update_texture` → `gl.tex_sub_image_2d()`
   - `create_pipeline` → compile GLSL vertex+fragment shaders, link program
   - `set_uniform_data` → `gl.uniform_matrix_4_f32_slice()` etc.
   - `draw_textured_quad` → bind texture, bind program, set uniforms, draw quad (vertex buffer with position+UV)
   - Checkerboard pattern → either a shader or a small tiling texture
3. The existing GLSL fragment shader should work as-is with glow.
4. Write a minimal test: create a glow context (headless or in a hidden window), upload a texture, verify it round-trips.

**Verify**: The glow renderer compiles. Unit tests pass if applicable.

### Chunk 2.2: Implement eframe App shell

**Files to change**: new `src/app.rs`, `src/main.rs`

**What to do**:
1. Create `src/app.rs` with a struct `OculanteApp` that implements `eframe::App`:
   ```rust
   impl eframe::App for OculanteApp {
       fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
           // This is where everything happens:
           // 1. Process input (from egui's InputState)
           // 2. Update app state
           // 3. Run egui UI
           // 4. Custom paint callback for image rendering
       }
   }
   ```
2. In the `update()` method:
   - Read input from `ctx.input()` — translate to our `InputState`
   - Call the same shortcut processing logic
   - Call the same UI functions (they already use `egui::Context`)
   - For image rendering: use `egui::PaintCallback` with a custom glow callback that invokes our `Renderer`
3. Use `egui::CentralPanel` for the image area, with a custom paint callback:
   ```rust
   let callback = egui::PaintCallback {
       rect: available_rect,
       callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
           renderer.draw_image(painter.gl(), ...);
       })),
   };
   ui.painter().add(callback);
   ```
4. Keep the old notan main.rs code intact — don't delete it yet. The new app.rs is a parallel path.

**Verify**: App can start with eframe, show a window, display egui UI. Image rendering may be stubbed initially.

### Chunk 2.3: Wire up image rendering in eframe

**Files to change**: `src/app.rs`, `src/render.rs`

**What to do**:
1. Initialize the glow `Renderer` in `OculanteApp::new()` using the `glow::Context` from eframe (available via `cc.gl()` in the creation closure).
2. Wire up texture loading: when a new image arrives via the channel, upload it through the glow `Renderer`.
3. Wire up the paint callback to draw the current texture with proper transforms (offset, scale).
4. Wire up the checker pattern background.
5. Wire up the zoom preview rendering.
6. Verify channel swizzling shader works.

**Verify**: Can open and display an image. Pan and zoom work. Channel switching works. Large tiled images display correctly.

### Chunk 2.4: Wire up input handling in eframe

**Files to change**: `src/app.rs`, `src/input.rs`

**What to do**:
1. Implement `fn from_egui_input(ctx: &egui::Context) -> InputState` in `input.rs`:
   - `ctx.input(|i| i.keys_down)` → pressed keys
   - `ctx.input(|i| i.pointer.delta())` → mouse delta
   - `ctx.input(|i| i.scroll_delta)` → scroll
   - `ctx.input(|i| i.raw.dropped_files)` → file drop
   - `ctx.input(|i| i.modifiers)` → modifier keys
2. Key name mapping: egui uses `egui::Key` enum, current shortcuts use string names like "LControl", "A", etc. Write a translation function.
3. Wire up all input: shortcuts, mouse drag, scroll zoom, file drop, double-click fullscreen.
4. Handle `key_grab` and `mouse_grab` states as before.

**Verify**: All keyboard shortcuts work. Mouse interaction works. File drop works. Scroll zoom works. Painting in edit mode works.

### Chunk 2.5: Wire up window management in eframe

**Files to change**: `src/app.rs`, `src/main.rs`

**What to do**:
1. Convert `WindowSettings` to `eframe::NativeOptions`:
   - Window title, size, min size, icon
   - VSync, DPI
   - Persistence (eframe can save/restore window position)
2. Handle window-specific operations:
   - Fullscreen toggle: `ctx.send_viewport_cmd(ViewportCommand::Fullscreen(bool))`
   - Always-on-top: `ctx.send_viewport_cmd(ViewportCommand::WindowLevel(...))`
   - Window position save/restore
   - Window resize handling
3. Handle app exit: `ctx.send_viewport_cmd(ViewportCommand::Close)`
4. Implement lazy rendering: call `ctx.request_repaint()` only when state changes (image loaded, UI interaction, animation frame).

**Verify**: Window starts at correct size. Fullscreen works. Always-on-top works. App exits cleanly. Idle CPU usage is low (lazy rendering).

### Chunk 2.6: Switch main entry point to eframe

**Files to change**: `src/main.rs`

**What to do**:
1. Replace the `#[notan_main] fn main()` with a standard `fn main()` that calls `eframe::run_native()`.
2. Delete or `#[cfg(never)]`-gate the old notan init/event/update/draw functions.
3. Remove the notan `AppState` wrapper if it still exists.
4. Verify the full application flow works end-to-end with eframe.

**Verify**: Full app works. This is the first time notan is not driving the event loop. Test extensively:
- [ ] Open image from CLI argument
- [ ] Open image from file browser
- [ ] Open image from drag-and-drop
- [ ] Pan, zoom, reset view
- [ ] All keyboard shortcuts
- [ ] Channel switching (R/G/B/A/RGB/RGBA)
- [ ] Edit mode (filters, painting)
- [ ] Animation playback
- [ ] Window resize, fullscreen, always-on-top
- [ ] Settings persistence
- [ ] Copy/paste
- [ ] Multi-monitor / HiDPI
- [ ] Idle CPU usage (lazy rendering)

---

## Phase 3: Cleanup (remove notan)

### Chunk 3.1: Remove notan dependency

**Files to change**: `Cargo.toml`, delete bridge code

**What to do**:
1. Remove `notan` from `Cargo.toml`.
2. Remove the `notan/shaderc` feature flag.
3. Delete the notan backend from `render.rs` (keep only glow).
4. Delete the notan event translator from `input.rs` (keep only egui).
5. Delete any remaining `use notan::*` imports.
6. Delete the old notan main loop code from `main.rs`.
7. Run `cargo clippy` and fix any warnings.
8. Run `cargo build` on all platforms (or CI).

**Verify**: `cargo build` succeeds with no notan references. `grep -r "notan" src/` returns nothing.

### Chunk 3.2: Clean up Cargo.toml

**What to do**:
1. The egui version is no longer pinned to notan's version — update `egui`, `egui_plot`, `egui-notify`, `egui_extras` to latest compatible versions if desired.
2. Remove any version comments referencing notan (line 40-41 in current Cargo.toml).
3. Verify feature flags are clean.

**Verify**: `cargo build` and `cargo test` pass.

---

## Risk Register

| Risk | Mitigation |
|------|------------|
| eframe glow paint callback doesn't support all needed GL state | Prototype the custom paint callback early (Chunk 2.1) before committing to the full swap |
| egui input doesn't expose all key events needed for shortcuts | Test key mapping thoroughly in Chunk 2.4. Fallback: use winit events via eframe's `raw_input` |
| Performance regression (eframe overhead vs notan) | Benchmark frame times before/after. eframe's lazy rendering should keep idle perf identical. Profile active rendering. |
| Platform-specific bugs (macOS fullscreen, Wayland, BSD) | Test on all platforms at Chunk 2.6. The macOS fullscreen key-up workaround may need adjustment. |
| Large image tiling breaks with glow backend | Test with images >8192px early in Chunk 2.3. GL texture size limits are the same regardless of framework. |
| Animation performance (GIF/WebP playback) | Animation uses `request_repaint_after()` in eframe for frame timing. Test with complex GIFs. |
| egui version mismatch between eframe and plugins | Pin all egui crates to the same minor version. eframe 0.31 bundles egui 0.31. |
| Clipboard stops working | eframe handles clipboard natively. Test copy/paste of images in Chunk 2.6. |

## Estimated Chunk Sizes

| Chunk | Files touched | Complexity | Risk |
|-------|---------------|------------|------|
| 1.1 Input abstraction | 3 | Medium | Low |
| 1.2 Render abstraction | 3 | High | Medium |
| 1.3 AppState cleanup | 2 | Low | Low |
| 1.4 UI notan isolation | ~15 | Low (mechanical) | Low |
| 1.5 Window config | 1 | Low | Low |
| 2.1 glow Renderer | 2 | High | High — prototype early |
| 2.2 eframe App shell | 2 | Medium | Medium |
| 2.3 Image rendering | 2 | High | High |
| 2.4 Input wiring | 2 | Medium | Medium |
| 2.5 Window management | 2 | Medium | Medium |
| 2.6 Entry point swap | 1 | Low | High (integration) |
| 3.1 Remove notan | 5 | Low | Low |
| 3.2 Cargo cleanup | 1 | Low | Low |

## Order of Operations Summary

```
Phase 1 (Preparation — notan still works):
  1.4 → 1.1 → 1.2 → 1.3 → 1.5
  (start with mechanical import changes, then abstractions)

Phase 2 (The Swap — eframe takes over):
  2.1 → 2.2 → 2.3 → 2.4 → 2.5 → 2.6
  (renderer first, then shell, then wire everything up)

Phase 3 (Cleanup):
  3.1 → 3.2
```

Note: Chunk 1.4 is recommended first because it's the safest, most mechanical change (just rewriting imports) and immediately reduces notan surface area across ~15 files. This gives confidence and momentum before tackling the harder abstraction work.
