import type { ModuleProps } from "../App";
import { ModuleGrid } from "../components/ModuleGrid";

/** The phone's Modules tab (Paper "Mobile · Modules"): everything the tab bar doesn't show. */
export function Modules({ navigate }: ModuleProps) {
  return <ModuleGrid navigate={navigate} exclude={["modules"]} />;
}
