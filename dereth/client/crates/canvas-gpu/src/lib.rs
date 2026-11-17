//! What both games' browser clients ask of the graphics device for a canvas: WebGPU or WebGL 2 on
//! the faster graphics card, and the adapter's line for the page.
//!
//! **Depends on** no other workspace crate (`wgpu`, and in the WebAssembly build `js-sys`,
//! `wasm-bindgen`, `wasm-bindgen-futures` and `web-sys`). **Used by** the device's browser half
//! (`dereth-render`) and Asheron's Call 2's browser client (`auberean-web`).
//!
//! **Must never** name either game: both browser clients draw with it. It never reaches the client
//! runtime (`cargo xtask seams`, `seam: client crates`).
//!
//! A canvas is drawn with WebGPU where the browser has it and WebGL 2 where it does not
//! ([`backends`]), on the [`POWER`] graphics card either way: a WebGPU adapter request carries the
//! preference, and the WebGL 2 context is asked for it ([`webgl_context`]) before `wgpu` asks for
//! the context, since `wgpu`'s own request leaves it to the browser. The adapter's line
//! ([`adapter_line`]) names the adapter as the browser does: on WebGPU by its vendor and
//! architecture ([`webgpu_adapter_name`]), which `wgpu` does not keep and which a browser gives
//! where it leaves the description empty.
//!
//! The browser half, `instance` and `line_for`, compiles only for `wasm32`.

/// The graphics card a device prefers: the more capable of two, on a machine that has two.
pub const POWER: wgpu::PowerPreference = wgpu::PowerPreference::HighPerformance;

/// The graphics API a canvas is drawn with: WebGPU when `webgpu` says the browser has it and it is
/// wanted, WebGL 2 otherwise.
#[must_use]
pub fn backends(webgpu: bool) -> wgpu::Backends {
    if webgpu {
        wgpu::Backends::BROWSER_WEBGPU
    } else {
        wgpu::Backends::GL
    }
}

/// A power preference as a browser spells it in a WebGL context's attributes, and in a WebGPU
/// adapter request, which leaves "default" unsaid.
#[must_use]
pub fn power_preference_name(power: wgpu::PowerPreference) -> &'static str {
    match power {
        wgpu::PowerPreference::None => "default",
        wgpu::PowerPreference::LowPower => "low-power",
        wgpu::PowerPreference::HighPerformance => "high-performance",
    }
}

/// The attributes a canvas's WebGL 2 context is asked for with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebGlContext {
    /// Whether the context's own frame is antialiased.
    pub antialias: bool,
    /// The graphics card it is asked to be made on, as a browser spells it.
    pub power_preference: &'static str,
}

/// What a canvas's WebGL 2 context is asked for: no antialiasing, as `wgpu` asks for it, and the
/// [`POWER`] graphics card, as a WebGPU request asks for it. `wgpu` asks for no graphics card of its
/// own, which leaves the browser's choice, often the less capable of two.
#[must_use]
pub fn webgl_context() -> WebGlContext {
    WebGlContext {
        antialias: false,
        power_preference: power_preference_name(POWER),
    }
}

/// A WebGPU adapter's name from what the browser says of it: its vendor and architecture, with its
/// description after them where the browser gives one (some leave it empty).
#[must_use]
pub fn webgpu_adapter_name(vendor: &str, architecture: &str, description: &str) -> String {
    let named = [vendor, architecture]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    match (named.is_empty(), description.is_empty()) {
        (false, false) => format!("{named} ({description})"),
        (false, true) => named,
        (true, false) => description.to_owned(),
        (true, true) => "an unnamed adapter".to_owned(),
    }
}

/// The adapter's line for a canvas: the adapter's name and the graphics API under it, as a page
/// names them, and whether it is the browser's software adapter.
#[must_use]
pub fn adapter_line(name: &str, webgpu: bool, software: bool) -> String {
    let api = if webgpu { "WebGPU" } else { "WebGL 2" };
    if software {
        format!("{name} on {api} (software)")
    } else {
        format!("{name} on {api}")
    }
}

/// The `wgpu` instance `canvas` is drawn with, and whether it is WebGPU's: WebGPU where the browser
/// has it and `webgl_only` does not refuse it, else WebGL 2, whose context is first asked for with
/// [`webgl_context`]'s attributes. A canvas has one context, made by the first request for it, and
/// `wgpu`'s own request takes that one; where the browser makes none, `wgpu`'s request says why.
#[cfg(target_arch = "wasm32")]
pub async fn instance(
    canvas: &web_sys::OffscreenCanvas,
    webgl_only: bool,
) -> (wgpu::Instance, bool) {
    let webgpu = !webgl_only && wgpu::util::is_browser_webgpu_supported().await;
    if !webgpu {
        let asked = webgl_context();
        let attributes = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&attributes, &"antialias".into(), &asked.antialias.into());
        let _ = js_sys::Reflect::set(
            &attributes,
            &"powerPreference".into(),
            &asked.power_preference.into(),
        );
        let _ = canvas.get_context_with_context_options("webgl2", &attributes);
    }
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends(webgpu);
    (wgpu::Instance::new(desc), webgpu)
}

/// The adapter's line for `adapter`, on WebGPU when `webgpu` says so ([`adapter_line`]): named on
/// WebGPU by what the browser says of the adapter a request like the canvas's gets
/// ([`webgpu_adapter_name`]), and otherwise, or where the browser says nothing, by `wgpu`'s name.
#[cfg(target_arch = "wasm32")]
pub async fn line_for(adapter: &wgpu::Adapter, webgpu: bool) -> String {
    let info = adapter.get_info();
    let software = info.device_type == wgpu::DeviceType::Cpu;
    let said = if webgpu {
        webgpu_adapter_info().await
    } else {
        None
    };
    let name = match said {
        Some([vendor, architecture, description]) => {
            webgpu_adapter_name(&vendor, &architecture, &description)
        }
        None => info.name,
    };
    adapter_line(&name, webgpu, software)
}

/// The vendor, architecture and description of the adapter the browser gives a WebGPU request for
/// the [`POWER`] graphics card, as a canvas's own request is made; `None` where it gives none.
#[cfg(target_arch = "wasm32")]
async fn webgpu_adapter_info() -> Option<[String; 3]> {
    use js_sys::Reflect;
    use wasm_bindgen::JsCast as _;
    let navigator = Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
    let gpu = Reflect::get(&navigator, &"gpu".into()).ok()?;
    let request: js_sys::Function = Reflect::get(&gpu, &"requestAdapter".into())
        .ok()?
        .dyn_into()
        .ok()?;
    let options = js_sys::Object::new();
    Reflect::set(
        &options,
        &"powerPreference".into(),
        &power_preference_name(POWER).into(),
    )
    .ok()?;
    let promise: js_sys::Promise = request.call1(&gpu, &options).ok()?.dyn_into().ok()?;
    let adapter = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?;
    let info = Reflect::get(&adapter, &"info".into())
        .ok()
        .filter(wasm_bindgen::JsValue::is_object)?;
    let text = |key: &str| {
        Reflect::get(&info, &key.into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };
    Some([text("vendor"), text("architecture"), text("description")])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canvas is drawn with WebGPU where the browser has it, and with WebGL 2 otherwise.
    #[test]
    fn a_canvas_is_drawn_with_webgpu_where_the_browser_has_it() {
        assert_eq!(backends(true), wgpu::Backends::BROWSER_WEBGPU);
        assert_eq!(backends(false), wgpu::Backends::GL);
    }

    /// A canvas drawn with WebGL 2 asks its context for the graphics card a WebGPU request asks
    /// for, the more capable of two, and for no antialiasing, as `wgpu` asks for it.
    #[test]
    fn a_webgl_canvas_asks_for_the_more_capable_graphics_card() {
        assert_eq!(POWER, wgpu::PowerPreference::HighPerformance);
        assert_eq!(
            webgl_context(),
            WebGlContext {
                antialias: false,
                power_preference: "high-performance",
            }
        );
        assert_eq!(
            power_preference_name(wgpu::PowerPreference::LowPower),
            "low-power"
        );
        assert_eq!(
            power_preference_name(wgpu::PowerPreference::None),
            "default"
        );
    }

    /// The page's line names a WebGPU adapter by its vendor and architecture, which a browser gives
    /// where it leaves the description empty, and says which API it draws with and whether the
    /// adapter is the browser's software one.
    #[test]
    fn the_pages_line_names_the_adapter_and_its_api() {
        assert_eq!(
            webgpu_adapter_name("nvidia", "blackwell", ""),
            "nvidia blackwell"
        );
        assert_eq!(
            webgpu_adapter_name("nvidia", "blackwell", "NVIDIA GeForce RTX 5080"),
            "nvidia blackwell (NVIDIA GeForce RTX 5080)"
        );
        assert_eq!(webgpu_adapter_name("apple", "", ""), "apple");
        assert_eq!(webgpu_adapter_name("", "", "A GPU"), "A GPU");
        assert_eq!(webgpu_adapter_name("", "", ""), "an unnamed adapter");
        assert_eq!(
            adapter_line("nvidia blackwell", true, false),
            "nvidia blackwell on WebGPU"
        );
        assert_eq!(
            adapter_line("ANGLE (NVIDIA)", false, false),
            "ANGLE (NVIDIA) on WebGL 2"
        );
        assert_eq!(
            adapter_line("google swiftshader", true, true),
            "google swiftshader on WebGPU (software)"
        );
    }
}
