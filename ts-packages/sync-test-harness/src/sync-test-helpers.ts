/**
 * Test helpers for creating and verifying projects via the sync client.
 *
 * These use the public API of @quarto/quarto-sync-client — the same
 * code path the browser uses.
 */

import {
  createSyncClient,
  type SyncClient,
  type SyncClientCallbacks,
  type CreateProjectResult,
  type FilePayload,
  type Patch,
} from '@quarto/quarto-sync-client';

// ============================================================================
// Types
// ============================================================================

export interface TestFile {
  path: string;
  content: string;
  contentType: 'text' | 'binary';
}

export interface ExpectedFile {
  path: string;
  content: string;
  contentType: 'text';
}

export interface CreateResult {
  indexDocId: string;
  files: Array<{ path: string; docId: string }>;
  client: SyncClient;
}

export interface VerifyResult {
  ok: boolean;
  /** Files that were expected but not found */
  missing: string[];
  /** Files found on server but not expected */
  unexpected: string[];
  /** Files whose content didn't match */
  contentMismatch: Array<{
    path: string;
    expected: string;
    actual: string;
  }>;
  /** All files found on the server */
  foundFiles: Map<string, string>;
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Create a sync client with no-op callbacks that tracks added files.
 * Returns the client and a map of path → content for files received.
 */
function createTrackingClient(): {
  client: SyncClient;
  receivedFiles: Map<string, FilePayload>;
  errors: Error[];
  connected: boolean;
} {
  const receivedFiles = new Map<string, FilePayload>();
  const errors: Error[] = [];
  let connected = false;

  const callbacks: SyncClientCallbacks = {
    onFileAdded(path: string, file: FilePayload) {
      receivedFiles.set(path, file);
    },
    onFileChanged(_path: string, _text: string, _patches: Patch[]) {
      // no-op for test purposes
    },
    onBinaryChanged(_path: string, _data: Uint8Array, _mimeType: string) {
      // no-op
    },
    onFileRemoved(path: string) {
      receivedFiles.delete(path);
    },
    onFilesChange() {
      // no-op
    },
    onConnectionChange(isConnected: boolean) {
      connected = isConnected;
    },
    onError(error: Error) {
      errors.push(error);
      console.error('[sync-test] Error:', error.message);
    },
  };

  const client = createSyncClient(callbacks);

  return {
    client,
    receivedFiles,
    errors,
    get connected() {
      return connected;
    },
  };
}

/**
 * Create a new project on the sync server with the given files.
 *
 * Returns the index document ID, file entries, and the connected client.
 * The caller is responsible for disconnecting the client when done.
 */
export async function createTestProject(
  serverUrl: string,
  files: TestFile[],
): Promise<CreateResult> {
  const { client } = createTrackingClient();

  const result: CreateProjectResult = await client.createNewProject({
    syncServer: serverUrl,
    files,
  });

  return {
    indexDocId: result.indexDocId,
    files: result.files,
    client,
  };
}

/**
 * Verify a project's contents by connecting a fresh client and checking
 * all expected files are present with correct content.
 *
 * Creates a brand-new sync client (no local state) — everything must
 * come from the server. This is the scenario where the hub bug manifests.
 */
export async function verifyProject(
  serverUrl: string,
  indexDocId: string,
  expectedFiles: ExpectedFile[],
): Promise<VerifyResult> {
  const { client, receivedFiles, errors } = createTrackingClient();

  try {
    // Connect and load the project from the server
    await client.connect(serverUrl, indexDocId);

    // Give a moment for any pending syncs to complete
    await sleep(1000);

    // Build result
    const foundFiles = new Map<string, string>();
    for (const [path, payload] of receivedFiles) {
      if (payload.type === 'text') {
        foundFiles.set(path, payload.text);
      }
    }

    const expectedPaths = new Set(expectedFiles.map((f) => f.path));
    const foundPaths = new Set(foundFiles.keys());

    const missing: string[] = [];
    const unexpected: string[] = [];
    const contentMismatch: VerifyResult['contentMismatch'] = [];

    for (const expected of expectedFiles) {
      if (!foundPaths.has(expected.path)) {
        missing.push(expected.path);
      } else {
        const actual = foundFiles.get(expected.path)!;
        if (actual !== expected.content) {
          contentMismatch.push({
            path: expected.path,
            expected: expected.content,
            actual,
          });
        }
      }
    }

    for (const path of foundPaths) {
      if (!expectedPaths.has(path)) {
        unexpected.push(path);
      }
    }

    const ok = missing.length === 0 && contentMismatch.length === 0;

    if (errors.length > 0) {
      console.warn('[verify] Errors during verification:', errors);
    }

    return { ok, missing, unexpected, contentMismatch, foundFiles };
  } finally {
    await client.disconnect();
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
