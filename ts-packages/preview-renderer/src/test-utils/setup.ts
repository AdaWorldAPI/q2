import '@testing-library/jest-dom';
import { vi } from 'vitest';

if (!globalThis.crypto?.randomUUID) {
  const cryptoPolyfill = {
    ...globalThis.crypto,
    randomUUID: () => 'test-uuid-' + Math.random().toString(36).substring(2, 11),
  } as Crypto;
  Object.defineProperty(globalThis, 'crypto', { value: cryptoPolyfill });
}

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = vi.fn().mockImplementation(() => ({
    observe: vi.fn(),
    unobserve: vi.fn(),
    disconnect: vi.fn(),
  }));
}

if (!globalThis.IntersectionObserver) {
  globalThis.IntersectionObserver = vi.fn().mockImplementation(() => ({
    observe: vi.fn(),
    unobserve: vi.fn(),
    disconnect: vi.fn(),
    root: null,
    rootMargin: '',
    thresholds: [],
    takeRecords: () => [],
  })) as unknown as typeof IntersectionObserver;
}

// jsdom does not implement getClientRects()/getBoundingClientRect() on Text
// nodes, so ProseMirror's coordsAtPos (used by tiptap focus → scrollIntoView)
// throws "target.getClientRects is not a function" when a tiptap editor is
// mounted in a test and its autofocus rAF fires. Geometry is meaningless in
// jsdom anyway (verified in a real browser), so stub zero-size rects to keep the
// editor's focus/scroll machinery from crashing the test run.
{
  const emptyRectList = () =>
    Object.assign([], { item: () => null }) as unknown as DOMRectList;
  const zeroRect = () =>
    ({ x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON: () => ({}) }) as DOMRect;
  // Text nodes and Ranges are the two targets ProseMirror's coordsAtPos passes to
  // singleRect(); jsdom implements getClientRects on neither (Element has it).
  for (const Ctor of [globalThis.Text, globalThis.Range]) {
    const proto = Ctor?.prototype as { getClientRects?: unknown; getBoundingClientRect?: unknown } | undefined;
    if (proto && typeof proto.getClientRects !== 'function') proto.getClientRects = emptyRectList;
    if (proto && typeof proto.getBoundingClientRect !== 'function') proto.getBoundingClientRect = zeroRect;
  }
}
