import { ActivityRail } from "@/components/ActivityRail";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/panels/StatusBar";
import { AssistantPanel } from "@/panels/AssistantPanel";
import { WorkspaceHome } from "@/views/WorkspaceHome";

/**
 * Shell Layout (§8.1). Structural scaffolding only -- this milestone does
 * not implement visual design, navigation, or view switching (§9). The
 * Main Document Area defaults to Workspace Home per §8.3 ("never open to a
 * blank chat box").
 */
export function App() {
  return (
    <div className="flex h-screen w-screen flex-col">
      <div className="flex flex-1 overflow-hidden">
        <ActivityRail />
        <Sidebar />
        <main className="flex-1 overflow-auto">
          <WorkspaceHome />
        </main>
        <AssistantPanel />
      </div>
      <StatusBar />
    </div>
  );
}
