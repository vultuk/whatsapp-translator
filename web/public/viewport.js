export function calculateViewportLayout({
  innerHeight,
  viewportHeight,
  viewportOffsetTop = 0,
  keyboardOpenThreshold = 120,
}) {
  const layoutHeight = Math.max(0, Math.round(Number(innerHeight) || 0));
  const visualHeight = viewportHeight == null
    ? layoutHeight
    : Math.max(0, Math.round(Number(viewportHeight) + Number(viewportOffsetTop || 0)));
  const keyboardOffset = Math.max(0, layoutHeight - visualHeight);
  const keyboardOpen = keyboardOffset > keyboardOpenThreshold;

  return {
    keyboardOffset,
    keyboardOpen,
    effectiveHeight: keyboardOpen ? visualHeight : null,
  };
}
