/**
 * ClipPacket Type Definitions
 *
 * Unified data format for plugin communication
 */

export type ClipPacketType =
  | "text"
  | "image"
  | "file"
  | "rich-text"
  | `custom:${string}`;

export interface PacketMetadata {
  sourceApp?: string;
  windowTitle?: string;
  createdAt: string;
  updatedAt: string;
  isSensitive: boolean;
  tags: string[];
  contentHash?: string;
}

export type ProcessStatus =
  | "success"
  | "modified"
  | "filtered"
  | { failed: string };

export interface PipelineTrace {
  plugins: string[];
  timestamps: string[];
  statuses: ProcessStatus[];
}

export interface ClipPacket {
  /** Unique identifier */
  id: string;

  /** Data type */
  type: ClipPacketType;

  /** Actual data content */
  data: string;

  /** MIME type */
  mimeType: string;

  /** Metadata */
  metadata: PacketMetadata;

  /** Processing pipeline trace */
  pipeline: PipelineTrace;

  /** Plugin custom data */
  extensions?: Record<string, unknown>;
}

/**
 * Helper functions for ClipPacket
 */
export function isTextPacket(packet: ClipPacket): boolean {
  return packet.type === "text" || packet.type === "rich-text";
}

export function isImagePacket(packet: ClipPacket): boolean {
  return packet.type === "image";
}

export function isCustomPacket(packet: ClipPacket): boolean {
  return packet.type.startsWith("custom:");
}

export function getCustomType(packet: ClipPacket): string | null {
  if (isCustomPacket(packet)) {
    return packet.type.replace("custom:", "");
  }
  return null;
}
