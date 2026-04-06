# Secure Preview Architecture for Untrusted Web Content

## Overview

This document describes an architecture for safely previewing user-authored web content — including arbitrary JavaScript — within a web-based authoring tool. The goal is to minimize the blast radius when a user opens a malicious document shared with them.

The core idea is to render untrusted content in a sandboxed iframe hosted on a **separate origin** from the main application, using service workers on both domains to enable a fully static SPA deployment.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  app.example.com                                    │
│                                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │  Authoring Tool (editor, collaboration, auth) │  │
│  │                                               │  │
│  │  Service Worker: caches app shell & assets    │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │  <iframe                                      │  │
│  │    src="https://preview.example.com/render"   │  │
│  │    sandbox="allow-scripts allow-same-origin">  │  │
│  │  </iframe>                                    │  │
│  └──────────────────┬────────────────────────────┘  │
│                     │ postMessage                    │
└─────────────────────┼───────────────────────────────┘
                      │
┌─────────────────────┼───────────────────────────────┐
│  preview.example.com│                                │
│                     ▼                                │
│  ┌───────────────────────────────────────────────┐  │
│  │  Shell Page: receives content via postMessage,│  │
│  │  clears persistent state, injects authored    │  │
│  │  HTML into the DOM                            │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
│  Service Worker: serves the shell page, handles    │
│  subresource requests from authored content by      │
│  delegating to the parent via postMessage           │
└─────────────────────────────────────────────────────┘
```

### Why two domains?

The browser's same-origin policy is the primary security boundary. By hosting previews on `preview.example.com`, malicious content in the iframe cannot:

- Access `app.example.com`'s DOM, cookies, localStorage, or session tokens.
- Make authenticated API requests to `app.example.com`'s backend.
- Interfere with the editor or collaborative editing state.

This isolation is enforced by the browser itself and does not depend on the `sandbox` attribute alone.

### Why `sandbox="allow-scripts allow-same-origin"`?

- **`allow-scripts`** is required because we are previewing user-authored JavaScript.
- **`allow-same-origin`** is required so that the preview iframe retains `preview.example.com` as its origin. Without it, the iframe receives an opaque (null) origin, and:
  - The service worker on `preview.example.com` **will not intercept** subresource requests from the iframe.
  - `navigator.serviceWorker` is inaccessible, making registration and integrity checks impossible.
- This combination is safe **only because** `preview.example.com` is a different origin from `app.example.com`. The `allow-same-origin` flag gives the iframe access to `preview.example.com`'s own storage — but that domain is intentionally kept bare, with nothing valuable to steal.

**Important:** The following sandbox flags should be **omitted** unless there is a specific, justified need:

| Flag | Risk |
|---|---|
| `allow-top-navigation` | Malicious content can redirect the user away from the authoring tool (e.g., to a phishing page). |
| `allow-top-navigation-by-user-activation` | Same risk, triggered on click. |
| `allow-popups` | Can be used to escape sandbox restrictions via `window.opener` in some browsers. |
| `allow-forms` | Could submit forms to arbitrary endpoints. |

---

## Service Worker on `app.example.com`

This service worker is conventional. It caches the authoring tool's shell, assets, and any other application resources. It has no interaction with the preview iframe.

```javascript
// sw-app.js — registered on app.example.com
const CACHE_NAME = 'app-shell-v1';
const APP_ASSETS = [
  '/',
  '/index.html',
  '/app.js',
  '/styles.css',
  // ... other application assets
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME)
      .then(cache => cache.addAll(APP_ASSETS))
  );
});

self.addEventListener('fetch', (event) => {
  event.respondWith(
    caches.match(event.request)
      .then(cached => cached || fetch(event.request))
  );
});
```

No special considerations apply here beyond standard SPA service worker practices.

---

## Service Worker on `preview.example.com`

This service worker serves two purposes:

1. **Serve the shell page statically**, enabling a fully static deployment with no server-side rendering.
2. **Intercept subresource requests** from the authored content and delegate them back to the parent frame on `app.example.com` via `postMessage`.

### The service worker script

```javascript
// sw-preview.js — registered on preview.example.com
const SHELL_CACHE = 'preview-shell-v1';
const SHELL_URL = '/render';

// The shell page HTML, embedded directly so the SW can serve it
// without any network dependency.
const SHELL_HTML = `<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body>
<script src="/preview-shell.js"></script>
</body>
</html>`;

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(SHELL_CACHE).then(cache =>
      cache.put(SHELL_URL, new Response(SHELL_HTML, {
        headers: { 'Content-Type': 'text/html' }
      }))
    )
  );
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Serve the shell page
  if (url.pathname === '/render') {
    event.respondWith(
      caches.match(SHELL_URL).then(r => r || new Response(SHELL_HTML, {
        headers: { 'Content-Type': 'text/html' }
      }))
    );
    return;
  }

  // Serve the shell script
  if (url.pathname === '/preview-shell.js') {
    event.respondWith(
      caches.match('/preview-shell.js')
        .then(r => r || fetch(event.request))
    );
    return;
  }

  // All other requests: delegate to the parent frame.
  // This handles subresource requests from authored content
  // (e.g., <img src="/assets/diagram.png">).
  event.respondWith(
    delegateToParent(event.request)
  );
});

function delegateToParent(request) {
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = (event) => {
      if (event.data.error) {
        resolve(new Response('Not found', { status: 404 }));
      } else {
        resolve(new Response(event.data.body, {
          status: event.data.status || 200,
          headers: event.data.headers || {}
        }));
      }
    };

    // Send to all clients (the shell page), which will
    // relay to the parent via postMessage
    self.clients.matchAll().then(clients => {
      for (const client of clients) {
        client.postMessage({
          type: 'fetch-delegate',
          url: request.url,
          method: request.method,
        }, [channel.port2]);
      }
    });
  });
}
```

### The shell page script

```javascript
// preview-shell.js — runs inside the iframe on preview.example.com

// ============================================================
// STEP 1: Clear all persistent state left by previous documents
// ============================================================
clearPersistentState();

// ============================================================
// STEP 2: Verify service worker integrity
// ============================================================
verifySW();

// ============================================================
// STEP 3: Relay fetch-delegate messages from SW to parent
// ============================================================
navigator.serviceWorker.addEventListener('message', (event) => {
  if (event.data.type === 'fetch-delegate') {
    const port = event.ports[0];
    // Forward to the parent frame on app.example.com
    window.parent.postMessage({
      type: 'resource-request',
      url: event.data.url,
      method: event.data.method,
    }, 'https://app.example.com');

    // Listen for the parent's response
    const handler = (reply) => {
      if (reply.data.type === 'resource-response'
          && reply.data.url === event.data.url) {
        window.removeEventListener('message', handler);
        port.postMessage({
          body: reply.data.body,
          status: reply.data.status,
          headers: reply.data.headers,
        });
      }
    };
    window.addEventListener('message', handler);
  }
});

// ============================================================
// STEP 4: Accept authored content from the parent
// ============================================================
window.addEventListener('message', (event) => {
  if (event.origin !== 'https://app.example.com') return;
  if (event.data.type !== 'preview-content') return;

  document.open();
  document.write(event.data.html);
  document.close();
});

// Signal readiness
window.parent.postMessage(
  { type: 'preview-ready' },
  'https://app.example.com'
);
```

---

## Clearing Persistent State

Because the iframe has `allow-same-origin`, malicious authored content can write to any storage mechanism scoped to `preview.example.com`. The shell page must clear all of these before injecting new content.

### Storage mechanisms to clear

| Mechanism | Persistence | Clearable from JS? | Notes |
|---|---|---|---|
| `localStorage` | Indefinite | Yes | `localStorage.clear()` |
| `sessionStorage` | Per-tab | Yes | `sessionStorage.clear()` |
| `IndexedDB` | Indefinite | Yes | Must enumerate and delete all databases |
| `document.cookie` | Varies | Partially | Cannot clear `HttpOnly` cookies from JS |
| Cache API (`caches`) | Indefinite | Yes | Must enumerate and delete all caches |
| Service worker registration | Indefinite | Yes | Can be replaced by malicious content — see below |
| `BroadcastChannel` | In-memory | N/A | Not persistent, but enables cross-tab interference |
| `SharedWorker` | In-memory | N/A | Not persistent, but shared across tabs on same origin |
| `window.name` | Per-tab, survives navigations | Yes | `window.name = ''` |

### Cleanup implementation

```javascript
function clearPersistentState() {
  // localStorage
  try { localStorage.clear(); } catch (e) {}

  // sessionStorage
  try { sessionStorage.clear(); } catch (e) {}

  // Cookies (non-HttpOnly only)
  try {
    document.cookie.split(';').forEach(c => {
      const name = c.split('=')[0].trim();
      if (!name) return;
      // Clear with and without path, for both the domain and bare
      document.cookie =
        `${name}=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;`;
      document.cookie =
        `${name}=; expires=Thu, 01 Jan 1970 00:00:00 UTC;`;
    });
  } catch (e) {}

  // IndexedDB
  try {
    if (indexedDB.databases) {
      indexedDB.databases().then(dbs =>
        dbs.forEach(db => indexedDB.deleteDatabase(db.name))
      );
    }
  } catch (e) {}

  // Cache API
  try {
    caches.keys().then(keys =>
      Promise.all(keys.map(k => caches.delete(k)))
    );
  } catch (e) {}

  // window.name
  window.name = '';
}
```

### What this does NOT cover

- **`HttpOnly` cookies** set by a server or by a service worker's `Set-Cookie` response header. JavaScript cannot read or delete these. Mitigation: never set `HttpOnly` cookies on `preview.example.com`. If the domain is served entirely from a static host and a service worker, this should not occur.
- **`IndexedDB` enumeration on Firefox.** The `indexedDB.databases()` method is not available in Firefox. If Firefox support is required, consider tracking known database names or using `Clear-Site-Data` headers (see below).

---

## Service Worker Integrity

### The threat

Because the iframe has `allow-scripts` and `allow-same-origin`, malicious authored content can call `navigator.serviceWorker.register('/malicious-sw.js')`. If the malicious service worker activates, it controls all future requests to `preview.example.com` — including the shell page itself. A rogue service worker could:

- Serve a tampered shell page that omits the cleanup script.
- Intercept and modify all subsequent preview content.
- Persist across sessions, affecting future users.

This is the most dangerous persistence vector in this architecture because the rogue code runs **before** the shell page loads — the shell page's cleanup script never gets a chance to execute.

### Mitigation: verify and re-register on every load

```javascript
async function verifySW() {
  try {
    const registrations = await navigator.serviceWorker.getRegistrations();
    for (const reg of registrations) {
      const activeURL = reg.active?.scriptURL || '';
      if (!activeURL.endsWith('/sw-preview.js')) {
        // Rogue service worker detected — unregister it
        await reg.unregister();
      }
    }
    // Always re-register ours to ensure it is current
    await navigator.serviceWorker.register('/sw-preview.js');
  } catch (e) {
    console.error('Service worker verification failed:', e);
  }
}
```

### Limitations of this approach

This check runs inside the shell page — but if a rogue service worker has already replaced the shell page with a tampered version, this code will never execute. To address this:

**Option A: `Clear-Site-Data` header on the shell page.**

If the shell page is served by an actual HTTP server (even a minimal one), the response can include:

```
Clear-Site-Data: "cookies", "storage", "cache"
```

This instructs the browser to wipe all persistent state for the origin **before** the page renders. It also unregisters all service workers. The downside is that your own service worker is also destroyed, so you must re-register it on every page load, and you lose the ability to serve the shell page from the service worker cache (defeating part of the "fully static SPA" goal).

**Option B: Use a unique subdomain per document.**

Instead of a single `preview.example.com`, use `{document-id}.preview.example.com`. Each document gets its own origin, so a rogue service worker on one document's subdomain cannot affect another. This requires wildcard DNS and a wildcard TLS certificate, but it eliminates cross-document contamination entirely. The cleanup script becomes a defense-in-depth measure rather than the primary safeguard.

**Option C: Accept the residual risk.**

If the threat model is "users generally trust shared documents but want to limit blast radius," the service worker replacement attack requires the malicious document to both register a new service worker *and* serve a script at a URL on `preview.example.com`. Since the service worker's `fetch` handler controls what URLs resolve to, a well-written service worker that only serves known, hardcoded assets (the shell page and shell script) will not serve an arbitrary script URL that the malicious `register()` call points to. The rogue registration will fail to install because the browser will try to fetch the script via the existing (legitimate) service worker, which won't serve it.

This is a meaningful defense, but it depends on the legitimate service worker remaining in control during the race between the malicious `register()` call and the service worker update cycle.

---

## Communication Protocol

### Parent → iframe (content injection)

```javascript
// In app.example.com, after the iframe signals readiness:
iframe.contentWindow.postMessage({
  type: 'preview-content',
  html: userAuthoredHTML,
}, 'https://preview.example.com');
```

### Iframe → parent (resource requests)

When authored content requests a subresource (e.g., `<img src="/assets/photo.png">`), the preview service worker intercepts it, delegates to the shell page, which relays to the parent:

```javascript
// In app.example.com, handle resource requests:
window.addEventListener('message', (event) => {
  if (event.origin !== 'https://preview.example.com') return;
  if (event.data.type !== 'resource-request') return;

  const asset = resolveAssetFromProject(event.data.url);
  event.source.postMessage({
    type: 'resource-response',
    url: event.data.url,
    body: asset.body,       // ArrayBuffer or string
    status: asset ? 200 : 404,
    headers: { 'Content-Type': asset.mimeType },
  }, 'https://preview.example.com');
});
```

**Always validate `event.origin`** on both sides. The parent should only accept messages from `https://preview.example.com`, and the shell should only accept messages from `https://app.example.com`.

---

## Threat Summary

| Threat | Mitigated by | Residual risk |
|---|---|---|
| Malicious content accesses app session/cookies | Separate origin (`preview.example.com`) | None — enforced by browser same-origin policy |
| Malicious content navigates user away from app | Omitting `allow-top-navigation` from sandbox | None if flag is omitted |
| Malicious content opens phishing popups | Omitting `allow-popups` from sandbox | None if flag is omitted |
| Malicious content persists XSS via localStorage/IndexedDB | Cleanup script on shell page load | Firefox `indexedDB.databases()` gap |
| Malicious content poisons Cache API | Cleanup script on shell page load | Low — caches are enumerable and deletable |
| Malicious content sets cookies | Cleanup script + keeping preview domain stateless | `HttpOnly` cookies not clearable from JS |
| Malicious content replaces service worker | Integrity check + re-registration on load | Rogue SW could tamper with the shell page itself |
| Cross-document contamination | Cleanup script; or unique subdomains per document | Rogue SW race condition (see above) |
| Malicious content reads other users' data | No shared state on preview domain | None if preview domain is kept bare |

---

## Deployment Checklist

- [ ] Register `preview.example.com` as a separate domain (or subdomain with no shared cookies).
- [ ] Ensure no authentication cookies, API endpoints, or sensitive data exist on `preview.example.com`.
- [ ] Deploy `sw-preview.js` and `preview-shell.js` as static assets on `preview.example.com`.
- [ ] Set the iframe's `sandbox` attribute to exactly `allow-scripts allow-same-origin`.
- [ ] Validate `event.origin` on all `postMessage` handlers, on both sides.
- [ ] Implement the persistent state cleanup in the shell page.
- [ ] Implement service worker integrity verification in the shell page.
- [ ] Consider `Clear-Site-Data` headers or per-document subdomains if the service worker replacement risk is unacceptable for your threat model.
- [ ] Avoid setting any `HttpOnly` cookies on `preview.example.com`.
- [ ] Test with deliberately malicious documents that attempt storage writes, service worker replacement, top navigation, and popup escapes.