import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeVariants = cva('inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors', {
  variants: {
    variant: {
      default: 'border-transparent bg-[hsl(var(--primary)/0.18)] text-[hsl(var(--primary))]',
      secondary: 'border-[hsl(var(--border))] bg-[hsl(var(--secondary))] text-[hsl(var(--secondary-foreground))]',
      outline: 'border-[hsl(var(--border))] text-[hsl(var(--foreground))]',
      warning: 'border-amber-300/30 bg-amber-300/10 text-amber-200',
      success: 'border-emerald-300/30 bg-emerald-300/10 text-emerald-200',
    },
  },
  defaultVariants: { variant: 'default' },
});

export interface BadgeProps extends React.HTMLAttributes<HTMLDivElement>, VariantProps<typeof badgeVariants> {}
function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}
export { Badge, badgeVariants };
