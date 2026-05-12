import { z } from 'zod';

// Color scheme types
export const ColorSchemeSchema = z.enum(['auto', 'dark', 'light']);
export type ColorScheme = z.infer<typeof ColorSchemeSchema>;

// Schema definition - single source of truth
export const UserPreferencesSchema = z.object({
  version: z.literal(1),
  scrollSyncEnabled: z.boolean(),
  errorOverlayCollapsed: z.boolean(),
  colorScheme: ColorSchemeSchema,
  // Authorship overlay (Phase 5c). Off by default — colours node
  // borders/labels in the q2-debug preview by their last-touch
  // Automerge actor, with display name + colour resolved by
  // `useAttribution` (replay + fnv1a fallback) and pre-baked into
  // `astContext.attribution` / `astContext.attributionActors` by the
  // Rust render transform. `.default(false)` so localStorage entries
  // written before this key existed don't fail validation and reset
  // every other preference.
  attributionEnabled: z.boolean().default(false),
});

// Infer TypeScript type from schema
export type UserPreferences = z.infer<typeof UserPreferencesSchema>;

// Keys that can be updated (excludes version)
export type PreferenceKey = keyof Omit<UserPreferences, 'version'>;

// Default values
export const DEFAULT_PREFERENCES: UserPreferences = {
  version: 1,
  scrollSyncEnabled: true,
  errorOverlayCollapsed: true, // collapsed by default
  colorScheme: 'auto',
  attributionEnabled: false, // opt-in surfacing of author identities
};

// Validation function - returns valid preferences or defaults
export function validatePreferences(data: unknown): UserPreferences {
  const result = UserPreferencesSchema.safeParse(data);
  return result.success ? result.data : DEFAULT_PREFERENCES;
}
