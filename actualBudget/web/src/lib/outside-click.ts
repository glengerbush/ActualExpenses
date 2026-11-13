/** Attach only while an editor is mounted. The caller owns draft/receipt safety. */
export function outsideClick(dismiss: () => void) {
  return (element: HTMLElement) => {
    const document = element.ownerDocument;
    const handle = (event: MouseEvent) => {
      if (event.button === 0 && !event.composedPath().includes(element))
        dismiss();
    };
    // Disabled toolbar controls suppress clicks, but still receive pointer events.
    const disabledPointer = (event: PointerEvent) => {
      if (
        event
          .composedPath()
          .some((node) => node instanceof Element && node.matches(":disabled"))
      )
        handle(event);
    };
    document.addEventListener("pointerdown", disabledPointer, true);
    // Svelte can mount the editor before its opening click reaches document.
    const timer = setTimeout(
      () => document.addEventListener("click", handle),
      0,
    );
    return () => {
      clearTimeout(timer);
      document.removeEventListener("pointerdown", disabledPointer, true);
      document.removeEventListener("click", handle);
    };
  };
}
