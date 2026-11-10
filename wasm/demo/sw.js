// Keeps the demo's own files so the page works offline once loaded. It handles same-origin GET
// requests only and never sees a user's file (those are read in the page and never requested).
// Network first, cache as the fallback: online, a new release shows at once.
const CACHE = "wsq-demo-v1";
const ASSETS = [
  "./",
  "index.html",
  "style.css",
  "app.js",
  "sample.wsq",
  "sample.png",
  "wsq/wsq_codec.js",
  "wsq/wsq_codec_bg.wasm",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(ASSETS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key.startsWith("wsq-demo-") && key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET" || new URL(request.url).origin !== self.location.origin) return;
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      })
      .catch(() => caches.match(request, { ignoreSearch: true })),
  );
});
