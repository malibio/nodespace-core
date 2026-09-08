/**
 * Import Modal Store
 *
 * Shared open/closed state for `ImportOptionsModal`, mounted once in
 * `app-shell.svelte`. Both the File → Import → "Import Folder..." menu and
 * Settings → Import Sources open the same modal instance through this store
 * instead of each owning a separate import flow.
 *
 * Svelte 5 rune store (ADR-049): reactive state lives on the class as `$state`.
 */
class ImportModalStore {
  open = $state(false);

  show(): void {
    this.open = true;
  }
}

export const importModalStore = new ImportModalStore();
