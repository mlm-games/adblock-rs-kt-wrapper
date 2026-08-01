use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::Resource;
use adblock::Engine;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jbyteArray, jlong, jstring};
use jni::JNIEnv;
use serde_json::json;
use std::sync::Mutex;

struct EngineHandle {
    engine: Engine,
}

fn handle_from(ptr: jlong) -> Option<&'static Mutex<EngineHandle>> {
    if ptr == 0 {
        return None;
    }
    Some(unsafe { &*(ptr as *const Mutex<EngineHandle>) })
}

#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeCreate(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    let engine = Engine::default();
    let handle = Box::new(Mutex::new(EngineHandle { engine }));
    Box::into_raw(handle) as jlong
}

#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        unsafe {
            drop(Box::from_raw(ptr as *mut Mutex<EngineHandle>));
        }
    }
}

/// Loads full filter list text (can be multiple lists joined with `\n`).
#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeLoadFilterList<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    rules: JString<'local>,
) -> jboolean {
    let result = (|| -> anyhow::Result<()> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let mut guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let rules: String = env.get_string(&rules)?.into();
        let mut set = FilterSet::new(false);
        set.add_filter_list(rules, ParseOptions::default());
        guard.engine = Engine::new_with_filter_set(set);
        Ok(())
    })();

    match result {
        Ok(()) => jni::sys::JNI_TRUE,
        Err(e) => {
            let _ = env.throw_new("java/lang/RuntimeException", format!("{e:#}"));
            jni::sys::JNI_FALSE
        }
    }
}

/// Checks a single network request against the loaded filters.
#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeCheckNetworkUrls<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    url: JString<'local>,
    source_url: JString<'local>,
    request_type: JString<'local>,
) -> jboolean {
    let result = (|| -> anyhow::Result<bool> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let url: String = env.get_string(&url)?.into();
        let source_url: String = env.get_string(&source_url)?.into();
        let request_type: String = env.get_string(&request_type)?.into();

        let request = Request::new(&url, &source_url, &request_type, "")?;
        Ok(guard.engine.check_network_request(&request).should_block())
    })();

    match result {
        Ok(true) => jni::sys::JNI_TRUE,
        Ok(false) | Err(_) => jni::sys::JNI_FALSE,
    }
}

/// Loads the scriptlet/redirect resources required by `+js(...)` and `$redirect` rules.
/// `resources_json` is a JSON array of `Resource` objects (see brave/adblock-rust
/// `data/brave/brave-resources.json`).
#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeLoadResources<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    resources_json: JString<'local>,
) -> jboolean {
    let result = (|| -> anyhow::Result<()> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let mut guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let json: String = env.get_string(&resources_json)?.into();
        let resources: Vec<Resource> = serde_json::from_str(&json)?;
        guard.engine.use_resources(resources);
        Ok(())
    })();

    match result {
        Ok(()) => jni::sys::JNI_TRUE,
        Err(e) => {
            let _ = env.throw_new("java/lang/RuntimeException", format!("{e:#}"));
            jni::sys::JNI_FALSE
        }
    }
}

/// Returns cosmetic filtering resources for a page URL as JSON:
/// `{"selectors": [...], "js": "..."}` where `js` is the scriptlet code to inject
/// at document start and `selectors` are the CSS selectors to hide.
#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeGetCosmeticResources<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    url: JString<'local>,
) -> jstring {
    let result = (|| -> anyhow::Result<String> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let url: String = env.get_string(&url)?.into();
        let res = guard.engine.url_cosmetic_resources(&url);
        let selectors: Vec<String> = res.hide_selectors.into_iter().collect();
        Ok(json!({ "selectors": selectors, "js": res.injected_script }).to_string())
    })();

    match result {
        Ok(s) => env
            .new_string(&s)
            .map(|x| x.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            let _ = env.throw_new("java/lang/RuntimeException", format!("{e:#}"));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeSerialize(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let bytes = (|| -> anyhow::Result<Vec<u8>> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        Ok(guard.engine.serialize())
    })()
    .unwrap_or_default();

    env.byte_array_from_slice(&bytes)
        .map(|a| a.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_mlm_adblock_AdblockEngine_nativeDeserialize<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    data: JByteArray<'local>,
) -> jboolean {
    let result = (|| -> anyhow::Result<()> {
        let handle = handle_from(ptr).ok_or_else(|| anyhow::anyhow!("null engine"))?;
        let mut guard = handle.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let bytes = env.convert_byte_array(&data)?;
        let bytes: Vec<u8> = bytes.into_iter().map(|b| b as u8).collect();
        guard.engine.deserialize(&bytes)?;
        Ok(())
    })();

    if result.is_ok() {
        jni::sys::JNI_TRUE
    } else {
        jni::sys::JNI_FALSE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(rules: &str) -> Engine {
        let mut set = FilterSet::new(false);
        set.add_filter_list(rules.to_string(), ParseOptions::default());
        Engine::new_with_filter_set(set)
    }

    #[test]
    fn blocks_matching_ad() {
        let e = engine("||doubleclick.net^");
        let req =
            Request::new("https://ad.doubleclick.net/footer.js", "https://example.com/", "script", "")
                .unwrap();
        assert!(e.check_network_request(&req).should_block());
    }

    #[test]
    fn allows_benign_request() {
        let e = engine("||doubleclick.net^");
        let req =
            Request::new("https://example.com/logo.png", "https://example.com/", "image", "")
                .unwrap();
        assert!(!e.check_network_request(&req).should_block());
    }

    #[test]
    fn serialize_round_trip() {
        let e = engine("||ads.example.com^");
        let bytes = e.serialize();
        let mut restored = Engine::default();
        restored.deserialize(&bytes).unwrap();
        let req = Request::new(
            "https://ads.example.com/banner.gif",
            "https://example.com/",
            "image",
            "",
        )
        .unwrap();
        assert!(restored.check_network_request(&req).should_block());
    }

    #[test]
    fn cosmetic_resources_and_scriptlets() {
        let data = std::fs::read_to_string("../assets/adblock/resources.json").unwrap();
        let resources: Vec<Resource> = serde_json::from_str(&data).unwrap();

        let mut set = FilterSet::new(false);
        set.add_filter_list(
            "youtube.com##.ytp-ad-module\n\
             youtube.com##+js(trusted-replace-xhr-response, '\"adPlacements\"', '\"no_ads\"', /player/)\n\
             ||ads.example.com^"
                .to_string(),
            ParseOptions::default(),
        );
        let mut engine = Engine::new_with_filter_set(set);
        engine.use_resources(resources);

        let res = engine.url_cosmetic_resources("https://www.youtube.com/watch?v=xyz");
        assert!(
            res.hide_selectors.contains(".ytp-ad-module"),
            "cosmetic hide selector missing: {res:?}"
        );
        assert!(
            !res.injected_script.is_empty(),
            "scriptlet injection should be assembled"
        );
    }
}
