<!--
  SchemaFieldLeaf — renders ONE typed leaf control for a schema field.

  Extracted verbatim (in behavior) from GenericSchemaForm's five leaf cases so
  that leaf rendering lives in a single place, reused by both the top-level
  generic form and the recursive nested-field editor. It is a controlled,
  presentational component: it renders a control bound to `value` and calls
  `onChange(newValue)` on every edit. It never touches the store.

  Field type → control:
  - enum    → Select (coreValues + userValues)
  - date    → Popover + Calendar
  - number  → number Input
  - boolean → checkbox
  - string/text → text Input
  - anything else → "Unknown field type" fallback
-->
<script lang="ts">
  import * as Select from '$lib/components/ui/select';
  import * as Popover from '$lib/components/ui/popover';
  import { Calendar } from '$lib/components/ui/calendar';
  import { Input } from '$lib/components/ui/input';
  import type { SchemaField, EnumValue } from '$lib/types/schema-node';
  import { parseDate, type DateValue } from '@internationalized/date';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('SchemaFieldLeaf');

  let {
    field,
    value,
    onChange,
    fieldId
  }: {
    field: SchemaField;
    value: unknown;
    onChange: (_value: unknown) => void;
    fieldId: string;
  } = $props();

  // Date picker popover state — owned here so picking a date dismisses the
  // calendar instead of leaving it open over the rest of the form.
  let datePickerOpen = $state(false);

  function formatFieldLabel(fieldName: string): string {
    return fieldName
      .replace(/[_-]/g, ' ')
      .split(' ')
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join(' ');
  }

  function getEnumValues(f: SchemaField): EnumValue[] {
    const values: EnumValue[] = [];
    if (f.coreValues) values.push(...f.coreValues);
    if (f.userValues) values.push(...f.userValues);
    return values;
  }

  function parseDateFromValue(raw: string | null | undefined): DateValue | undefined {
    if (!raw) return undefined;
    try {
      const dateOnly = typeof raw === 'string' && raw.includes('T') ? raw.split('T')[0] : raw;
      return parseDate(dateOnly as string);
    } catch (error) {
      log.warn(`Failed to parse date value "${raw}":`, error);
      return undefined;
    }
  }

  function formatDateDisplay(raw: string | null | undefined): string {
    if (!raw) return 'Pick a date';
    const date = parseDateFromValue(raw as string);
    return date ? date.toString() : (raw as string);
  }

  function formatDateForStorage(dateValue: DateValue | undefined): string | null {
    if (!dateValue) return null;
    return `${dateValue.year}-${String(dateValue.month).padStart(2, '0')}-${String(dateValue.day).padStart(2, '0')}`;
  }
</script>

{#if field.type === 'enum'}
  {@const enumValues = getEnumValues(field)}
  {@const currentValue = (value as string) || ''}
  <Select.Root
    type="single"
    value={currentValue}
    onValueChange={(newValue) => onChange(newValue)}
  >
    <Select.Trigger class="w-full">
      <!-- A stored value the schema no longer declares still reads as a label rather than a
           raw key, so the control agrees with the collapsed header that humanizes the same
           value. -->
      {enumValues.find((ev) => ev.value === currentValue)?.label ||
        (currentValue ? formatFieldLabel(currentValue) : '') ||
        `Select ${formatFieldLabel(field.name)}...`}
    </Select.Trigger>
    <Select.Content>
      {#each enumValues as ev}
        <Select.Item value={ev.value} label={ev.label} />
      {/each}
    </Select.Content>
  </Select.Root>
{:else if field.type === 'date'}
  {@const rawValue = value as string | null}
  {@const dateVal = parseDateFromValue(rawValue)}
  <Popover.Root bind:open={datePickerOpen}>
    <Popover.Trigger
      id={fieldId}
      class="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none"
    >
      <span class={dateVal ? '' : 'text-muted-foreground'}>
        {formatDateDisplay(rawValue)}
      </span>
      <svg class="h-4 w-4 opacity-50" viewBox="0 0 16 16" fill="none">
        <rect x="2" y="3" width="12" height="11" rx="1" stroke="currentColor" stroke-width="1.5" />
        <path d="M5 1v3M11 1v3M2 6h12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </Popover.Trigger>
    <Popover.Content class="w-auto p-0" align="start">
      <Calendar
        value={dateVal as never}
        onValueChange={(newValue: DateValue | DateValue[] | undefined) => {
          const singleValue = Array.isArray(newValue) ? newValue[0] : newValue;
          onChange(formatDateForStorage(singleValue));
          datePickerOpen = false;
        }}
        type="single"
      />
    </Popover.Content>
  </Popover.Root>
{:else if field.type === 'number'}
  <Input
    id={fieldId}
    type="number"
    value={(value as number) ?? (field.default as number) ?? 0}
    oninput={(e) => onChange(parseFloat(e.currentTarget.value) || 0)}
  />
{:else if field.type === 'boolean'}
  <div class="flex items-center gap-2 h-10">
    <input
      id={fieldId}
      type="checkbox"
      checked={!!(value as boolean)}
      onchange={(e) => onChange(e.currentTarget.checked)}
      class="h-4 w-4 rounded border-input"
    />
  </div>
{:else if field.type === 'string' || field.type === 'text'}
  <Input
    id={fieldId}
    type="text"
    value={(value as string) || ''}
    oninput={(e) => onChange(e.currentTarget.value)}
    placeholder={field.default ? String(field.default) : ''}
  />
{:else}
  <div class="text-sm text-muted-foreground">Unknown field type: {field.type}</div>
{/if}
