/**
 * Type-Safe Schema Node Wrapper
 *
 * Provides ergonomic, type-safe access to schema node properties
 * while maintaining the universal Node storage model.
 *
 * Mirrors the Rust SchemaNode struct for consistent type safety across the stack.
 *
 * @example
 * ```typescript
 * import { Node } from '$lib/types/node';
 * import { SchemaNode, isSchemaNode, getSchemaFields } from '$lib/types/schema-node';
 *
 * // Type guard
 * if (isSchemaNode(node)) {
 *   const fields = getSchemaFields(node);
 *   console.log(`Schema has ${fields.length} fields`);
 * }
 * ```
 */

import type { Node } from './node';

/**
 * Protection level for schema fields
 *
 * Determines whether a field can be modified or deleted by users.
 */
export type ProtectionLevel = 'core' | 'user' | 'system';

/**
 * Definition of a single field in a schema
 *
 * Supports various field types including primitives, enums, arrays, and objects.
 * Enum fields can have protected core values and user-extensible values.
 */
export interface SchemaField {
  /** Field name (must be unique within schema) */
  name: string;

  /** Field type (e.g., "string", "number", "boolean", "enum", "array", "object") */
  type: string;

  /** Protection level determining mutability */
  protection: ProtectionLevel;

  /** Protected enum values (cannot be removed) - enum fields only */
  coreValues?: string[];

  /** User-extensible enum values (can be added/removed) - enum fields only */
  userValues?: string[];

  /** Whether this field should be indexed for faster queries */
  indexed: boolean;

  /** Whether this field is required (cannot be null/undefined) */
  required?: boolean;

  /** Whether enum values can be extended by users */
  extensible?: boolean;

  /** Default value for the field */
  default?: unknown;

  /** Human-readable field description */
  description?: string;

  /** For array fields, the type of items in the array */
  itemType?: string;
}

/**
 * Schema node interface extending base Node
 *
 * Represents a schema definition with typed properties.
 * Properties are stored flat in the node matching the Rust SchemaNode pattern.
 */
export interface SchemaNode extends Node {
  nodeType: 'schema';
  properties: {
    /** Whether this is a core (built-in) schema */
    isCore?: boolean;
    /** Schema version (increments on changes) */
    version?: number;
    /** Human-readable schema description */
    description?: string;
    /** Array of field definitions */
    fields?: SchemaField[];
    [key: string]: unknown;
  };
}

/**
 * Type guard to check if a node is a schema node
 *
 * @param node - Node to check
 * @returns True if node is a schema node
 */
export function isSchemaNode(node: Node): node is SchemaNode {
  return node.nodeType === 'schema';
}

/**
 * Get whether this is a core (built-in) schema
 *
 * @param node - Schema node
 * @returns True if core schema
 */
export function isCore(node: SchemaNode): boolean {
  return node.properties.isCore ?? false;
}

/**
 * Get the schema version number
 *
 * @param node - Schema node
 * @returns Version number (defaults to 1)
 */
export function getSchemaVersion(node: SchemaNode): number {
  return node.properties.version ?? 1;
}

/**
 * Get the schema description
 *
 * @param node - Schema node
 * @returns Description string
 */
export function getSchemaDescription(node: SchemaNode): string {
  return node.properties.description ?? '';
}

/**
 * Get the schema fields
 *
 * @param node - Schema node
 * @returns Array of field definitions
 */
export function getSchemaFields(node: SchemaNode): SchemaField[] {
  return node.properties.fields ?? [];
}

/**
 * Get a specific field by name
 *
 * @param node - Schema node
 * @param fieldName - Name of the field to find
 * @returns Field definition or undefined if not found
 */
export function getSchemaField(node: SchemaNode, fieldName: string): SchemaField | undefined {
  return getSchemaFields(node).find((f) => f.name === fieldName);
}

/**
 * Get all enum values for a field (core + user values combined)
 *
 * @param field - Schema field (must be enum type)
 * @returns Combined array of all enum values
 */
export function getEnumValues(field: SchemaField): string[] {
  const coreValues = field.coreValues ?? [];
  const userValues = field.userValues ?? [];
  return [...coreValues, ...userValues];
}

/**
 * Helper namespace for schema node operations
 */
export const SchemaNodeHelpers = {
  isSchemaNode,
  isCore,
  getSchemaVersion,
  getSchemaDescription,
  getSchemaFields,
  getSchemaField,
  getEnumValues,

  /**
   * Check if a field can be deleted (user-protected only)
   */
  canDeleteField(node: SchemaNode, fieldName: string): boolean {
    const field = getSchemaField(node, fieldName);
    return field?.protection === 'user';
  },

  /**
   * Check if an enum value can be removed (user values only)
   */
  canRemoveEnumValue(field: SchemaField, value: string): boolean {
    return field.userValues?.includes(value) ?? false;
  },

  /**
   * Extract default values for all fields
   */
  extractDefaults(node: SchemaNode): Record<string, unknown> {
    const defaults: Record<string, unknown> = {};
    for (const field of getSchemaFields(node)) {
      if (field.default !== undefined) {
        defaults[field.name] = field.default;
      }
    }
    return defaults;
  }
};
