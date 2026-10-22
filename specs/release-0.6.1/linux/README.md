# Linux GUI check, 0.6.1

The published `papercraft-0.6.1-linux-x86_64.tar.gz` was downloaded from the release and run in a Debian bookworm container (x86_64 under Rosetta) with Xvfb at 1440x900 and Mesa's software Vulkan driver (lavapipe).

* The app starts, opens its window (`PaperCraft`, 1440x900) and draws the home screen: layout, icons, colours and buttons are right (`home-xvfb-lavapipe.png`).
* Text glyphs render garbled. With `WGPU_BACKEND=gl` there is no compatible surface under Xvfb, so lavapipe was the only renderer available; the garbling is not reproduced on macOS. It needs a check on a real Linux desktop with a hardware GPU driver before it can be called an app bug.
* With no usable GPU adapter at all, the app panics with "no compatible GPU adapter" (`src/app.rs:997`); a readable error dialog is a follow-up.
