<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-4 text-xl font-semibold">Import Sources</h2>
    <p class="text-muted-foreground mb-6 text-sm">Configure external sources for importing content into NodeSpace.</p>

    <div class="border-border flex items-center justify-between border-b py-3">
        <span class="text-foreground text-sm">Local Folder Import</span>
        <Badge class="border-green-500/25 bg-green-500/12 text-green-700">Available</Badge>
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
