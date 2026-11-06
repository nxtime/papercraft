import { defineConfig } from '@playwright/test';

// One relay, one tab at a time: the scenarios share the relay's single document-per-token model.
export default defineConfig({
  testDir: './tests',
  workers: 1,
  fullyParallel: false,
  timeout: 90_000,
  reporter: [['list']],
  // The web build is WebGPU-only. The default headless shell has no WebGPU adapter ("No available
  // adapters"), so use the new headless mode (channel 'chromium') with SwiftShader as a software
  // Vulkan device (adding --use-angle=swiftshader breaks adapter discovery). Headed chromium with --enable-unsafe-webgpu also works but steals focus.
  use: {
    browserName: 'chromium',
    channel: 'chromium',
    launchOptions: { args: ['--enable-unsafe-webgpu', '--enable-unsafe-swiftshader'] },
  },
});
