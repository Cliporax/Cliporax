export interface Permission {
  permission: string;
  reason?: string;
  required?: boolean;
  id?: string;
  name?: string;
  riskLevel?: "low" | "medium" | "high" | "critical";
  category?: string;
  description?: string;
}

export interface PermissionRequest {
  permission: string;
  reason: string;
  required?: boolean;
}

export interface PermissionDefinition {
  permission: string;
  riskLevel: "low" | "medium" | "high" | "critical";
  description: string;
  children?: PermissionDefinition[];
}

export function getRiskLevelColor(level: string): string {
  switch (level) {
    case "low":
      return "text-green-600";
    case "medium":
      return "text-yellow-600";
    case "high":
      return "text-orange-600";
    case "critical":
      return "text-red-600";
    default:
      return "text-gray-600";
  }
}

export function getRiskLevelLabel(level: string): string {
  switch (level) {
    case "low":
      return "Low risk";
    case "medium":
      return "Medium risk";
    case "high":
      return "High risk";
    case "critical":
      return "Critical risk";
    default:
      return "Unknown";
  }
}

export function getCategoryLabel(category: string): string {
  switch (category) {
    case "data":
      return "Data access";
    case "clipboard":
      return "Clipboard";
    case "network":
      return "Network";
    case "file":
      return "File system";
    case "extension":
      return "Extension point";
    default:
      return "Other";
  }
}

export const PERMISSION_DEFINITIONS: PermissionDefinition[] = [
  {
    permission: "data:read",
    riskLevel: "low",
    description: "Read clipboard item data",
  },
  {
    permission: "data:write",
    riskLevel: "medium",
    description: "Modify clipboard item data",
  },
  {
    permission: "clipboard:read",
    riskLevel: "medium",
    description: "Read the system clipboard",
  },
  {
    permission: "clipboard:write",
    riskLevel: "high",
    description: "Write to the system clipboard",
  },
  {
    permission: "network:request",
    riskLevel: "high",
    description: "Make network requests",
  },
  {
    permission: "file:read",
    riskLevel: "high",
    description: "Read local files",
  },
  {
    permission: "file:write",
    riskLevel: "critical",
    description: "Write local files",
  },
  {
    permission: "extension:card",
    riskLevel: "low",
    description: "Add card extension buttons",
  },
  {
    permission: "extension:settings",
    riskLevel: "low",
    description: "Add settings panel extensions",
  },
];

export const PERMISSION_GROUPS = {
  reader: ["data:read", "extension:card"],
  writer: ["data:read", "data:write", "clipboard:read", "clipboard:write"],
  network: ["network:request", "data:read"],
  dangerous: [
    "file:read",
    "file:write",
    "network:request",
    "clipboard:read",
    "clipboard:write",
  ],
};
