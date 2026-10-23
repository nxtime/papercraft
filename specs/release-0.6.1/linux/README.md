# Linux GUI check, 0.6.1

* **x86_64 under emulation** (`home-xvfb-lavapipe.png`): the published `papercraft-0.6.1-linux-x86_64.tar.gz`, run in a Debian bookworm x86_64 container under Rosetta with Xvfb (1440x900) and Mesa lavapipe. The app starts and draws the home screen; text glyphs come out garbled.
* **aarch64 native** (`home-xvfb-lavapipe-arm64-native.png`): the same 0.6.1 code built for aarch64 and run in a Debian bookworm arm64 container (no emulation) with the same Xvfb and lavapipe stack (llvmpipe, LLVM 15.0.6, Mesa 22.3.6, Vulkan). Every glyph renders correctly.
* Conclusion: the garbling is an artefact of running x86 lavapipe's JIT under Rosetta, not an app bug. Text goes through stock `egui-wgpu` (`Rgba8Unorm`, unpadded rows handled by wgpu), and canvas text is tessellated geometry, so nothing in the text path is platform-dependent.
* Not checked: a hardware GPU driver on a real Linux desktop, and native x86_64 lavapipe.
* With no usable GPU adapter at all the app panics with "no compatible GPU adapter" (`src/app.rs:997`); a readable error is a follow-up.
