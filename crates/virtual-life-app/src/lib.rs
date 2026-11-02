//! Bevy owns the entire game lifecycle. Browser services communicate through queues.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

pub mod audio;
#[cfg(feature = "web-client")]
mod client;
pub mod font;
pub mod input;
pub mod pixels;
pub mod prefetch;
pub mod presentation;
pub mod render;
pub mod runtime;
pub mod scene;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlatformRequest {
    pub id: u32,
    pub operation: String,
    pub payload: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformEvent {
    pub id: u32,
    pub operation: String,
    pub value: Value,
    pub error: Option<String>,
    pub bytes: Vec<u8>,
}
#[derive(Resource, Default)]
pub struct PlatformRequests(pub VecDeque<PlatformRequest>);
#[derive(Resource, Default)]
pub struct PlatformEvents(pub VecDeque<PlatformEvent>);

/// The browser may present the same CPU-composited pixels without a GPU.
#[derive(Resource)]
pub struct Canvas2dDisplay(pub bool);

pub fn build_app() -> App {
    build_app_with_display(false)
}

pub fn build_app_with_display(canvas_2d: bool) -> App {
    let mut app = App::new();
    #[cfg(not(feature = "web-client"))]
    app.add_plugins(MinimalPlugins);
    #[cfg(feature = "web-client")]
    client::configure(&mut app, canvas_2d);
    app.insert_resource(Canvas2dDisplay(canvas_2d));
    app.init_resource::<PlatformRequests>()
        .init_resource::<PlatformEvents>();
    app.add_plugins(runtime::GamePlugin);
    app
}
