import React from "react";
import { useTranslation } from "react-i18next";

interface StatusOverlayProps {
  isSearching: boolean;
  isSearchMode: boolean;
  searchResults: any[];
  searchMode: "fuzzy" | "regex";
  totalCount: number;
  isLoading: boolean;
  isDark: boolean;
}

export const StatusOverlay: React.FC<StatusOverlayProps> = ({
  isSearching,
  isSearchMode,
  searchResults,
  searchMode,
  totalCount,
  isLoading,
  isDark,
}) => {
  const { t } = useTranslation();

  return (
    <>
      {/* Search loading */}
      {isSearching && (
        <div
          style={{
            position: "absolute",
            bottom: 16,
            left: "50%",
            transform: "translateX(-50%)",
            padding: "8px 16px",
            background: isDark
              ? "rgba(30, 41, 59, 0.9)"
              : "rgba(255,255,255,0.9)",
            borderRadius: "8px",
            fontSize: "13px",
            color: isDark ? "#94a3b8" : "#6b7280",
          }}
        >
          {t('clipboardList.searching')}
        </div>
      )}

      {/* No search results */}
      {isSearchMode && searchResults.length === 0 && !isSearching && (
        <div
          data-testid="clipboard-no-results"
          style={{
            position: "absolute",
            top: "50%",
            left: "50%",
            transform: "translate(-50%, -50%)",
            textAlign: "center",
            color: isDark ? "#94a3b8" : "#6b7280",
          }}
        >
          <p style={{ fontSize: "14px" }}>{t('clipboardList.noResults')}</p>
          <p style={{ fontSize: "12px", marginTop: "4px" }}>
            {searchMode === "regex"
              ? t('clipboardList.noResultsRegexHint')
              : t('clipboardList.noResultsFuzzyHint')}
          </p>
        </div>
      )}

      {/* Empty state */}
      {totalCount === 0 && !isLoading && !isSearchMode && (
        <div
          data-testid="clipboard-empty-state"
          style={{
            position: "absolute",
            top: "50%",
            left: "50%",
            transform: "translate(-50%, -50%)",
            textAlign: "center",
            color: isDark ? "#94a3b8" : "#6b7280",
          }}
        >
          <p style={{ fontSize: "16px" }}>{t('clipboardList.clipboardEmpty')}</p>
          <p style={{ fontSize: "13px", marginTop: "4px" }}>
            {t('clipboardList.clipboardEmptyHint')}
          </p>
        </div>
      )}

    </>
  );
};
