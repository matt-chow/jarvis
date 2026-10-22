/** File manager types and path helpers. */

export interface FileEntry {
  id: string;
  folder: string;
  name: string;
  size: number;
  content_type: string;
  created_at: number;
  updated_at: number;
}

export interface Listing {
  folder: string;
  folders: string[];
  files: FileEntry[];
}

export interface Usage {
  used: number;
  quota: number;
  max_file: number;
}

/** "a/b" + "c" → "a/b/c"; "" + "c" → "c". */
export function joinFolder(parent: string, child: string): string {
  const clean = child.trim().replace(/^\/+|\/+$/g, "");
  return parent ? `${parent}/${clean}` : clean;
}

export function parentFolder(folder: string): string {
  const i = folder.lastIndexOf("/");
  return i === -1 ? "" : folder.slice(0, i);
}

/** Breadcrumb trail for a folder path, starting at the root. */
export function breadcrumbs(folder: string): { name: string; path: string }[] {
  const crumbs = [{ name: "files", path: "" }];
  let path = "";
  for (const part of folder.split("/").filter(Boolean)) {
    path = joinFolder(path, part);
    crumbs.push({ name: part, path });
  }
  return crumbs;
}

export function filesRoute(folder: string): string {
  return folder ? `#/files?folder=${encodeURIComponent(folder)}` : "#/files";
}
