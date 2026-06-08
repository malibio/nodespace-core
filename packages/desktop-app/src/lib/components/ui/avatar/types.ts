/**
 * Avatar component types
 */
import type { Avatar as AvatarPrimitive } from 'bits-ui';
import { type VariantProps, tv } from 'tailwind-variants';

export const avatarVariants = tv({
  base: 'relative flex shrink-0 overflow-hidden',
  variants: {
    size: {
      xs: 'size-6',
      sm: 'size-8',
      md: 'size-10',
      lg: 'size-14'
    },
    shape: {
      circle: 'rounded-full',
      square: 'rounded-md'
    }
  },
  defaultVariants: {
    size: 'md',
    shape: 'circle'
  }
});

export type AvatarSize = VariantProps<typeof avatarVariants>['size'];
export type AvatarShape = VariantProps<typeof avatarVariants>['shape'];

export type AvatarProps = AvatarPrimitive.RootProps & {
  size?: AvatarSize;
  shape?: AvatarShape;
};
