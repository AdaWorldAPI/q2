/**
 * Parse a "project reference" — the value a caller supplies wherever a
 * Quarto Hub project is named.
 *
 * Historically this was always a bare automerge index document ID. It may now
 * also be a full quarto-hub.com **share URL** — the link users hand each other
 * to grant project access, e.g.
 *
 *   https://quarto-hub.com/#/share/<id>?server=wss://…&file=_brand.yml&name=…
 *
 * The catch: everything after `#` is the URL *fragment*, so `<id>`, `file`,
 * `server`, and `name` all live there — and `new URL(u).searchParams` (which
 * only sees the real query string, which is empty here) returns none of them.
 * So we split the fragment by hand. Parsing this once, in TypeScript, is far
 * more reliable than asking a model to decode `%60`→backtick and `+`→space and
 * split the fragment correctly on every call.
 */
export interface ProjectRef {
  /** The automerge index document ID of the project. */
  project: string;
  /** The file named by `?file=` in a share URL, if any. */
  file?: string;
  /** The hub websocket endpoint named by `?server=` in a share URL, if any. */
  server?: string;
  /** The human-readable project name from `?name=` in a share URL, if any. */
  name?: string;
}

/**
 * Normalize a project reference. Accepts either a bare index doc id or a
 * quarto-hub.com share URL; always returns at least `{ project }`.
 */
export function parseProjectRef(input: string): ProjectRef {
  const raw = input.trim();

  // Not a share link — treat the whole string as a bare index doc id.
  if (!raw.includes('/share/')) {
    return { project: raw };
  }

  // The share link's data is in the fragment; isolate it, then split off its
  // (fragment-local) query string.
  const fragment = raw.includes('#') ? raw.slice(raw.indexOf('#') + 1) : raw;
  const queryStart = fragment.indexOf('?');
  const pathPart = queryStart === -1 ? fragment : fragment.slice(0, queryStart);
  const queryPart = queryStart === -1 ? '' : fragment.slice(queryStart + 1);

  const idMatch = pathPart.match(/\/share\/([^/?#]+)/);
  const project = idMatch ? decodeURIComponent(idMatch[1]) : raw;

  // URLSearchParams handles percent-decoding and `+`→space for us.
  const params = new URLSearchParams(queryPart);
  const ref: ProjectRef = { project };
  const file = params.get('file');
  const server = params.get('server');
  const name = params.get('name');
  if (file) ref.file = file;
  if (server) ref.server = server;
  if (name) ref.name = name;
  return ref;
}

/**
 * Whether two hub server URLs refer to the same endpoint. Tolerant of trailing
 * slashes, host case, and surrounding whitespace, but **scheme-sensitive**
 * (`ws://` ≠ `wss://`) and path-sensitive. Used to reject a share URL whose
 * `server=` names a different hub than the MCP is configured to talk to —
 * connecting to the wrong hub would read/write the wrong documents.
 *
 * Falls back to a trimmed exact-string compare when either value is not a
 * parseable URL, so a bad input never silently "matches".
 */
export function serversMatch(a: string, b: string): boolean {
  const normalize = (s: string): string | null => {
    try {
      const u = new URL(s.trim());
      const path = u.pathname.replace(/\/+$/, '');
      return `${u.protocol}//${u.host.toLowerCase()}${path}`;
    } catch {
      return null;
    }
  };
  const na = normalize(a);
  const nb = normalize(b);
  if (na === null || nb === null) {
    return a.trim() === b.trim();
  }
  return na === nb;
}
