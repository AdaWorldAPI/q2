/**
 * Tests for projectSetStorage service.
 *
 * Verifies IndexedDB operations for the project set pointer —
 * the singleton that points to the Automerge-backed project set document.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import 'fake-indexeddb/auto';
import { IDBFactory } from 'fake-indexeddb';
import {
  getProjectSetPointer,
  setProjectSetPointer,
  clearProjectSetPointer,
} from './projectSetStorage';
import { closeDatabase } from './projectStorage';

describe('projectSetStorage', () => {
  beforeEach(() => {
    closeDatabase();
    const idbFactory = new IDBFactory();
    Object.defineProperty(globalThis, 'indexedDB', {
      value: idbFactory,
      writable: true,
    });
  });

  afterEach(() => {
    closeDatabase();
  });

  it('should return null when no pointer is set', async () => {
    const pointer = await getProjectSetPointer();
    expect(pointer).toBeNull();
  });

  it('should store and retrieve a project set pointer', async () => {
    await setProjectSetPointer('automerge:abc123', 'wss://sync.example.com');

    const pointer = await getProjectSetPointer();
    expect(pointer).not.toBeNull();
    expect(pointer!.projectSetDocId).toBe('automerge:abc123');
    expect(pointer!.syncServer).toBe('wss://sync.example.com');
    expect(pointer!.key).toBe('projectSet');
  });

  it('should overwrite an existing pointer', async () => {
    await setProjectSetPointer('automerge:first', 'wss://server1');
    await setProjectSetPointer('automerge:second', 'wss://server2');

    const pointer = await getProjectSetPointer();
    expect(pointer!.projectSetDocId).toBe('automerge:second');
    expect(pointer!.syncServer).toBe('wss://server2');
  });

  it('should clear the pointer', async () => {
    await setProjectSetPointer('automerge:toDelete', 'wss://server');

    await clearProjectSetPointer();

    const pointer = await getProjectSetPointer();
    expect(pointer).toBeNull();
  });

  it('should handle clearing when no pointer exists', async () => {
    // Should not throw
    await clearProjectSetPointer();
    const pointer = await getProjectSetPointer();
    expect(pointer).toBeNull();
  });
});
