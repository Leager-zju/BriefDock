# BriefDock Windows transparency patch

Source: iced_wgpu 0.14.0 from crates.io, MIT license (LICENSE).
Only src/window/compositor.rs differs from upstream:
- Windows uses DX12 with DxgiFromVisual (DirectComposition), since HWND swapchains ignore alpha.
- Windows prefers premultiplied alpha, as required by composition swapchains.
Other platforms keep upstream settings. Remove this patch when upstream supports configuring these options.
