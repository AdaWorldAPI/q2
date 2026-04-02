/**
 * IndexedDB storage for the project set pointer.
 *
 * After migration, IndexedDB stores only a singleton pointer to the
 * Automerge-backed ProjectSetDocument. The actual project list lives
 * in the Automerge document, synced across browsers.
 */
import type { ProjectSetPointer } from './storage/types';
import { STORES, getDb } from './storage';

/**
 * Get the stored project set pointer, or null if not yet configured.
 */
export async function getProjectSetPointer(): Promise<ProjectSetPointer | null> {
  const db = await getDb();
  if (!db.objectStoreNames.contains(STORES.PROJECT_SET)) {
    return null;
  }
  const pointer = await db.get(STORES.PROJECT_SET, 'projectSet');
  return pointer ?? null;
}

/**
 * Store the project set pointer.
 * This is the commit point for migration — only call this after the
 * Automerge ProjectSetDocument has been successfully created and synced.
 */
export async function setProjectSetPointer(
  projectSetDocId: string,
  syncServer: string,
): Promise<void> {
  const db = await getDb();
  const pointer: ProjectSetPointer = {
    key: 'projectSet',
    projectSetDocId,
    syncServer,
  };
  await db.put(STORES.PROJECT_SET, pointer);
}

/**
 * Clear the project set pointer.
 * Used when unlinking from a project set (e.g., to switch to a different one).
 */
export async function clearProjectSetPointer(): Promise<void> {
  const db = await getDb();
  if (db.objectStoreNames.contains(STORES.PROJECT_SET)) {
    await db.delete(STORES.PROJECT_SET, 'projectSet');
  }
}
