import { CARD_SIZE_CONFIG } from "../ClipboardCard";

// Pagination and preload configuration
export const PAGE_SIZE = 100;
export const OVERSCAN = 5;
export const PRELOAD_SIZE = 100; // Preload count; load more before and after during fast scrolling
export const TYPE_PRELOAD_LIMIT = 2000; // Avoid warming all item types on startup for large lists because it can stall the WebView

// Dynamic height configuration
export const IMAGE_HEIGHT = 96; // Fixed image card height
export const CARD_GAP = 5; // Card gap

// Content area padding configuration
export const CONTENT_PADDING_TOP = CARD_GAP; // Leave one card gap at the top
export const CONTENT_PADDING_BOTTOM = CARD_GAP; // Leave one card gap at the bottom
export const CONTENT_PADDING_LEFT = 10; // Left padding
export const CONTENT_PADDING_RIGHT = 3; // Right padding, close to the scrollbar

// Scrollbar configuration
export const SCROLLBAR_WIDTH = 6; // Scrollbar width
export const SCROLLBAR_GAP = 4; // Gap between scrollbar and content

// Get text card height from line height
export function getTextHeight(
  lineHeight: "small" | "medium" | "large",
): number {
  return CARD_SIZE_CONFIG[lineHeight].textHeight;
}

// Create a transparent drag image to avoid the browser default rectangular background
export const createTransparentDragImage = (): HTMLElement => {
  const div = document.createElement("div");
  div.style.width = "1px";
  div.style.height = "1px";
  div.style.position = "absolute";
  div.style.top = "-1000px";
  div.style.opacity = "0";
  div.style.pointerEvents = "none";
  document.body.appendChild(div);
  return div;
};
