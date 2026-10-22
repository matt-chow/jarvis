import { useCallback } from "react";
import { api } from "../lib/api";
import type { NewProject, Project, ProjectPatch } from "../lib/projects";
import { useCachedList } from "./useCachedList";

export interface ProjectsState {
  projects?: Project[];
  loading: boolean;
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (p: NewProject) => Promise<Project | undefined>;
  update: (id: string, patch: ProjectPatch) => Promise<Project | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.projects(true);

/** All projects (active and archived), cached for offline viewing. */
export function useProjects(online: boolean): ProjectsState {
  const list = useCachedList<Project>("projects:last", "project", fetchAll, online);
  const { mutate } = list;

  const create = useCallback((p: NewProject) => mutate(() => api.createProject(p), (cur, created) => [...cur, created]), [mutate]);

  const update = useCallback(
    (id: string, patch: ProjectPatch) =>
      mutate(() => api.updateProject(id, patch), (cur, u) => cur.map((p) => (p.id === id ? u : p))),
    [mutate],
  );

  const remove = useCallback(
    async (id: string) =>
      (await mutate(
        async () => {
          await api.deleteProject(id);
          return true;
        },
        (cur) => cur.filter((p) => p.id !== id),
      )) ?? false,
    [mutate],
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
