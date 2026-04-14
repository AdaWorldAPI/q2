import { useMemo } from 'react'
import hljs from 'highlight.js/lib/core'
import xml from 'highlight.js/lib/languages/xml' // covers HTML

hljs.registerLanguage('html', xml)

interface Props {
  html: string
}

/**
 * Renders HTML source with syntax highlighting via highlight.js.
 *
 * We compute the highlighted markup in render and inject it via
 * `dangerouslySetInnerHTML`. An earlier version used `useEffect` +
 * `hljs.highlightElement`, which mutated the DOM under React — when the
 * `html` prop changed, React's reconciler tried to update a text node that
 * hljs had already detached, so the DOM kept displaying the previous
 * stage's content. Computing highlighted HTML as a pure value keeps React
 * in charge of the DOM and avoids that class of bug entirely.
 *
 * Deliberately does NOT render the HTML — that requires resolving CSS / JS
 * and supporting files, which is out of scope for the trace viewer.
 */
export function HtmlSourceView({ html }: Props) {
  const highlighted = useMemo(
    () => hljs.highlight(html, { language: 'html' }).value,
    [html],
  )

  return (
    <pre className="text-view">
      <code
        className="language-html hljs"
        dangerouslySetInnerHTML={{ __html: highlighted }}
      />
    </pre>
  )
}
