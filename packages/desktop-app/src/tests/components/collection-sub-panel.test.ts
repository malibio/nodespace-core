/**
 * collection-sub-panel component.
 *
 * The fly-out that previews a collection's members. Clicking the panel title
 * opens the collection's own page (Contents / Collaboration tabs) — the
 * reachable path to the collaboration/admin UI — while clicking a member row
 * opens that member node.
 */
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

import CollectionSubPanel from '$lib/components/layout/collection-sub-panel.svelte';

function baseProps() {
  return {
    open: true,
    collectionName: 'Architecture',
    members: [{ id: 'm1', name: 'System Overview', nodeType: 'header' }],
    onClose: vi.fn(),
    onNodeClick: vi.fn(),
    onOpenCollection: vi.fn()
  };
}

describe('CollectionSubPanel', () => {
  afterEach(() => cleanup());

  it('opens the collection page when the title is clicked', async () => {
    const props = baseProps();
    const { getByRole } = render(CollectionSubPanel, { props });

    await fireEvent.click(getByRole('button', { name: /Architecture/i }));

    expect(props.onOpenCollection).toHaveBeenCalledTimes(1);
    expect(props.onNodeClick).not.toHaveBeenCalled();
  });

  it('opens a member node when its row is clicked', async () => {
    const props = baseProps();
    const { getByRole } = render(CollectionSubPanel, { props });

    await fireEvent.click(getByRole('button', { name: /System Overview/i }));

    expect(props.onNodeClick).toHaveBeenCalledWith('m1', 'header');
    expect(props.onOpenCollection).not.toHaveBeenCalled();
  });
});
