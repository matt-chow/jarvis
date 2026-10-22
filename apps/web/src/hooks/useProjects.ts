import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import type { NewProject, Project, ProjectPatch } from "../lib/projects";
import { applyProjectLocally, clientId } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface ProjectsState {
  projects?: Project[];
  loading: boolean;
  /** Showing the copy saved on this device. Edits still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (p: NewProject) => Promise<Project | undefined>;
  update: (id: string, patch: ProjectPatch) => Promise<Project | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.projects(true);

/** All projects (active and archived); editable offline (DECISIONS D-017). */
export function useProjects(online: boolean): ProjectsState {
  const list = useCachedList<Project>("projects:last", "project", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (p: NewProject) => {
      const project = { ...p, id: clientId() };
      return runOrQueue({
        list,
        noun: "project",
        op: { kind: "project.create", project },
        server: () => api.createProject(project),
        fromServer: (cur, created) => [...cur, created],
        applyLocal: applyProjectLocally,
        local: (cur) => cur.find((x) => x.id === project.id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: ProjectPatch) =>
      runOrQueue({
        list,
        noun: "project",
        op: { kind: "project.update", id, patch },
        server: () => api.updateProject(id, patch),
        fromServer: (cur, u) => cur.map((p) => (p.id === id ? u : p)),
        applyLocal: applyProjectLocally,
        local: (cur) => cur.find((p) => p.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "project",
        op: { kind: "project.delete", id },
        server: async () => {
          await api.deleteProject(id);
          return true;
        },
        fromServer: (cur) => cur.filter((p) => p.id !== id),
        applyLocal: applyProjectLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  return {
    projects: list.items,
    loading: list.loading,
    stale: list.stale,
    error: list.error,
    reload: list.reload,
    create,
    update,
    remove,
  };
}
