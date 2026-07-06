import type { HTMLAttributes } from "react";
import { cn } from "../utils/cn";

export type BadgeVariant = "default" | "secondary" | "destructive" | "outline";

export type BadgeProps = HTMLAttributes<HTMLDivElement> & {
  variant?: BadgeVariant;
};

export function badgeVariants({ variant = "default" }: { variant?: BadgeVariant } = {}) {
  return cn(
    "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium transition-colors",
    variant === "default" && "border-transparent bg-primary text-primary-foreground",
    variant === "secondary" && "border-transparent bg-secondary text-secondary-foreground",
    variant === "destructive" && "border-transparent bg-destructive text-destructive-foreground",
    variant === "outline" && "border-border text-foreground"
  );
}

export function Badge({ className, variant = "default", ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}
