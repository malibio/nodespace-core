/**
 * Icon registry — built-in node type icon configs.
 *
 * `project` is a built-in core node type and must resolve to a registered
 * icon config rather than falling back to the default text icon.
 */

import { describe, it, expect } from 'vitest';
import { iconRegistry, getIconConfig } from '$lib/design/icons/registry';

describe('icon registry — project', () => {
  it('resolves a registered icon config for project (not the unknown-type fallback)', () => {
    expect(iconRegistry.hasConfig('project')).toBe(true);
    // Sanity: an unregistered type is NOT reported as having its own config.
    expect(iconRegistry.hasConfig('definitely-not-a-real-node-type')).toBe(false);
  });

  it('exposes a usable icon component and config for project, mirroring task', () => {
    const projectConfig = getIconConfig('project');
    const taskConfig = getIconConfig('task');

    expect(projectConfig.component).toBeTruthy();
    expect(taskConfig.component).toBeTruthy();

    // Project is a container entity: no per-state icon, shows a ring for children.
    expect(projectConfig.hasState).toBe(false);
    expect(projectConfig.hasRingEffect).toBe(true);
    expect(projectConfig.semanticClass).toBe('node-icon');
  });
});
