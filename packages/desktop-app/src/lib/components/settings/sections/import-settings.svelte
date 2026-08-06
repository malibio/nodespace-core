<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
    import { Button } from '$lib/components/ui/button';
    import FolderInputIcon from '@lucide/svelte/icons/folder-input';
    import ImportOptionsModal from '$lib/components/settings/import-options-modal.svelte';

    let importModalOpen = $state(false);
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-4 text-xl font-semibold">Import Sources</h2>
    <p class="text-muted-foreground mb-6 text-sm">Configure external sources for importing content into NodeSpace.</p>

    <div class="border-border flex items-center justify-between border-b py-3">
        <span class="text-foreground text-sm">Local Folder Import</span>
        <Button variant="outline" size="sm" onclick={() => (importModalOpen = true)}>
            <FolderInputIcon />
            Import Folder…
        </Button>
    </div>

    <div class="border-border flex items-center justify-between border-b py-3">
        <span class="text-foreground text-sm">Notion</span>
        <Badge variant="secondary">Coming Soon</Badge>
    </div>

    <div class="border-border flex items-center justify-between border-b py-3">
        <span class="text-foreground text-sm">Confluence</span>
        <Badge variant="secondary">Coming Soon</Badge>
    </div>
</div>

<ImportOptionsModal bind:open={importModalOpen} />
