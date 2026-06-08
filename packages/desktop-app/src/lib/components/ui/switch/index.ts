import { Switch as SwitchPrimitive } from 'bits-ui';
import type { WithoutChildrenOrChild } from '$lib/utils.js';
import Switch from './switch.svelte';

export type SwitchProps = WithoutChildrenOrChild<SwitchPrimitive.RootProps>;

export { Switch };
