<script lang="ts">
    import { appSettings, updateDisplaySetting } from '$lib/stores/settings';
    import { setTheme } from '$lib/design/theme';
    import type { Theme } from '$lib/design/tokens';
    import { Label } from '$lib/components/ui/label';
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-6 text-xl font-semibold">Display</h2>

    <div class="mb-6">
        <Label for="theme-select" class="mb-2 block">Theme</Label>
        <select
            id="theme-select"
            class="border-input bg-background text-foreground rounded-[var(--radius)] min-w-[200px] border px-3 py-2 text-sm"
            value={$appSettings?.display?.theme ?? 'system'}
            onchange={async (e) => {
                const value = e.currentTarget.value;
                await updateDisplaySetting('theme', value);
                setTheme(value as Theme);
            }}
        >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
        </select>
    </div>

    <div class="mb-6">
        <label class="text-foreground flex cursor-pointer items-center gap-3 text-sm">
            <input
                type="checkbox"
                checked={$appSettings?.display?.renderMarkdown ?? false}
                onchange={async (e) => {
                    await updateDisplaySetting('renderMarkdown', e.currentTarget.checked);
                }}
                class="border-input h-4 w-4 cursor-pointer rounded"
            />
            <span>Render Markdown in node content</span>
        </label>
    </div>
</div>
