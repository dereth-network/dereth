//! `RendererChoice` — which graphics backend the client was *asked* for.
//!
//! The client has a backend selector (`--renderer vulkan|d3d12|wgpu`, and the `Renderer=`
//! preference under it). `dereth_render::device::Backend` cannot spell the selection outside the
//! renderer: `Backend::compiled_in` is `cfg!(feature = "vulkan")` /
//! `cfg!(all(windows, feature = "d3d12"))`, read against `dereth-render`'s own cargo features, and
//! in any other crate both arms are `false`.
//!
//! So the *selection* and the *capability* are separated instead. This enum is the selection: a
//! name a player typed or a preferences file spelled, with no opinion about what this build can
//! create. `dereth_client::config::Config::renderer` holds one, which is what lets `config.rs` live
//! in `dereth-client-runtime`. `dereth_render::device::Backend` keeps the capability and implements
//! `From<RendererChoice>`, and the one site that creates a device
//! (`dereth_client::app::App::device_presentation`) resolves the choice against
//! `Backend::compiled_in`.
//!
//! [`parse`](RendererChoice::parse) and [`name`](RendererChoice::name) are `Backend`'s own: the
//! same spellings, the same case folding, the same `None`.

/// Which graphics backend the command line, the preferences file or the environment named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererChoice {
    /// `dereth_render::vulkan` — the default on every platform.
    Vulkan,
    /// `dereth_render::d3d12` — Windows only, and only when this build compiled the `d3d12`
    /// feature.
    D3d12,
    /// `dereth_render::wgpu` — the platform's own API through `wgpu`.
    Wgpu,
}

impl RendererChoice {
    /// The name the command line and the preferences file both spell.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::D3d12 => "d3d12",
            Self::Wgpu => "wgpu",
        }
    }

    /// Parse one of those spellings, case-insensitively. `None` for anything else.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vulkan" | "vk" => Some(Self::Vulkan),
            "d3d12" | "dx12" | "direct3d12" => Some(Self::D3d12),
            "wgpu" | "webgpu" => Some(Self::Wgpu),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spellings are `Backend::parse`'s, and this asserts them here so the two cannot drift
    /// apart silently: `dereth_render::device::Backend::parse` is written as a delegation to this
    /// function, and `dereth-render`'s own test asserts the round trip through `From`.
    #[test]
    fn the_selector_accepts_the_three_spellings_of_each_backend_and_nothing_else() {
        assert_eq!(
            RendererChoice::parse("vulkan"),
            Some(RendererChoice::Vulkan)
        );
        assert_eq!(RendererChoice::parse("vk"), Some(RendererChoice::Vulkan));
        assert_eq!(
            RendererChoice::parse(" VULKAN "),
            Some(RendererChoice::Vulkan)
        );
        assert_eq!(RendererChoice::parse("d3d12"), Some(RendererChoice::D3d12));
        assert_eq!(RendererChoice::parse("dx12"), Some(RendererChoice::D3d12));
        assert_eq!(
            RendererChoice::parse("Direct3D12"),
            Some(RendererChoice::D3d12)
        );
        assert_eq!(RendererChoice::parse("opengl"), None);
        assert_eq!(RendererChoice::parse(""), None);
        assert_eq!(RendererChoice::Vulkan.name(), "vulkan");
        assert_eq!(RendererChoice::D3d12.name(), "d3d12");
    }

    /// The third backend is spelled `wgpu` (or `webgpu`), in any case, and names itself `wgpu`.
    #[test]
    fn the_selector_accepts_the_wgpu_backend_by_its_two_spellings() {
        assert_eq!(RendererChoice::parse("wgpu"), Some(RendererChoice::Wgpu));
        assert_eq!(
            RendererChoice::parse(" WebGPU "),
            Some(RendererChoice::Wgpu)
        );
        assert_eq!(RendererChoice::Wgpu.name(), "wgpu");
    }
}
