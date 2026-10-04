export function animateModal(overlay: HTMLElement, entering: boolean, onFinish: () => void) {
  const style = getComputedStyle(overlay);
  const token =
    style.getPropertyValue(entering ? "--modal-entrance" : "--modal-exit").trim() ||
    style.getPropertyValue(entering ? "--motion-entrance" : "--motion-exit").trim();
  const milliseconds = Number.parseFloat(token) * (token.endsWith("ms") ? 1 : 1000);
  const duration = Number.isFinite(milliseconds) ? milliseconds : entering ? 150 : 100;
  const easing = style.getPropertyValue("--motion-ease").trim() || "cubic-bezier(0.2, 0, 0, 1)";
  // A fullscreen shell lives in the browser's top layer, outside ancestor opacity.
  const fullscreen = document.fullscreenElement;
  const targets =
    fullscreen instanceof HTMLElement && overlay.contains(fullscreen)
      ? [overlay, fullscreen]
      : [overlay];
  let cancelled = false;
  const finish = () => {
    if (!cancelled) onFinish();
  };
  const animations = targets.map((target) => {
    const from = target.style.opacity || (entering ? "0" : getComputedStyle(target).opacity || "1");
    target.style.opacity = entering ? "1" : "0";
    const animation = target.animate?.([{ opacity: from }, { opacity: target.style.opacity }], {
      duration,
      easing
    });
    if (animation) animation.onfinish = finish;
    return { target, animation };
  });
  // Covers unavailable animation events, including a suspended WebView.
  const timer = window.setTimeout(finish, duration);
  return () => {
    cancelled = true;
    window.clearTimeout(timer);
    for (const { target, animation } of animations) {
      const opacity = getComputedStyle(target).opacity;
      if (animation) {
        animation.onfinish = null;
        animation.cancel();
      }
      target.style.opacity = opacity;
    }
  };
}
