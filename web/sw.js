// Keeps the app starting without a connection. Only same-origin static app files are cached;
// the sign-in proxy (/oauth2/), the relay (/p/, /s/, /library, ...) and anything that is not a
// plain GET always go to the network. Projects are not cached here: they live in the page's own
// store and sync through the relay when it is reachable again.
const CACHE = "papercraft-app";
const BUILD = "pkg/build.txt";
const SHELL = ["./", "manifest.webmanifest", "icons/papercraft.svg", "icons/papercraft-512.png", "icons/papercraft-1024.png"];
const LIVE = ["/oauth2/", "/p/", "/s/", "/library"];

const buildOf = (r) => r.text().then((t) => encodeURIComponent(t.trim()));

// Never keep a sign-in redirect or an error page as the app.
const cacheable = (r) => r.ok && !r.redirected && r.type === "basic";

async function put(cache, request, response) {
  if (cacheable(response)) await cache.put(request, response.clone());
  return response;
}

// The glue and wasm are named by the build stamp, so a stamp never changes its bytes.
async function precache() {
  const cache = await caches.open(CACHE);
  const res = await fetch(BUILD, { cache: "no-store" });
  if (!cacheable(res)) throw new Error("build stamp unavailable");
  const v = await buildOf(res.clone());
  await cache.put(BUILD, res);
  await cache.addAll([...SHELL, `pkg/papercraft.js?v=${v}`, `pkg/papercraft_bg.wasm?v=${v}`]);
}

// A new stamp makes the older glue and wasm unreachable; drop them.
async function dropOldBuilds(cache, v) {
  for (const req of await cache.keys()) {
    const url = new URL(req.url);
    if (url.pathname.includes("/pkg/") && url.searchParams.has("v") && url.searchParams.get("v") !== v) {
      await cache.delete(req);
    }
  }
}

async function networkFirst(request) {
  const cache = await caches.open(CACHE);
  try {
    const res = await fetch(request);
    if (request.url.endsWith(BUILD) && cacheable(res)) await dropOldBuilds(cache, await buildOf(res.clone()));
    return await put(cache, request, res);
  } catch (e) {
    const hit = await cache.match(request, { ignoreSearch: request.mode === "navigate" });
    if (hit) return hit;
    throw e;
  }
}

async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  return (await cache.match(request)) || put(cache, request, await fetch(request));
}

self.addEventListener("install", (e) => e.waitUntil(precache().then(() => self.skipWaiting())));
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  const { request } = e;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== location.origin) return;
  if (LIVE.some((p) => url.pathname.startsWith(p))) return;
  const versioned = url.pathname.includes("/pkg/") && url.searchParams.has("v");
  e.respondWith(versioned ? cacheFirst(request) : networkFirst(request));
});
