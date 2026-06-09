import { useEffect, useState } from 'react';
import katex from 'katex';
import 'katex/dist/katex.min.css';

interface UpdateAstPayload {
  astJson: string;
  currentFilePath: string;
}

export function App() {
  const [astJson, setAstJson] = useState<string>('');

  useEffect(() => {
    // Listen for messages from parent
    const handleMessage = (event: MessageEvent) => {
      if (event.data.type === 'UPDATE_AST') {
        const payload = event.data.payload as UpdateAstPayload;
        setAstJson(payload.astJson);
      }
    };

    window.addEventListener('message', handleMessage);

    // Signal that the iframe is ready to receive messages
    window.parent.postMessage({ type: 'IFRAME_READY' }, '*');

    return () => window.removeEventListener('message', handleMessage);
  }, []);

  if (!astJson) {
    return <div style={{ padding: '20px' }}>Loading q2-raw renderer...</div>;
  }

  try {
    const ast = JSON.parse(astJson);
    const prettyJson = JSON.stringify(ast, null, 2);

    return (
      <pre
        style={{
          margin: 0,
          padding: 16,
          fontFamily: "'Courier New', monospace",
          fontSize: 12,
          whiteSpace: 'pre-wrap',
          wordWrap: 'break-word',
        }}
      >
        {prettyJson}
      </pre>
    );
  } catch (err) {
    return (
      <div style={{ padding: 20, color: 'red' }}>
        <strong>Parse Error:</strong>
        <pre>{err instanceof Error ? err.message : String(err)}</pre>
      </div>
    );
  }
}

// Example showing katex is available
console.log('KaTeX version:', katex.version);
