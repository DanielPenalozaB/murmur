import type { FolderNode } from "../../lib/ipc";

/** Flattens the tree into `path` strings in display order, for selects. */
export function flattenFolders(nodes: FolderNode[]): string[] {
  return nodes.flatMap((n) => [n.path, ...flattenFolders(n.children)]);
}
