/* Cache the loaded app shell only. Financial reads/writes always use the API. */
const CACHE = "actualExpenses-shell-v1";
const staticAsset = (url) =>
  url.origin === self.location.origin &&
  (url.pathname.startsWith("/assets/") ||
    [
      "/manifest.webmanifest",
      "/expense-icon.png",
      "/expense-icon-512.png",
    ].includes(url.pathname));
self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) =>
        cache.addAll(["/", "/manifest.webmanifest", "/expense-icon.png"]),
      ),
  );
});
self.addEventListener("activate", (event) =>
  event.waitUntil(self.clients.claim()),
);
self.addEventListener("message", (event) => {
  if (event.data?.type !== "CACHE_SHELL" || !Array.isArray(event.data.urls))
    return;
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      await Promise.allSettled(
        event.data.urls
          .slice(0, 100)
          .filter((value) => {
            try {
              return staticAsset(new URL(value));
            } catch {
              return false;
            }
          })
          .map((url) => cache.add(url)),
      );
      event.ports[0]?.postMessage("cached");
    })(),
  );
});
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") return;
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin || url.pathname.startsWith("/api/"))
    return;
  if (event.request.mode === "navigate") {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE);
        try {
          const response = await fetch(event.request);
          if (
            response.ok &&
            response.headers.get("content-type")?.includes("text/html")
          )
            await cache.put("/", response.clone());
          if (response.ok) return response;
          return (await cache.match("/")) || response;
        } catch (error) {
          const saved = await cache.match("/");
          if (saved) return saved;
          throw error;
        }
      })(),
    );
  } else if (staticAsset(url)) {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE),
          saved = await cache.match(event.request);
        if (saved) return saved;
        const response = await fetch(event.request);
        if (response.ok) await cache.put(event.request, response.clone());
        return response;
      })(),
    );
  }
});
