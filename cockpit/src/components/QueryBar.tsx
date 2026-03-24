import { useState, useRef, useCallback } from 'react';
import { useStore } from '../store';
import { executeQuery } from '../transport';

const LANG_PATTERNS: Array<{ pattern: RegExp; lang: string }> = [
  { pattern: /^\s*g\./i, lang: 'gremlin' },
  { pattern: /^\s*MATCH\s*\(/i, lang: 'cypher' },
  { pattern: /^\s*(SELECT|CONSTRUCT|ASK|DESCRIBE)\s/i, lang: 'sparql' },
  { pattern: /^\s*(library|require|ggplot|data\.frame|<-|%>%)/i, lang: 'r' },
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
    <section className="querybar">
      <div className="prompt">&gt;_</div>
      <div className="query-input-wrap">
        <label>active cell / auto-detect language</label>
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
      </div>
      <button className="chip" onClick={handleRun} disabled={executing}>
        {lang}
      </button>
      <button className="button" onClick={handleRun} disabled={executing}>
        {executing ? 'running\u2026' : 'reactive execute'}
      </button>
    </section>
  );
}
