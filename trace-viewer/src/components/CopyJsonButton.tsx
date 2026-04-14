import { useState } from 'react'

interface Props {
  /** Value to serialize to JSON and copy to the clipboard. */
  value: unknown
  /** Button label override. */
  label?: string
}

export function CopyJsonButton({ value, label = 'Copy JSON' }: Props) {
  const [state, setState] = useState<'idle' | 'copied' | 'error'>('idle')

  const onClick = async () => {
    try {
      const text = JSON.stringify(value, null, 2)
      await navigator.clipboard.writeText(text)
      setState('copied')
      setTimeout(() => setState('idle'), 1200)
    } catch {
      setState('error')
      setTimeout(() => setState('idle'), 1500)
    }
  }

  return (
    <button onClick={onClick} title="Copy entry as JSON to clipboard">
      {state === 'idle' ? label : state === 'copied' ? 'Copied!' : 'Failed'}
    </button>
  )
}
