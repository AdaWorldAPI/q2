import { describe, expect, it } from 'vitest'
import { render } from '@testing-library/react'
import { HtmlSourceView } from './HtmlSourceView'

describe('HtmlSourceView', () => {
  it('renders the given HTML source as text inside the code element', () => {
    const { container } = render(
      <HtmlSourceView html="<p>Hello, world!</p>" />,
    )
    const code = container.querySelector('code')!
    // Highlighted source still has the literal text content.
    expect(code.textContent).toContain('<p>Hello, world!</p>')
  })

  it('updates DOM when the html prop changes', () => {
    // Regression test for the stale-content bug: an earlier implementation
    // used hljs.highlightElement inside useEffect, which mutated the DOM and
    // caused React's text reconciliation to target a detached text node. A
    // prop change then silently left the previous stage's content visible.
    const { container, rerender } = render(
      <HtmlSourceView html="<p>BODY ONLY</p>" />,
    )
    const codeBefore = container.querySelector('code')!
    expect(codeBefore.textContent).toContain('BODY ONLY')
    expect(codeBefore.textContent).not.toContain('DOCTYPE')

    rerender(<HtmlSourceView html="<!DOCTYPE html><html><body><p>TEMPLATED</p></body></html>" />)

    const codeAfter = container.querySelector('code')!
    expect(codeAfter.textContent).toContain('TEMPLATED')
    expect(codeAfter.textContent).toContain('DOCTYPE')
    expect(codeAfter.textContent).not.toContain('BODY ONLY')
  })
})
