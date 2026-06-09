import { useEffect, useRef, useState } from 'react';

interface Q2RawIframeProps {
  astJson: string;
}

/**
 * Simplest possible iframe wrapper: displays JSON.stringified AST
 * in a pre element. No interactivity, no custom components, no navigation.
 */
export function Q2RawIframe({ astJson }: Q2RawIframeProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [iframeReady, setIframeReady] = useState(false);

  // Handle messages from the iframe
  useEffect(() => {
    const handleMessage = (event: MessageEvent) => {
      if (event.data.type === 'IFRAME_READY') {
        setIframeReady(true);
      }
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, []);

  // Send AST updates when iframe is ready
  useEffect(() => {
    if (!iframeReady || !iframeRef.current?.contentWindow) return;

    iframeRef.current.contentWindow.postMessage(
      {
        type: 'UPDATE_AST',
        payload: { astJson },
      },
      '*'
    );
  }, [iframeReady, astJson]);

  return (
    <iframe
      ref={iframeRef}
      src="q2-raw.html"
      title="q2-raw Renderer"
      sandbox="allow-scripts"
      style={{
        width: '99%',
        height: '100%',
        border: 'none',
        display: 'block',
      }}
    />
  );
}
