/**
 * Iframe-safe code-copy handler for q2-preview / hub-client (bd-wa2pgri8).
 *
 * The native `q2 render` path wires `.code-copy-button` clicks via
 * `ClipboardJsStage` / the reveal `js:revealjs:*` assets (bd-lg6t6qfy). Those
 * are deliberately NOT shipped to the WASM iframe: the preview re-renders the
 * whole AST on every edit, which replaces button nodes and would orphan any
 * listener bound to a specific node (`pipeline.rs` "hub-client iframe reinit").
 *
 * This is the React-level replacement. It installs ONE **delegated** listener
 * on a stable root (PreviewRoot's `previewHostRef`, which wraps BOTH the
 * `<RevealDeck>` and the plain-HTML `<Ast>` branch), matching
 * `event.target.closest('.code-copy-button')`. Because the root persists across
 * edit re-renders and we match by selector, buttons created/destroyed by a
 * re-render need no re-binding — the property the original stateful bind lacked.
 * One mount point covers reveal AND plain-HTML preview, in both q2-preview and
 * hub-client (both render through `PreviewRoot`).
 *
 * Copy uses `navigator.clipboard.writeText` — the codebase's established pattern
 * (`ShareDialog`, `Editor`, `ProjectTab`); no clipboard.js dependency. The
 * "Copied!" feedback is the `code-copy-button-checked` class (its checkmark SVG
 * ships in `copy-code.scss`); there is no Bootstrap tooltip, matching native v1.
 *
 * **Why capture phase + stopPropagation.** PreviewRoot activates the block
 * editor from `onPointerUp` (`useBlockEditHover`). A copy click must not also
 * open the editor. Listening in the capture phase on the host — an ancestor of
 * every block — lets us `stopPropagation()` on a copy-button's pointer/click
 * events before they reach React's bubble-phase block handlers, so the copy is
 * isolated from the edit surface. We do NOT `preventDefault()` the pointer
 * events (the browser still synthesises the `click` we copy on).
 */

const COPY_BUTTON_SELECTOR = '.code-copy-button';
const SCAFFOLD_SELECTOR = '.code-copy-outer-scaffold';
const CHECKED_CLASS = 'code-copy-button-checked';
/** How long the checkmark stays before reverting (matches native code-copy-init.js). */
const CHECKED_MS = 1000;

/**
 * Extract the text to copy for a copy button: the `<code>` inside the button's
 * scaffold, with `.code-annotation-*` markers removed (mirrors native
 * `code-copy-init.js::getTextToCopy`). Returns `null` when no code is found.
 */
function getTextToCopy(button: Element): string | null {
    const scaffold = button.closest(SCAFFOLD_SELECTOR);
    const code = scaffold?.querySelector('code');
    if (!code) return null;
    // Clone so stripping annotation markers doesn't mutate the rendered DOM.
    const clone = code.cloneNode(true) as HTMLElement;
    clone
        .querySelectorAll('[class*="code-annotation-"]')
        .forEach((el) => el.remove());
    return clone.textContent ?? '';
}

/** Add the checkmark class, then revert after CHECKED_MS. */
function flashChecked(button: HTMLElement): void {
    button.classList.add(CHECKED_CLASS);
    button.ownerDocument.defaultView?.setTimeout(() => {
        button.classList.remove(CHECKED_CLASS);
    }, CHECKED_MS);
}

/**
 * Install the delegated code-copy handler on `root`. Returns a cleanup function
 * that removes every listener (call it from a React effect's cleanup).
 */
export function installCodeCopy(root: HTMLElement): () => void {
    // Swallow pointer events on a copy button so they never reach PreviewRoot's
    // pointer-driven block-edit activation. Capture phase + stopPropagation only
    // (no preventDefault — the click must still fire so we can copy on it).
    const onPointer = (ev: Event) => {
        const target = ev.target as Element | null;
        if (target?.closest?.(COPY_BUTTON_SELECTOR)) {
            ev.stopPropagation();
        }
    };

    const onClick = (ev: Event) => {
        const target = ev.target as Element | null;
        const button = target?.closest?.(COPY_BUTTON_SELECTOR);
        if (!button) return;
        // Isolate the copy from the edit surface and any default button action.
        ev.stopPropagation();
        ev.preventDefault();

        const text = getTextToCopy(button);
        if (text == null) return;

        const clipboard = root.ownerDocument.defaultView?.navigator?.clipboard;
        if (!clipboard?.writeText) return;
        void clipboard.writeText(text).then(
            () => flashChecked(button as HTMLElement),
            () => {
                /* copy failed (permissions / insecure context) — no feedback */
            },
        );
    };

    root.addEventListener('pointerdown', onPointer, true);
    root.addEventListener('pointerup', onPointer, true);
    root.addEventListener('click', onClick, true);

    return () => {
        root.removeEventListener('pointerdown', onPointer, true);
        root.removeEventListener('pointerup', onPointer, true);
        root.removeEventListener('click', onClick, true);
    };
}
