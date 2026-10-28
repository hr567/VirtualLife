mod binary;
pub mod bmp;
pub mod character;
pub mod engine;
pub mod flic;
pub mod pcx;
pub mod original_rules;
pub mod legacy_vm;
pub mod minigames;
pub mod hammer;
pub mod fist;
pub mod fire;
pub mod firebee;
pub mod gas;
pub mod competition;
pub mod competition_run;
pub mod epilogue;
pub mod ending_summary;
pub mod final_battle;
pub mod bonus;
pub mod battle;
pub mod relationships;
pub mod fortune;
pub mod equipment;
pub mod inventory;
pub mod inventory_view;
pub mod shops;
pub mod spirits;
pub mod status;
pub mod transition;
pub mod rng;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    indices: Option<Vec<u8>>,
}

#[wasm_bindgen]
impl DecodedImage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 { self.width }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 { self.height }
    pub fn pixels(&self) -> Vec<u8> { self.rgba.clone() }
    pub fn indices(&self) -> Option<Vec<u8>> { self.indices.clone() }
}

#[wasm_bindgen(js_name = decodeBmp)]
pub fn decode_bmp(data: &[u8]) -> Result<DecodedImage, JsValue> {
    let image = bmp::decode(data).map_err(|e| JsValue::from_str(&e))?;
    Ok(DecodedImage { width: image.width, height: image.height, rgba: image.rgba, indices: image.indices })
}

#[wasm_bindgen]
pub struct FlicDecoder { inner: flic::Decoder }

#[wasm_bindgen]
impl FlicDecoder {
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<FlicDecoder, JsValue> {
        flic::Decoder::new(data).map(|inner| Self { inner }).map_err(|e| JsValue::from_str(&e))
    }
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 { self.inner.width as u32 }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 { self.inner.height as u32 }
    #[wasm_bindgen(getter, js_name = frameCount)]
    pub fn frame_count(&self) -> u32 { self.inner.frame_count as u32 }
    #[wasm_bindgen(getter, js_name = frameDelayMs)]
    pub fn frame_delay_ms(&self) -> u32 { self.inner.frame_delay_ms }
    #[wasm_bindgen(js_name = nextFrame)]
    pub fn next_frame(&mut self) -> Result<Vec<u8>, JsValue> {
        self.inner.next_frame().map_err(|e| JsValue::from_str(&e))
    }
    pub fn reset(&mut self) { self.inner.reset(); }
    pub fn indices(&self)->Vec<u8>{self.inner.indices()}
}

#[wasm_bindgen]
pub struct Engine { inner: engine::Runtime }

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(content_json: &str) -> Result<Engine, JsValue> {
        engine::Runtime::new(content_json).map(|inner| Self { inner }).map_err(|e| JsValue::from_str(&e))
    }
    pub fn dispatch(&mut self, command_json: &str) -> String { self.inner.dispatch(command_json) }
    pub fn snapshot(&self) -> String { self.inner.snapshot_json() }
    pub fn serialize(&self) -> String { self.inner.serialize() }
    pub fn restore(&mut self, save_json: &str) -> String { self.inner.restore(save_json) }
}
