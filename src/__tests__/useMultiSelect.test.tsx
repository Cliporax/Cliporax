import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ClipboardCacheManager } from "../components/ClipboardList/cache";
import { useMultiSelect } from "../components/ClipboardList/hooks/useMultiSelect";

vi.mock("../lib/tauri-api", () => ({
  clipboard: {},
  window: {},
}));

describe("useMultiSelect", () => {
  it("clears the selection and range anchor when multi-select exits", () => {
    const setIsMultiSelectMode = vi.fn();
    const setCheckedIds = vi.fn();
    const setSelectionRange = vi.fn();
    const onMultiSelectChange = vi.fn();

    const { result } = renderHook(() =>
      useMultiSelect({
        selectedId: null,
        isMultiSelectMode: false,
        checkedIds: new Set(),
        selectionRange: null,
        cacheManagerRef: { current: new ClipboardCacheManager() },
        setIsMultiSelectMode,
        setCheckedIds,
        setSelectionRange,
        setSelectedId: vi.fn(),
        onMultiSelectChange,
      }),
    );

    act(() => {
      result.current.handleCardClick(1, 5, {
        ctrlKey: true,
        metaKey: false,
        shiftKey: false,
      } as React.MouseEvent);
      result.current.exitMultiSelectMode();
      result.current.handleCardClick(2, 9, {
        ctrlKey: false,
        metaKey: false,
        shiftKey: true,
      } as React.MouseEvent);
    });

    expect(setIsMultiSelectMode).toHaveBeenCalledWith(false);
    expect(setCheckedIds).toHaveBeenCalledWith(new Set());
    expect(onMultiSelectChange).toHaveBeenCalledWith(new Set(), []);
    expect(setSelectionRange).toHaveBeenLastCalledWith({ start: 9, end: 9 });
  });
});
