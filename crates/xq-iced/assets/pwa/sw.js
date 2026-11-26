/* Service worker for the Xiangqi PWA (spike, 2026-11-26).
 *
 * Strategy: stale-while-revalidate for every GET request — serve the
 * cached copy at once (so the game opens offline), then refresh the
 * cache from the network in the background. Trunk hashes the WASM/JS
 * file names, so a new build never collides with a stale cached
 * bundle; `index.html` (which names the hashed files) is revalidated
 * the same way. Only the known static files are precached at
 * install; the WASM/JS bundles join the cache on first load.
 *
 * Scope: this file is copied to `dist/sw.js`, so its scope is the
 * whole app (`./`). No push, no background sync — install-time
 * precache + runtime cache only.
 */

const CACHE = "xiangqi-pwa-v1";

const PRECACHE = [
  "./",
  "index.html",
  "manifest.webmanifest",
  "icons/icon-192.png",
  "icons/icon-512.png",
  "icons/icon-512-maskable.png",
  "icons/apple-touch-icon.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") {
    return;
  }
  event.respondWith(
    caches.match(event.request, { ignoreSearch: true }).then((cached) => {
      const network = fetch(event.request)
        .then((response) => {
          if (response && response.ok) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(event.request, copy));
          }
          return response;
        })
        .catch(() => cached);
      // Cached copy first (offline works); network refreshes it.
      return cached || network;
    }),
  );
});
