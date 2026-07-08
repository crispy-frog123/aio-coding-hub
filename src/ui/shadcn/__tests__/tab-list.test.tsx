import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TabList } from "../tab-list";

describe("ui/shadcn/TabList", () => {
  it("changes tab on click and keyboard navigation while skipping disabled items", () => {
    const onChange = vi.fn();
    render(
      <TabList
        ariaLabel="Example tabs"
        value="a"
        onChange={onChange}
        size="md"
        items={[
          { key: "a", label: "Alpha" },
          { key: "b", label: "Beta", disabled: true },
          { key: "c", label: "Gamma" },
        ]}
      />
    );

    fireEvent.click(screen.getByRole("tab", { name: "Gamma" }));
    expect(onChange).toHaveBeenCalledWith("c");

    const tablist = screen.getByRole("tablist", { name: "Example tabs" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    fireEvent.keyDown(tablist, { key: "ArrowLeft" });
    fireEvent.keyDown(tablist, { key: "End" });
    fireEvent.keyDown(tablist, { key: "Home" });

    expect(onChange).toHaveBeenNthCalledWith(2, "c");
    expect(onChange).toHaveBeenNthCalledWith(3, "c");
    expect(onChange).toHaveBeenNthCalledWith(4, "c");
    expect(onChange).toHaveBeenNthCalledWith(5, "a");
  });

  it("ignores unrelated keys and empty enabled sets", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <TabList
        ariaLabel="Disabled tabs"
        value="a"
        onChange={onChange}
        items={[{ key: "a", label: "Alpha", disabled: true }]}
      />
    );

    const tablist = screen.getByRole("tablist", { name: "Disabled tabs" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });

    rerender(
      <TabList
        ariaLabel="Disabled tabs"
        value="a"
        onChange={onChange}
        items={[{ key: "a", label: "Alpha" }]}
      />
    );
    fireEvent.keyDown(tablist, { key: "Enter" });

    expect(onChange).not.toHaveBeenCalled();
  });
});
