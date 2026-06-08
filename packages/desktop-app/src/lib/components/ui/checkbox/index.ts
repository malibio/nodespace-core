import { Checkbox as CheckboxPrimitive } from 'bits-ui';
import type { WithoutChildrenOrChild } from '$lib/utils.js';
import Checkbox from './checkbox.svelte';

export type CheckboxProps = WithoutChildrenOrChild<CheckboxPrimitive.RootProps>;

export { Checkbox };
