// Leave ample room below WebKit's layout limit, including gallery padding/footer.
export const MAX_GRID_HEIGHT = 4_000_000;

export function scrollCoordinates(total: number, viewport: number, margin: number) {
  const height = Math.min(total, MAX_GRID_HEIGHT);
  const physicalRange = Math.max(1, height - viewport);
  const logicalRange = Math.max(1, total - viewport);
  const scale = logicalRange / physicalRange;
  return {
    height,
    compressed: total > height,
    toLogical(offset: number) {
      const inside = Math.max(0, Math.min(physicalRange, offset - margin));
      return offset + inside * (scale - 1);
    },
    toPhysical(offset: number) {
      const inside = Math.max(0, Math.min(logicalRange, offset - margin));
      return offset - inside * (1 - 1 / scale);
    }
  };
}
