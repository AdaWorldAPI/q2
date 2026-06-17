import { useCallback, useEffect } from 'react';
import type * as Monaco from 'monaco-editor';
import {
  registerIntelligenceProviders,
  disposeIntelligenceProviders,
} from '../services/monacoProviders';

/**
 * Own the lifecycle of the global Monaco intelligence providers (document
 * symbols, folding ranges, semantic tokens).
 *
 * The providers are registered once when the editor mounts (we need the
 * `monaco` instance from the mount callback) and disposed once when the editor
 * unmounts. The disposal is deliberately on its own mount-only effect: it must
 * NOT be coupled to anything that changes per `currentFile`. A previous version
 * wired `disposeIntelligenceProviders()` into an effect whose deps tracked a
 * `currentFile`-dependent callback, so opening/switching a file could dispose
 * the providers with no remount to re-register them — leaving the editor with
 * no semantic-tokens provider (links' `[`/`]` rendered by the Monarch base
 * only, hence mismatched) until a full page reload.
 *
 * @param getCurrentFilePath - Stable getter for the active VFS path
 * @returns An editor-mount handler that registers the providers
 */
export function useIntelligenceProviders(
  getCurrentFilePath: () => string | null
): (monaco: typeof Monaco) => void {
  const onEditorMount = useCallback(
    (monaco: typeof Monaco) => {
      registerIntelligenceProviders(monaco, getCurrentFilePath);
    },
    [getCurrentFilePath]
  );

  useEffect(() => {
    return () => {
      disposeIntelligenceProviders();
    };
  }, []);

  return onEditorMount;
}
