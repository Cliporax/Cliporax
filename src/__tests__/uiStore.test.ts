import { describe, expect, it, beforeEach } from "vitest";
import { useUIStore } from "../stores/uiStore";

describe("useUIStore search scope", () => {
  beforeEach(() => {
    useUIStore.setState({ searchScope: "current" });
  });

  it("defaults search scope to current tab", () => {
    expect(useUIStore.getState().searchScope).toBe("current");
  });

  it("can switch search scope", () => {
    useUIStore.getState().setSearchScope("global");

    expect(useUIStore.getState().searchScope).toBe("global");
  });
});
