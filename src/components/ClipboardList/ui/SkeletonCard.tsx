import React from "react";
import { getTextHeight } from "../constants";

interface SkeletonCardProps {
  height?: number;
  isDark: boolean;
  lineHeight?: "small" | "medium" | "large";
}

export const SkeletonCard: React.FC<SkeletonCardProps> = ({
  height,
  isDark,
  lineHeight = "medium",
}) => {
  const actualHeight = height ?? getTextHeight(lineHeight);

  return (
    <div
      style={{
        width: "100%",
        height: actualHeight,
        padding: "8px 10px",
        borderRadius: "10px",
        background: isDark ? "rgba(255,255,255,0.03)" : "rgba(0,0,0,0.02)",
        border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
      }}
    >
      <div
        style={{
          width: "60%",
          height: "12px",
          borderRadius: "4px",
          background: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)",
        }}
      />
    </div>
  );
};
