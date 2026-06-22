import { describe, it, expect } from 'vitest';
import { parseProjectRef } from './share-url.js';

// The canonical share link quarto-hub.com hands users. Note that file/server/
// name live inside the URL *fragment* (after `#`), which `new URL().searchParams`
// does not see — the whole point of parsing it by hand.
const SHARE_URL =
  'https://quarto-hub.com/#/share/3fA4nXRpYK1JPkeyef3KXFMEs4aN' +
  '?server=wss%3A%2F%2Fquarto-hub.com%2Fws&file=_brand.yml&name=A+%60quarto-hub%60+update';

describe('parseProjectRef', () => {
  it('returns a bare index doc id unchanged', () => {
    expect(parseProjectRef('3fA4nXRpYK1JPkeyef3KXFMEs4aN')).toEqual({
      project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN',
    });
  });

  it('trims surrounding whitespace from a bare id', () => {
    expect(parseProjectRef('  3fA4nXRpYK1JPkeyef3KXFMEs4aN \n')).toEqual({
      project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN',
    });
  });

  it('extracts project, file, server, and name from a full share URL', () => {
    expect(parseProjectRef(SHARE_URL)).toEqual({
      project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN',
      file: '_brand.yml',
      server: 'wss://quarto-hub.com/ws',
      name: 'A `quarto-hub` update',
    });
  });

  it('decodes percent-encoding and + in the name', () => {
    // %60 -> backtick, + -> space
    expect(parseProjectRef(SHARE_URL).name).toBe('A `quarto-hub` update');
  });

  it('handles a share URL with no query params', () => {
    expect(
      parseProjectRef('https://quarto-hub.com/#/share/3fA4nXRpYK1JPkeyef3KXFMEs4aN'),
    ).toEqual({ project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN' });
  });

  it('handles a bare fragment without the scheme/host', () => {
    expect(
      parseProjectRef('/share/3fA4nXRpYK1JPkeyef3KXFMEs4aN?file=slides.qmd'),
    ).toEqual({ project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN', file: 'slides.qmd' });
  });

  it('omits file/server/name keys when those params are absent', () => {
    const ref = parseProjectRef(
      'https://quarto-hub.com/#/share/3fA4nXRpYK1JPkeyef3KXFMEs4aN?name=Untitled',
    );
    expect(ref).toEqual({ project: '3fA4nXRpYK1JPkeyef3KXFMEs4aN', name: 'Untitled' });
    expect('file' in ref).toBe(false);
    expect('server' in ref).toBe(false);
  });
});
