/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { usePreference } from './usePreference';

afterEach(() => {
  cleanup();
  localStorage.clear();
});

function Writer() {
  const [value, setValue] = usePreference('attributionEnabled');
  return (
    <button data-testid="writer" onClick={() => setValue(!value)}>
      writer: {String(value)}
    </button>
  );
}

function Reader() {
  const [value] = usePreference('attributionEnabled');
  return <span data-testid="reader">reader: {String(value)}</span>;
}

describe('usePreference cross-instance reactivity', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('a setValue in one component is observed by a sibling instance', () => {
    render(
      <>
        <Writer />
        <Reader />
      </>,
    );

    // Both start at default (false for attributionEnabled).
    expect(screen.getByTestId('writer').textContent).toBe('writer: false');
    expect(screen.getByTestId('reader').textContent).toBe('reader: false');

    // Toggle in the writer — the reader observes the change without
    // remounting.
    fireEvent.click(screen.getByTestId('writer'));

    expect(screen.getByTestId('writer').textContent).toBe('writer: true');
    expect(screen.getByTestId('reader').textContent).toBe('reader: true');
  });
});
