//! `Entropy` backed by the runtime's Web Crypto `getRandomValues`.

use directory_core::identity::Entropy;
use worker::js_sys::{self, Function, Reflect, Uint8Array};
use worker::wasm_bindgen::{JsCast, JsValue};

pub struct WebCryptoEntropy;

impl Entropy for WebCryptoEntropy {
    fn fill(&self, buf: &mut [u8]) {
        // getRandomValues is limited to 65536 bytes per call.
        for chunk in buf.chunks_mut(65536) {
            let crypto = Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
                .expect("globalThis.crypto");
            let get: Function = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
                .expect("crypto.getRandomValues")
                .unchecked_into();
            let array = Uint8Array::new_with_length(chunk.len() as u32);
            // A failure here means no secure randomness: never continue without it.
            get.call1(&crypto, &array).expect("getRandomValues failed");
            array.copy_to(chunk);
        }
    }
}
