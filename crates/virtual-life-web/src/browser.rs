use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use bevy::prelude::*;
use js_sys::{Function, Reflect, Uint8Array};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use virtual_life_app::{PlatformEvent, PlatformEvents, PlatformRequests};
use wasm_bindgen::{JsCast, prelude::*};

#[wasm_bindgen(module = "/../../web/platform.js")]
extern "C" {
    #[wasm_bindgen(js_name = createPlatform, catch)]
    fn create_platform(callback: &Function) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen(module = "/../../web/display.js")]
extern "C" {
    #[wasm_bindgen(js_name = useCanvas2d)]
    pub fn use_canvas_2d() -> bool;
    #[wasm_bindgen(js_name = presentCanvas2d, catch)]
    fn present_canvas_2d(bytes: &[u8]) -> Result<(), JsValue>;
}

/// JS handles intentionally remain on the browser thread, outside Send resources.
struct BrowserPlatform {
    handle: JsValue,
    incoming: Rc<RefCell<VecDeque<PlatformEvent>>>,
    _callback: Closure<dyn FnMut(JsValue)>,
}

#[derive(Deserialize)]
struct EventMetadata {
    id: u32,
    operation: String,
    value: Value,
    error: Option<String>,
}

impl BrowserPlatform {
    fn new() -> Result<Self, JsValue> {
        let incoming = Rc::new(RefCell::new(VecDeque::new()));
        let received = incoming.clone();
        let callback = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            // Deserialize only metadata; binary bytes never travel through JSON.
            let metadata = js_sys::Object::new();
            for field in ["id", "operation", "value", "error"] {
                if let Ok(value) = Reflect::get(&event, &JsValue::from_str(field)) {
                    let _ = Reflect::set(&metadata, &JsValue::from_str(field), &value);
                }
            }
            match serde_wasm_bindgen::from_value::<EventMetadata>(metadata.into()) {
                Ok(metadata) => {
                    let bytes = Reflect::get(&event, &JsValue::from_str("bytes"))
                        .ok()
                        .filter(|value| value.is_instance_of::<Uint8Array>())
                        .map(|value| Uint8Array::new(&value).to_vec())
                        .unwrap_or_default();
                    received.borrow_mut().push_back(PlatformEvent {
                        id: metadata.id,
                        operation: metadata.operation,
                        value: metadata.value,
                        error: metadata.error,
                        bytes,
                    });
                }
                Err(error) => received.borrow_mut().push_back(PlatformEvent {
                    id: 0,
                    operation: "platformError".into(),
                    value: Value::Null,
                    error: Some(format!("浏览器操作返回无效数据：{error}")),
                    bytes: Vec::new(),
                }),
            }
        });
        let handle = create_platform(callback.as_ref().unchecked_ref())?;
        Ok(Self {
            handle,
            incoming,
            _callback: callback,
        })
    }

    fn request(&self, id: u32, operation: &str, payload: &Value) -> Result<(), JsValue> {
        let request =
            Reflect::get(&self.handle, &JsValue::from_str("request"))?.dyn_into::<Function>()?;
        let payload = payload
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        request.call3(
            &self.handle,
            &JsValue::from_f64(f64::from(id)),
            &JsValue::from_str(operation),
            &payload,
        )?;
        Ok(())
    }
}

impl Drop for BrowserPlatform {
    fn drop(&mut self) {
        if let Ok(dispose) = Reflect::get(&self.handle, &JsValue::from_str("dispose")) {
            if let Some(dispose) = dispose.dyn_ref::<Function>() {
                let _ = dispose.call0(&self.handle);
            }
        }
    }
}

pub struct BrowserPlatformPlugin;

impl Plugin for BrowserPlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlatformRequests>()
            .init_resource::<PlatformEvents>()
            .add_systems(Startup, initialize)
            .add_systems(
                PreUpdate,
                receive_events.before(virtual_life_app::input::GameInputSystems),
            )
            .add_systems(Last, send_requests);
        if app
            .world()
            .resource::<virtual_life_app::Canvas2dDisplay>()
            .0
        {
            app.add_systems(Last, present_canvas.before(send_requests));
        }
    }
}

fn initialize(world: &mut World) {
    match BrowserPlatform::new() {
        Ok(platform) => world.insert_non_send(platform),
        Err(error) => world
            .resource_mut::<PlatformEvents>()
            .0
            .push_back(PlatformEvent {
                id: 0,
                operation: "platformError".into(),
                value: Value::Null,
                error: Some(format!("浏览器平台初始化失败：{error:?}")),
                bytes: Vec::new(),
            }),
    }
}

fn present_canvas(
    pixels: Res<virtual_life_app::pixels::PixelCanvas>,
    mut last_revision: Local<Option<u64>>,
    mut display_failed: Local<bool>,
    mut host: ResMut<virtual_life_app::runtime::GameHost>,
    mut requests: ResMut<PlatformRequests>,
) {
    if *last_revision == Some(pixels.revision) || *display_failed {
        return;
    }
    match present_canvas_2d(&pixels.rgba) {
        Ok(()) => *last_revision = Some(pixels.revision),
        Err(error) => {
            *display_failed = true;
            let message = format!("画面显示失败：{error:?}");
            host.error = Some(message.clone());
            host.hidden = true;
            host.request(
                &mut requests,
                "fatal",
                serde_json::json!({"message":message}),
            );
        }
    }
}

fn receive_events(
    platform: Option<NonSend<BrowserPlatform>>,
    mut events: ResMut<PlatformEvents>,
    mut viewport: ResMut<virtual_life_app::input::CanvasViewport>,
) {
    if let Some(platform) = platform {
        for event in platform.incoming.borrow_mut().drain(..) {
            if event.operation == "viewportSize" {
                if let (Some(width), Some(height)) = (
                    event.value["width"].as_f64(),
                    event.value["height"].as_f64(),
                ) {
                    if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
                        viewport.0 = Some((width as f32, height as f32));
                    }
                }
            } else {
                events.0.push_back(event);
            }
        }
    }
}

fn send_requests(
    platform: Option<NonSend<BrowserPlatform>>,
    mut requests: ResMut<PlatformRequests>,
    mut events: ResMut<PlatformEvents>,
) {
    let Some(platform) = platform else {
        return;
    };
    for request in requests.0.drain(..) {
        if let Err(error) = platform.request(request.id, &request.operation, &request.payload) {
            events.0.push_back(PlatformEvent {
                id: request.id,
                operation: request.operation,
                value: Value::Null,
                error: Some(format!("浏览器操作执行失败：{error:?}")),
                bytes: Vec::new(),
            });
        }
    }
}
