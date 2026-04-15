interface Props {
  text: string
}

/** Plain monospaced text view for markdown-like payloads. No highlighting. */
export function TextView({ text }: Props) {
  return <pre className="text-view">{text}</pre>
}
