export type VirtualWindow = {
  totalRows: number;
  firstRow: number;
  endRow: number;
  startIndex: number;
  endIndex: number;
  topSpacer: number;
  bottomSpacer: number;
};

export function getGridColumnCount(
  width: number,
  columnGap: number,
  minimumCardWidth: number,
  mode: "grid" | "list",
): number {
  if (mode === "list") return 1;
  const safeWidth = Math.max(0, width);
  const safeGap = Math.max(0, columnGap);
  const safeCardWidth = Math.max(1, minimumCardWidth);
  return Math.max(1, Math.floor((safeWidth + safeGap) / (safeCardWidth + safeGap)));
}

export function getVirtualWindow(
  itemCount: number,
  columns: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  rowGap: number,
  overscan = 2,
): VirtualWindow {
  const safeColumns = Math.max(1, Math.floor(columns));
  const safeRowHeight = Math.max(1, rowHeight);
  const safeCount = Math.max(0, Math.floor(itemCount));
  const totalRows = Math.ceil(safeCount / safeColumns);
  const firstRow = Math.min(totalRows, Math.max(0, Math.floor(Math.max(0, scrollTop) / safeRowHeight) - overscan));
  const visibleRows = Math.ceil(Math.max(0, viewportHeight) / safeRowHeight) + overscan * 2;
  const endRow = Math.min(totalRows, firstRow + visibleRows);
  return {
    totalRows,
    firstRow,
    endRow,
    startIndex: firstRow * safeColumns,
    endIndex: Math.min(safeCount, endRow * safeColumns),
    topSpacer: Math.max(0, firstRow * safeRowHeight - rowGap),
    bottomSpacer: Math.max(0, (totalRows - endRow) * safeRowHeight - rowGap),
  };
}
