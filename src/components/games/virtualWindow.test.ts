import { describe, expect, it } from 'vitest';
import { getGridColumnCount, getVirtualWindow } from './virtualWindow';

describe('virtual game library window', () => {
  it('derives responsive columns from the measured card gap and minimum width', () => {
    expect(getGridColumnCount(0, 15, 160, 'grid')).toBe(1);
    expect(getGridColumnCount(320, 15, 160, 'grid')).toBe(1);
    expect(getGridColumnCount(335, 15, 160, 'grid')).toBe(2);
    expect(getGridColumnCount(700, 15, 160, 'grid')).toBe(4);
    expect(getGridColumnCount(700, 15, 160, 'list')).toBe(1);
  });
  it.each([10, 100, 500, 1000])('keeps every row addressable with %i games', count => {
    for (const columns of [1, 3, 7]) {
      const rowHeight = 286;
      const maxScroll = Math.max(0, Math.ceil(count / columns) * rowHeight - 720);
      const windows = [];
      for (let scroll = 0; scroll <= maxScroll; scroll += rowHeight) {
        windows.push(getVirtualWindow(count, columns, scroll, 720, rowHeight, 17));
      }
      windows.push(getVirtualWindow(count, columns, maxScroll, 720, rowHeight, 17));
      expect(windows[0].startIndex).toBe(0);
      expect(windows.at(-1)?.endIndex).toBe(count);
      expect(windows.every(window => window.endIndex > window.startIndex || count === 0)).toBe(true);
      for (const window of windows) {
        expect(window.startIndex).toBeGreaterThanOrEqual(0);
        expect(window.endIndex).toBeLessThanOrEqual(count);
        expect(window.endIndex - window.startIndex).toBeLessThanOrEqual(80);
      }
      for (let index = 1; index < windows.length; index++) {
        expect(windows[index].startIndex).toBeLessThanOrEqual(windows[index - 1].endIndex);
      }
    }
  });

  it('uses one column for list mode and bounds empty libraries', () => {
    expect(getVirtualWindow(42, 1, 500, 300, 90, 8).totalRows).toBe(42);
    expect(getVirtualWindow(0, 4, 0, 300, 200, 16)).toMatchObject({ totalRows: 0, startIndex: 0, endIndex: 0 });
  });
});
