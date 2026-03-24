import { useState, useRef, useCallback } from 'react';
import { useStore } from '../store';
import { executeQuery } from '../transport';

const LANG_PATTERNS: Array<{ pattern: RegExp; lang: string }> = [
  { pattern: /^\s*g\./i, lang: 'gremlin' },
  { pattern: /^\s*MATCH\s*\(/i, lang: 'cypher' },
  { pattern: /^\s*(SELECT|CONSTRUCT|ASK|DESCRIBE)\s/i, lang: 'sparql' },
  { pattern: /^\s*(library|require|ggplot|data\.frame|<-)/i, lang: 'r' },
];

function detectLang(code: string): string {
  for (const { pattern, lang } of LANG_PATTERNS) {
    if (pattern.test(code)) return lang;
  }
  return 'gremlin';
}

export function QueryBar() {
  const [code, setCode] = useState('');
  const [lang, setLang] = useState('gremlin');
  const executing = useStore((s) => s.executing);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleInput = useCallback((value: string) => {
    setCode(value);
    setLang(detectLang(value));
  }, []);

  const handleRun = useCallback(async () => {
    if (!code.trim() || executing) return;
    await executeQuery(code, lang);
  }, [code, lang, executing]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.shiftKey && e.key === 'Enter') {
        e.preventDefault();
        handleRun();
      }
    },
    [handleRun],
  );

  return (
    <div className="query-bar" id="queryBar">
      <span className="query-lang-chip" id="langChip">
        <span id="langLabel">{lang}</span>
      </span>
      <input
        ref={inputRef}
        id="queryInput"
        type="text"
        placeholder="g.V().hasLabel('server').outE().inV().path()"
        value={code}
        onChange={(e) => handleInput(e.target.value)}
        onKeyDown={handleKeyDown}
        autoComplete="off"
        spellCheck={false}
      />
      <div className="query-actions">
        <button
          className="query-run"
          id="runBtn"
          onClick={handleRun}
          disabled={executing}
          title="Run (Shift+Enter)"
        >
          {executing ? (
            <svg width="12" height="12" viewBox="0 0 12 12">
              <rect x="2" y="2" width="8" height="8" rx="1" fill="currentColor" />
            </svg>
          ) : (
            <svg width="12" height="12" viewBox="0 0 12 12">
              <polygon points="2,0 12,6 2,12" fill="currentColor" />
            </svg>
          )}
        </button>
      </div>
    </div>
  );
}
