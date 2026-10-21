import { Panel } from "../components/ui";

export function Jarvis() {
  return (
    <Panel title="jarvis · offline">
      <div className="font-mono text-sm leading-relaxed">
        <p className="text-muted">&gt; status</p>
        <p>JARVIS is not online yet. The assistant module is planned for a later sprint.</p>
        <p className="mt-3 text-muted">&gt; roadmap</p>
        <p>voice input · natural-language commands across modules · on-device context</p>
        <p className="cursor mt-3 text-muted">&gt; </p>
      </div>
    </Panel>
  );
}
