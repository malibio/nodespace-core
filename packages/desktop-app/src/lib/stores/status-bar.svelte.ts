/**
 * Status Bar Store
 *
 * Simple store for displaying status messages at the bottom of the app.
 * Used for background operations like import progress and embedding queue status.
 *
 * Svelte 5 rune store (ADR-049): reactive state lives on the class as `$state`;
 * components read `statusBar.state` / `statusBar.visible` directly.
 */

export interface StatusBarState {
  /** Whether status bar is enabled (user preference via View menu) */
  enabled: boolean;
  /** Status message to display */
  message: string;
  /** Progress percentage 0-100 (optional) */
  progress?: number;
  /** Message type for styling */
  type: 'info' | 'success' | 'error';
}

const initialState: StatusBarState = {
  enabled: true, // Default to showing
  message: '',
  type: 'info',
};

class StatusBarStore {
  state = $state<StatusBarState>({ ...initialState });

  #successTimer: ReturnType<typeof setTimeout> | null = null;

  /** Whether the status bar should be visible */
  get visible(): boolean {
    return this.state.enabled;
  }

  /** Toggle status bar visibility (for View menu) */
  toggle(): void {
    this.state = { ...this.state, enabled: !this.state.enabled };
  }

  /** Set status bar enabled state */
  setEnabled(enabled: boolean): void {
    this.state = { ...this.state, enabled };
  }

  /** Show a status message */
  show(message: string, progress?: number): void {
    this.state = { ...this.state, message, progress, type: 'info' };
  }

  /** Show a success message (auto-hides message after 5s, but bar stays) */
  success(message: string): void {
    if (this.#successTimer) clearTimeout(this.#successTimer);
    this.state = { ...this.state, message, type: 'success', progress: undefined };
    this.#successTimer = setTimeout(() => {
      this.#successTimer = null;
      this.state = { ...this.state, message: '', type: 'info' };
    }, 5000);
  }

  /** Show an error message (stays visible) */
  error(message: string): void {
    this.state = { ...this.state, message, type: 'error', progress: undefined };
  }

  /** Update progress */
  updateProgress(current: number, total: number, message?: string): void {
    this.state = {
      ...this.state,
      message: message ?? this.state.message,
      progress: Math.round((current / total) * 100),
    };
  }

  /** Clear the status message (but keep bar visible if enabled) */
  clearMessage(): void {
    this.state = { ...this.state, message: '', progress: undefined, type: 'info' };
  }
}

export const statusBar = new StatusBarStore();
