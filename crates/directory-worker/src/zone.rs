//! `ZoneClock` backed by the runtime's `Intl.DateTimeFormat` (IANA zones).

use directory_core::reminders::ZoneClock;
use std::cell::RefCell;
use std::collections::HashMap;
use worker::js_sys::{self, Array, Function, Intl, Object, Reflect};
use worker::wasm_bindgen::{JsCast, JsValue};

/// At most this many cached formatters (zones in use are few).
const MAX_CACHED: usize = 64;

thread_local! {
    static FORMATTERS: RefCell<HashMap<String, Option<Intl::DateTimeFormat>>> =
        RefCell::new(HashMap::new());
}

pub struct IntlZones;

/// A formatter printing the wall clock in `zone`, or `None` if the runtime
/// rejects the zone (the constructor throws a RangeError).
fn formatter(zone: &str) -> Option<Intl::DateTimeFormat> {
    let make = || -> Result<Intl::DateTimeFormat, JsValue> {
        let opts = Object::new();
        for (k, v) in [
            ("timeZone", zone),
            ("hourCycle", "h23"),
            ("year", "numeric"),
            ("month", "numeric"),
            ("day", "numeric"),
            ("hour", "numeric"),
            ("minute", "numeric"),
            ("second", "numeric"),
        ] {
            Reflect::set(&opts, &k.into(), &v.into())?;
        }
        let intl = Reflect::get(&js_sys::global(), &"Intl".into())?;
        let ctor: Function = Reflect::get(&intl, &"DateTimeFormat".into())?.dyn_into()?;
        let args = Array::of2(&"en-US".into(), &opts);
        Ok(Reflect::construct(&ctor, &args)?.unchecked_into())
    };
    FORMATTERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(f) = cache.get(zone) {
            return f.clone();
        }
        let f = make().ok();
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(zone.to_owned(), f.clone());
        f
    })
}

impl ZoneClock for IntlZones {
    fn offset_minutes(&self, zone: &str, utc_ms: i64) -> Option<i32> {
        if zone.is_empty() || zone.len() > 64 {
            return None;
        }
        let f = formatter(zone)?;
        let parts = f.format_to_parts(&js_sys::Date::new(&JsValue::from_f64(utc_ms as f64)));
        let mut field = [0i64; 6]; // year, month, day, hour, minute, second
        for part in parts.iter() {
            let kind = Reflect::get(&part, &"type".into()).ok()?.as_string()?;
            let slot = match kind.as_str() {
                "year" => 0,
                "month" => 1,
                "day" => 2,
                "hour" => 3,
                "minute" => 4,
                "second" => 5,
                _ => continue,
            };
            let value = Reflect::get(&part, &"value".into()).ok()?.as_string()?;
            field[slot] = value.parse().ok()?;
        }
        // Some runtimes print midnight as 24 even with h23.
        let hour = field[3] % 24;
        let wall = js_sys::Date::utc(field[0] as f64, (field[1] - 1) as f64)
            + ((field[2] - 1) * 86_400_000 + hour * 3_600_000 + field[4] * 60_000 + field[5] * 1000)
                as f64;
        let utc_second = utc_ms.div_euclid(1000) * 1000;
        Some(((wall as i64 - utc_second) / 60_000) as i32)
    }
}
