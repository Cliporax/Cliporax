import React from "react";
import { useTheme } from "../../contexts/ThemeContext";

interface DropIndicatorProps {
  /** Bottom position of the previous item, or 0 for the first item */
  previousBottom: number;
  /** Top position of the target item */
  currentTop: number;
}

/**
 * Drag insertion position indicator
 * Displayed in the middle of the gap between two items, not attached to either item
 * Uses left: 0 and right: 0 so it spans the full list width
 */
export const DropIndicator: React.FC<DropIndicatorProps> = ({
  previousBottom,
  currentTop,
}) => {
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  // Calculate the middle of the gap
  const gap = currentTop - previousBottom;
  const indicatorTop = previousBottom + gap / 2 - 2; // Subtract half of the indicator line height

  return (
    <div
      style={{
        position: "absolute",
        top: indicatorTop,
        left: 0,
        right: 0,
        height: 4,
        zIndex: 50,
        pointerEvents: "none",
      }}
    >
      {/* Main indicator line - thicker and more visible */}
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: 4,
          background: `linear-gradient(90deg, 
            transparent 0%, 
            ${isDark ? "#60a5fa" : "#3b82f6"} 15%, 
            ${isDark ? "#93c5fd" : "#60a5fa"} 50%, 
            ${isDark ? "#60a5fa" : "#3b82f6"} 85%, 
            transparent 100%
          )`,
          borderRadius: "2px",
          boxShadow: `
            0 0 12px ${isDark ? "rgba(96, 165, 250, 0.8)" : "rgba(59, 130, 246, 0.6)"},
            0 0 24px ${isDark ? "rgba(96, 165, 250, 0.4)" : "rgba(59, 130, 246, 0.3)"}
          `,
          animation: "dropIndicatorPulse 1.5s ease-in-out infinite",
        }}
      />

      {/* Left arrow indicator */}
      <div
        style={{
          position: "absolute",
          left: -6,
          top: "50%",
          transform: "translateY(-50%)",
          width: 0,
          height: 0,
          borderLeft: `6px solid ${isDark ? "#60a5fa" : "#3b82f6"}`,
          borderTop: "6px solid transparent",
          borderBottom: "6px solid transparent",
          filter: `drop-shadow(0 0 4px ${isDark ? "rgba(96, 165, 250, 0.8)" : "rgba(59, 130, 246, 0.6)"})`,
        }}
      />

      {/* Right arrow indicator */}
      <div
        style={{
          position: "absolute",
          right: -6,
          top: "50%",
          transform: "translateY(-50%)",
          width: 0,
          height: 0,
          borderRight: `6px solid ${isDark ? "#60a5fa" : "#3b82f6"}`,
          borderTop: "6px solid transparent",
          borderBottom: "6px solid transparent",
          filter: `drop-shadow(0 0 4px ${isDark ? "rgba(96, 165, 250, 0.8)" : "rgba(59, 130, 246, 0.6)"})`,
        }}
      />

      {/* Animation styles */}
      <style>{`
        @keyframes dropIndicatorPulse {
          0%, 100% {
            opacity: 1;
            transform: scaleY(1);
          }
          50% {
            opacity: 0.7;
            transform: scaleY(1.3);
          }
        }
      `}</style>
    </div>
  );
};

export default DropIndicator;
