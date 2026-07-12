import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Badge, badgeVariants } from "../Badge";

describe("ui/Badge", () => {
  it("maps every supported variant to its visual classes", () => {
    expect(badgeVariants()).toContain("bg-primary");
    expect(badgeVariants({})).toContain("bg-primary");
    expect(badgeVariants({ variant: "secondary" })).toContain("bg-secondary");
    expect(badgeVariants({ variant: "destructive" })).toContain("bg-destructive");
    expect(badgeVariants({ variant: "outline" })).toContain("border-border");
  });

  it("renders children, custom classes, and the selected variant", () => {
    const { rerender } = render(<Badge className="custom-badge">Ready</Badge>);

    expect(screen.getByText("Ready")).toHaveClass("custom-badge", "bg-primary");

    rerender(
      <Badge className="custom-badge" variant="secondary">
        Waiting
      </Badge>
    );
    expect(screen.getByText("Waiting")).toHaveClass("custom-badge", "bg-secondary");
  });
});
