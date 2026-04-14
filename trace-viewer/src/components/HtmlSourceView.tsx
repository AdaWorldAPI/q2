import { useEffect, useRef } from 'react'
import hljs from 'highlight.js/lib/core'
import xml from 'highlight.js/lib/languages/xml' // covers HTML

hljs.registerLanguage('html', xml)

interface Props {
  html: string
}

/**
 * Renders HTML source with syntax highlighting via highlight.js.
 * Deliberately does NOT render the HTML — that requires resolving CSS/JS
 * and supporting files, which is out of scope for the trace viewer.
 */
export function HtmlSourceView({ html }: Props) {
  const ref = useRef<HTMLPreElement>(null)

  useEffect(() => {
    if (ref.current) {
      // Force re-highlight whenever the payload changes.
      ref.current.removeAttribute('data-highlighted')
      hljs.highlightElement(ref.current)
    }
  }, [html])

  return (
    <pre
      ref={ref}
      className="text-view"
      // highlight.js expects the .language-<name> class on the <pre> or a child <code>.
      // Use an inner <code> so hljs treats it consistently.
    >
      <code className="language-html">{html}</code>
    </pre>
  )
}
