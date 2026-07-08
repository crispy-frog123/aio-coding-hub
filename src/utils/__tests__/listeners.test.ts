import { describe, expect, it, vi } from "vitest";
import { emitListenerSnapshot } from "../listeners";

describe("utils/listeners", () => {
  it("notifies current listeners in snapshot order", () => {
    const calls: string[] = [];
    const first = vi.fn(() => calls.push("first"));
    const second = vi.fn(() => calls.push("second"));
    const listeners = new Set([first, second]);

    emitListenerSnapshot(listeners, (listener) => listener());

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    expect(calls).toEqual(["first", "second"]);
  });

  it("skips listeners removed during snapshot traversal", () => {
    const second = vi.fn();
    const listeners = new Set<() => void>();
    const first = vi.fn(() => {
      listeners.delete(second);
    });
    listeners.add(first);
    listeners.add(second);

    emitListenerSnapshot(listeners, (listener) => listener());

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).not.toHaveBeenCalled();
  });

  it("forwards listener errors and swallows onError failures", () => {
    const boom = new Error("boom");
    const listener = vi.fn(() => {
      throw boom;
    });
    const onError = vi.fn(() => {
      throw new Error("nested");
    });

    expect(() =>
      emitListenerSnapshot(new Set([listener]), (current) => current(), onError)
    ).not.toThrow();
    expect(onError).toHaveBeenCalledWith(boom);
  });
});
