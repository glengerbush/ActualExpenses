/** iPhone Home Screen only: width is deliberately not a platform test. */
export function isIPhonePwa(): boolean {
  return (
    /iPhone/.test(navigator.userAgent) &&
    ((navigator as Navigator & { standalone?: boolean }).standalone === true ||
      matchMedia("(display-mode: standalone)").matches)
  );
}

export async function enableOfflineShell(): Promise<void> {
  if (!isSecureContext || !("serviceWorker" in navigator)) return;
  try {
    await navigator.serviceWorker.register("/expense-sw.js", {
      updateViaCache: "none",
    });
    const registration = await navigator.serviceWorker.ready;
    const warm = () =>
      registration.active?.postMessage({
        type: "CACHE_SHELL",
        urls: performance
          .getEntriesByType("resource")
          .map((entry) => entry.name),
      });
    warm();
    // Include language chunks first loaded after budget selection, without polling.
    window.addEventListener("pagehide", warm);
    window.addEventListener("load", warm, { once: true });
    navigator.serviceWorker.addEventListener("controllerchange", warm);
  } catch {
    // Storage/OS restrictions must not prevent online expense entry.
  }
}
