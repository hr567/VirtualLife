//! Browser API adaptation for the Bevy application. Game state and drawing are
//! owned by `virtual_life_app`; this crate only executes platform requests.

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
pub use browser::BrowserPlatformPlugin;

/// Native builds validate the web package without opening a browser or window.
#[cfg(not(target_arch = "wasm32"))]
pub struct BrowserPlatformPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl bevy::prelude::Plugin for BrowserPlatformPlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {}
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    let mut app = virtual_life_app::build_app_with_display(browser::use_canvas_2d());
    app.add_plugins(BrowserPlatformPlugin);
    app.run();
}
