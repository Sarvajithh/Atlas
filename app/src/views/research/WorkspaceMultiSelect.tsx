import type { Workspace } from "@/ipc/types";

/**
 * Research Mode's workspace picker (§ objective "cross-document/
 * cross-workspace context"): every query below is explicitly scoped to
 * whichever workspaces the person checks here -- there is no implicit
 * "all workspaces" default, matching the backend's own contract
 * (`rag.researchQuery`/`graph.citationGraph` treat an empty selection as
 * "nothing selected", not "everything").
 */
export function WorkspaceMultiSelect({
  workspaces,
  selectedIds,
  onChange,
}: {
  workspaces: Workspace[];
  selectedIds: number[];
  onChange: (ids: number[]) => void;
}) {
  function toggle(id: number) {
    onChange(selectedIds.includes(id) ? selectedIds.filter((existing) => existing !== id) : [...selectedIds, id]);
  }

  if (workspaces.length === 0) {
    return <p className="text-sm text-muted-foreground">Link a workspace first to use Research Mode.</p>;
  }

  return (
    <fieldset className="flex flex-wrap gap-2" aria-label="Workspaces to include">
      {workspaces.map((workspace) => {
        const checked = selectedIds.includes(workspace.id);
        return (
          <label
            key={workspace.id}
            className="flex cursor-pointer items-center gap-1.5 rounded border px-2 py-1 text-sm has-[:checked]:border-primary has-[:checked]:bg-accent"
          >
            <input
              type="checkbox"
              checked={checked}
              onChange={() => toggle(workspace.id)}
              className="h-3.5 w-3.5"
            />
            {workspace.display_name}
          </label>
        );
      })}
    </fieldset>
  );
}
