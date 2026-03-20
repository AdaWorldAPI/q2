/**
 * Extract format string from the parsed AST metadata.
 * Returns null if no format is found or format is not handled by ReactPreview,
 * otherwise returns the format string (e.g., 'q2-slides', 'q2-debug', 'revealjs').
 */
export function getQ2Format(astJson: string): string | null {
  try {
    const ast = JSON.parse(astJson);
    const fmt = ast?.meta?.format;
    if (!fmt) return null;
    let formatStr: string | null = null;
    // MetaString: { t: "MetaString", c: "q2-slides" }
    if (fmt.t === 'MetaString') formatStr = fmt.c;
    // MetaInlines: { t: "MetaInlines", c: [{ t: "Str", c: "q2-slides" }] }
    if (fmt.t === 'MetaInlines') formatStr = fmt.c?.[0]?.c;
    // Only return formats handled by ReactPreview
    if (formatStr?.startsWith('q2-') || formatStr === 'revealjs') return formatStr;
    return null;
  } catch (err) {
    console.error('[PreviewRouter] Failed to parse AST:', err);
    return null;
  }
}
