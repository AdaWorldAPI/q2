// /cpic — CPIC pharmacogenomics cockpit (gene-first), additive alongside /fma-body.
//
// A scenario {gene, diplotype|phenotype, drug} is posted to POST /api/cpic/reason, which runs
// the standalone `cpic` crate's reason() over the REAL published CPIC tables (allele,
// gene_result, drug, pair, guideline, recommendation). It resolves a phenotype and chains
// diplotype → phenotype → recommendation by 2-hop NARS deduction with CPIC-authoritative
// confidence (classification → f, pair cpiclevel → c). Each chain node carries its routable
// (part_of:is_a) GUID prefix — the same canonical 16-byte NodeGuid the rest of the stack uses.
//
// POC over published CPIC rules — NOT clinical decision support.
import { useEffect, useMemo, useState } from 'react';

interface ChainNode {
  role: string; // "diplotype" | "phenotype" | "recommendation"
  label: string;
  guid: string;
}

interface Outcome {
  gene: string;
  input: string;
  drug: string;
  resolved: boolean;
  phenotype: string | null;
  how: string | null;
  chain: ChainNode[];
  classification: string | null;
  cpic_level: string | null;
  truth_f: number;
  truth_c: number;
  truth_exp: number;
  recommendation: string | null;
  flags: string[];
  disclaimer: string;
}

interface Catalog {
  genes: string[];
  drugs: string[];
}

interface Scenario {
  gene: string;
  input: string;
  drug: string;
}

// the four `reason` CLI demos: clean 2-hop, direct 1-hop, multi-gene flag, complex-guideline flag.
const EXAMPLES: { label: string; sc: Scenario }[] = [
  { label: 'CYP2C19 *2/*2 · clopidogrel', sc: { gene: 'CYP2C19', input: '*2/*2', drug: 'clopidogrel' } },
  { label: 'TPMT *3A/*3A · azathioprine', sc: { gene: 'TPMT', input: '*3A/*3A', drug: 'azathioprine' } },
  { label: 'HLA-B *57:01 positive · abacavir', sc: { gene: 'HLA-B', input: '*57:01 positive', drug: 'abacavir' } },
  { label: 'CYP2C9 *1/*1 · warfarin (complex)', sc: { gene: 'CYP2C9', input: '*1/*1', drug: 'warfarin' } },
];

const ROLE_COLOR: Record<string, string> = {
  diplotype: '#6db3ff',
  phenotype: '#7fd9a8',
  recommendation: '#ffb86b',
};

function levelColor(level: string | null): string {
  switch (level) {
    case 'A': return '#35d07f';
    case 'B': return '#9ad07f';
    case 'C': return '#ffb547';
    case 'D': return '#ff8c63';
    default: return '#93a9bf';
  }
}

export function CpicCockpit() {
  const [catalog, setCatalog] = useState<Catalog>({ genes: [], drugs: [] });
  const [gene, setGene] = useState('CYP2C19');
  const [input, setInput] = useState('*2/*2');
  const [drug, setDrug] = useState('clopidogrel');
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // pull the gene + drug pick-lists once (used as <datalist> autocomplete; free text still allowed).
  useEffect(() => {
    let cancelled = false;
    fetch('/api/cpic/catalog')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((c: Catalog) => { if (!cancelled) setCatalog(c); })
      .catch(() => { /* catalog is optional — the inputs accept free text regardless */ });
    return () => { cancelled = true; };
  }, []);

  async function runReason(sc: Scenario) {
    setBusy(true);
    setError(null);
    setOutcome(null);
    try {
      const r = await fetch('/api/cpic/reason', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(sc),
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      setOutcome((await r.json()) as Outcome);
    } catch (e) {
      setError(`reasoning endpoint unavailable (${e}) — needs the cockpit-server backend deployed`);
    } finally {
      setBusy(false);
    }
  }

  const canReason = useMemo(
    () => gene.trim() !== '' && input.trim() !== '' && drug.trim() !== '' && !busy,
    [gene, input, drug, busy],
  );

  function pickExample(sc: Scenario) {
    setGene(sc.gene);
    setInput(sc.input);
    setDrug(sc.drug);
    void runReason(sc);
  }

  const fieldStyle: React.CSSProperties = {
    boxSizing: 'border-box', padding: '8px 10px', borderRadius: 6,
    border: '1px solid #2a3242', background: '#0e1219', color: '#cdd9e5',
    font: '13px ui-monospace, monospace',
  };
  const chip: React.CSSProperties = {
    padding: '5px 11px', borderRadius: 6, border: '1px solid #2a3242',
    background: '#0e1219', color: '#9fb1c4', font: '12px ui-monospace, monospace', cursor: 'pointer',
  };
  const badge = (bg: string, fg: string): React.CSSProperties => ({
    display: 'inline-block', padding: '2px 9px', borderRadius: 999,
    background: bg, color: fg, font: '11px ui-monospace, monospace', marginRight: 8,
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'auto', color: '#cdd9e5' }}>
      <div style={{ maxWidth: 760, margin: '0 auto', padding: '28px 20px 60px', font: '13px ui-monospace, monospace' }}>
        <div style={{ fontSize: 20, color: '#fff', marginBottom: 4 }}>
          CPIC pharmacogenomics
        </div>
        <div style={{ opacity: 0.65, marginBottom: 2 }}>
          gene → phenotype → recommendation, chained by NARS deduction over the real CPIC tables.
        </div>
        <div style={{ opacity: 0.5, fontSize: 11, marginBottom: 18 }}>
          confidence is CPIC-authoritative (classification → f, pair cpiclevel → c). Each node shows its
          routable (part_of:is_a) GUID. POC over published CPIC rules — <b>not</b> clinical decision support.
        </div>

        {/* input row */}
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'flex-end' }}>
          <label style={{ display: 'flex', flexDirection: 'column', gap: 4, flex: '1 1 130px' }}>
            <span style={{ opacity: 0.6, fontSize: 11 }}>gene</span>
            <input list="cpic-genes" value={gene} onChange={(e) => setGene(e.target.value)}
              placeholder="CYP2C19" style={fieldStyle} />
          </label>
          <label style={{ display: 'flex', flexDirection: 'column', gap: 4, flex: '1 1 150px' }}>
            <span style={{ opacity: 0.6, fontSize: 11 }}>diplotype / phenotype</span>
            <input value={input} onChange={(e) => setInput(e.target.value)}
              placeholder="*2/*2" style={fieldStyle} />
          </label>
          <label style={{ display: 'flex', flexDirection: 'column', gap: 4, flex: '1 1 150px' }}>
            <span style={{ opacity: 0.6, fontSize: 11 }}>drug</span>
            <input list="cpic-drugs" value={drug} onChange={(e) => setDrug(e.target.value)}
              placeholder="clopidogrel" style={fieldStyle} />
          </label>
          <button
            disabled={!canReason}
            onClick={() => runReason({ gene, input, drug })}
            style={{
              ...fieldStyle, cursor: canReason ? 'pointer' : 'not-allowed',
              border: '1px solid #3a5f88', background: canReason ? '#16202e' : '#0c1017',
              color: canReason ? '#cdd9e5' : '#566', padding: '9px 18px',
            }}
          >
            {busy ? 'reasoning…' : 'reason'}
          </button>
          <datalist id="cpic-genes">{catalog.genes.map((g) => <option key={g} value={g} />)}</datalist>
          <datalist id="cpic-drugs">{catalog.drugs.map((d) => <option key={d} value={d} />)}</datalist>
        </div>

        {/* example chips */}
        <div style={{ display: 'flex', gap: 7, flexWrap: 'wrap', marginTop: 12 }}>
          <span style={{ opacity: 0.45, fontSize: 11, alignSelf: 'center' }}>examples:</span>
          {EXAMPLES.map((ex) => (
            <button key={ex.label} style={chip} disabled={busy} onClick={() => pickExample(ex.sc)}>{ex.label}</button>
          ))}
        </div>

        {error && (
          <div style={{ marginTop: 18, padding: 12, borderRadius: 8, border: '1px solid #ff637d44', background: '#160e12', color: '#ff8095' }}>
            {error}
          </div>
        )}

        {outcome && (
          <div style={{ marginTop: 20, background: '#0e1219', border: '1px solid #1c2530', borderRadius: 10, padding: 16 }}>
            <div style={{ color: '#fff', fontSize: 15, marginBottom: 2 }}>
              {outcome.gene} {outcome.input} <span style={{ opacity: 0.5 }}>+</span> {outcome.drug}
            </div>
            {outcome.how && <div style={{ opacity: 0.55, fontSize: 11, marginBottom: 12 }}>resolved via {outcome.how}</div>}

            {/* the reasoned chain: diplotype → phenotype → recommendation, each with its GUID */}
            {outcome.chain.length > 0 && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 0, marginBottom: 14 }}>
                {outcome.chain.map((n, i) => (
                  <div key={`${n.role}-${i}`}>
                    <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, padding: '6px 0' }}>
                      <span style={{ ...badge('#11161f', ROLE_COLOR[n.role] ?? '#9fb1c4'), minWidth: 96, textAlign: 'center' }}>{n.role}</span>
                      <span style={{ color: '#dbe6f0', flex: '1 1 auto' }}>{n.label}</span>
                      <span style={{ opacity: 0.5, fontSize: 11 }}>{n.guid}</span>
                    </div>
                    {i < outcome.chain.length - 1 && (
                      <div style={{ marginLeft: 44, color: '#33414f', fontSize: 13, lineHeight: '8px' }}>↓</div>
                    )}
                  </div>
                ))}
              </div>
            )}

            {outcome.resolved ? (
              <>
                <div style={{ marginBottom: 10 }}>
                  {outcome.classification && (
                    <span style={badge('#16202e', '#cdd9e5')}>class: {outcome.classification}</span>
                  )}
                  {outcome.cpic_level && (
                    <span style={badge('#11161f', levelColor(outcome.cpic_level))}>CPIC level {outcome.cpic_level}</span>
                  )}
                  <span style={badge('#11161f', '#6db3ff')}>
                    f={outcome.truth_f.toFixed(3)} c={outcome.truth_c.toFixed(3)} · exp {outcome.truth_exp.toFixed(3)}
                  </span>
                </div>
                {outcome.recommendation && (
                  <div style={{ marginTop: 4, padding: '10px 12px', borderRadius: 8, background: '#101a14', border: '1px solid #1d3326', color: '#bfe9cf' }}>
                    <span style={{ opacity: 0.6, fontSize: 11 }}>CPIC recommendation</span>
                    <div style={{ marginTop: 4 }}>{outcome.recommendation}</div>
                  </div>
                )}
              </>
            ) : (
              <div style={{ padding: '10px 12px', borderRadius: 8, background: '#1a1410', border: '1px solid #3a2c1c', color: '#ffce96' }}>
                no simple phenotype → recommendation — surfaced, not fabricated.
              </div>
            )}

            {/* flags: complexity / multi-gene / unknown-drug warnings CPIC itself raises */}
            {outcome.flags.length > 0 && (
              <div style={{ marginTop: 12 }}>
                {outcome.flags.map((f, i) => (
                  <div key={i} style={{ color: '#ffb86b', fontSize: 12, marginBottom: 4 }}>⚠ {f}</div>
                ))}
              </div>
            )}

            <div style={{ opacity: 0.4, fontSize: 10, marginTop: 14, borderTop: '1px solid #1c2530', paddingTop: 8 }}>
              {outcome.disclaimer}
            </div>
          </div>
        )}

        <div style={{ display: 'flex', gap: 16, marginTop: 22, opacity: 0.7 }}>
          <a href="/fma-body" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/fma-body →</a>
          <a href="/fma" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/fma graph →</a>
          <a href="/" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/ cockpit →</a>
        </div>
      </div>
    </div>
  );
}
