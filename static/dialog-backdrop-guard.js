// Native dialog backdrops are non-interactive. Clicks there are absorbed so
// that accidental or gesture-synthesized clicks never dismiss the dialog.
(() => {
  "use strict";

  document.addEventListener(
    "click",
    (event) => {
      if (event.target instanceof Element && event.target.closest("dialog") === event.target) {
        event.preventDefault();
        event.stopPropagation();
      }
    },
    true,
  );

  // Fixed overlay elements must not expose a dismissal affordance on backdrop.
  function bindBackdropDismissal(element) {
    if (!(element instanceof Element) || element.dataset.backdropGuard === "true") {
      return;
    }
    element.dataset.backdropGuard = "true";
    element.addEventListener("click", (event) => {
      if (event.target === element) {
        event.preventDefault();
        event.stopPropagation();
      }
    }, true);
  }

  globalThis.bindBackdropDismissal = bindBackdropDismissal;
})();
