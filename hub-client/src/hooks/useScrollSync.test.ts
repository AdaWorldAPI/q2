/**
 * Tests for useScrollSync — focused on the deferred editor→preview scroll.
 * The editor cursor moves the instant a keystroke lands, but the preview DOM
 * only carries fresh `data-loc` once the async render commits. So (with
 * `deferToRender`, the q2-preview path) a cursor move during an edit defers its
 * scroll until the iframe reports `AST_RENDERED` (`handleAstRendered`) — firing
 * once, against the fresh DOM. Pure navigation flushes on the debounce. The
 * HTML preview leaves `deferToRender` off and scrolls immediately.
 * `scrollToLineDeferred` lets replay drive the same mechanism with an explicit
 * line.
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { RefObject } from 'react';
import type * as Monaco from 'monaco-editor';
import { useScrollSync } from './useScrollSync';

// Matches RENDER_SETTLE_TIMEOUT_MS in useScrollSync.
const RENDER_SETTLE_TIMEOUT_MS = 1000;

function makeEditor(line: number) {
  let cursorCb: () => void = () => {};
  let contentCb: () => void = () => {};
  const editor = {
    getPosition: () => ({ lineNumber: line, column: 1 }),
    getScrollHeight: () => 1000,
    getLayoutInfo: () => ({ height: 400 }),
    setScrollTop: vi.fn(),
    onDidChangeCursorPosition: (cb: () => void) => {
      cursorCb = cb;
      return { dispose: vi.fn() };
    },
    onDidChangeModelContent: (cb: () => void) => {
      contentCb = cb;
      return { dispose: vi.fn() };
    },
  } as unknown as Monaco.editor.IStandaloneCodeEditor;
  return {
    editor,
    fireCursorChange: () => cursorCb(),
    fireContentChange: () => contentCb(),
  };
}

function setup(opts: { line: number; focus: boolean; deferToRender?: boolean }) {
  const { editor, fireCursorChange, fireContentChange } = makeEditor(opts.line);
  const editorRef = { current: editor } as RefObject<Monaco.editor.IStandaloneCodeEditor | null>;
  const editorHasFocusRef = { current: opts.focus } as RefObject<boolean>;
  const scrollPreviewToLine = vi.fn();
  const getPreviewScrollRatio = vi.fn(() => 0.5);

  const { result } = renderHook(() =>
    useScrollSync({
      editorRef,
      scrollPreviewToLine,
      getPreviewScrollRatio,
      enabled: true,
      editorHasFocusRef,
      deferToRender: opts.deferToRender ?? true,
    }),
  );

  return { result, scrollPreviewToLine, fireCursorChange, fireContentChange, editorHasFocusRef };
}

describe('useScrollSync deferred editor→preview scroll', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('navigation: flushes on the debounce when no edit is in flight', () => {
    const { scrollPreviewToLine, fireCursorChange } = setup({ line: 12, focus: true });
    act(() => { fireCursorChange(); vi.advanceTimersByTime(50); });
    expect(scrollPreviewToLine).toHaveBeenCalledExactlyOnceWith(12);
  });

  it('edit: defers past the debounce, then scrolls once when the render reports back', () => {
    const { scrollPreviewToLine, fireCursorChange, fireContentChange, result } =
      setup({ line: 42, focus: true });
    // A keystroke: content changes and the cursor moves.
    act(() => { fireContentChange(); fireCursorChange(); vi.advanceTimersByTime(50); });
    // The debounce elapsed but a render is pending, so nothing scrolled yet.
    expect(scrollPreviewToLine).not.toHaveBeenCalled();
    // Render committed → scroll exactly once, against the fresh DOM.
    act(() => { result.current.handleAstRendered(); });
    expect(scrollPreviewToLine).toHaveBeenCalledExactlyOnceWith(42);
    // A subsequent render with no new cursor move must not scroll again.
    act(() => { result.current.handleAstRendered(); });
    expect(scrollPreviewToLine).toHaveBeenCalledTimes(1);
  });

  it('does nothing on AST_RENDERED when no cursor move is pending', () => {
    const { scrollPreviewToLine, result } = setup({ line: 5, focus: true });
    act(() => { result.current.handleAstRendered(); });
    expect(scrollPreviewToLine).not.toHaveBeenCalled();
  });

  it('does not scroll on a cursor move when the editor is not focused', () => {
    const { scrollPreviewToLine, fireCursorChange } = setup({ line: 9, focus: false });
    act(() => { fireCursorChange(); vi.advanceTimersByTime(50); });
    expect(scrollPreviewToLine).not.toHaveBeenCalled();
  });

  it('safety: flushes a deferred scroll if the render never reports back', () => {
    const { scrollPreviewToLine, fireCursorChange, fireContentChange } =
      setup({ line: 7, focus: true });
    act(() => { fireContentChange(); fireCursorChange(); vi.advanceTimersByTime(50); });
    expect(scrollPreviewToLine).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(RENDER_SETTLE_TIMEOUT_MS); });
    expect(scrollPreviewToLine).toHaveBeenCalledExactlyOnceWith(7);
  });

  it('scrollToLineDeferred (replay): waits for the render, scrolls to the explicit line, not focus-gated', () => {
    // Replay: editor is not focused, content changed, then replay asks to
    // scroll to the changed line. It must defer to the render and target the
    // explicit line (not the cursor), despite the editor lacking focus.
    const { scrollPreviewToLine, fireContentChange, result } =
      setup({ line: 1, focus: false });
    act(() => { fireContentChange(); result.current.scrollToLineDeferred(73); });
    expect(scrollPreviewToLine).not.toHaveBeenCalled();
    act(() => { result.current.handleAstRendered(); });
    expect(scrollPreviewToLine).toHaveBeenCalledExactlyOnceWith(73);
  });

  it('HTML path (deferToRender: false): scrolls immediately on cursor move, no render wait', () => {
    const { scrollPreviewToLine, fireCursorChange } =
      setup({ line: 20, focus: true, deferToRender: false });
    act(() => { fireCursorChange(); vi.advanceTimersByTime(50); });
    expect(scrollPreviewToLine).toHaveBeenCalledExactlyOnceWith(20);
  });
});
