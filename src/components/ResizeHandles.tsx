import type { CSSProperties, MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createLogger } from "../utils/logger";

const logger = createLogger("ResizeHandles");

const EDGE_SIZE = 6;
const SIDE_EDGE_SIZE = 3;
const CORNER_SIZE = 18;

type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

type ResizeHandle = {
  direction: ResizeDirection;
  cursor: CSSProperties["cursor"];
  style: CSSProperties;
};

const baseStyle: CSSProperties = {
  position: "fixed",
  zIndex: 9999,
  background: "transparent",
  userSelect: "none",
};

const handles: ResizeHandle[] = [
  {
    direction: "North",
    cursor: "ns-resize",
    style: { top: 0, left: CORNER_SIZE, right: CORNER_SIZE, height: EDGE_SIZE },
  },
  {
    direction: "South",
    cursor: "ns-resize",
    style: { bottom: 0, left: CORNER_SIZE, right: CORNER_SIZE, height: EDGE_SIZE },
  },
  {
    direction: "West",
    cursor: "ew-resize",
    style: {
      top: CORNER_SIZE,
      bottom: CORNER_SIZE,
      left: 0,
      width: SIDE_EDGE_SIZE,
    },
  },
  {
    direction: "East",
    cursor: "ew-resize",
    style: {
      top: CORNER_SIZE,
      bottom: CORNER_SIZE,
      right: 0,
      width: SIDE_EDGE_SIZE,
    },
  },
  {
    direction: "NorthWest",
    cursor: "nwse-resize",
    style: { top: 0, left: 0, width: CORNER_SIZE, height: CORNER_SIZE },
  },
  {
    direction: "NorthEast",
    cursor: "nesw-resize",
    style: { top: 0, right: 0, width: CORNER_SIZE, height: CORNER_SIZE },
  },
  {
    direction: "SouthWest",
    cursor: "nesw-resize",
    style: { bottom: 0, left: 0, width: CORNER_SIZE, height: CORNER_SIZE },
  },
  {
    direction: "SouthEast",
    cursor: "nwse-resize",
    style: { bottom: 0, right: 0, width: CORNER_SIZE, height: CORNER_SIZE },
  },
];

export function ResizeHandles() {
  const startResize = async (
    event: MouseEvent<HTMLDivElement>,
    direction: ResizeDirection,
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();

    try {
      await getCurrentWindow().startResizeDragging(direction);
    } catch (error) {
      logger.error("Failed to start resize dragging:", error);
    }
  };

  return (
    <>
      {handles.map((handle) => (
        <div
          key={handle.direction}
          aria-hidden="true"
          onMouseDown={(event) => startResize(event, handle.direction)}
          style={{
            ...baseStyle,
            ...handle.style,
            cursor: handle.cursor,
          }}
        />
      ))}
    </>
  );
}
