import { useState, type FormEvent } from "react";
import { Button, inputClass } from "./ui";

/** Quick capture into the inbox. Enter submits; Shift+Enter adds a line. */
export function CaptureBox({
  onCapture,
  disabled,
  placeholder = "Capture a thought, task, or idea…",
}: {
  onCapture: (text: string) => Promise<unknown>;
  disabled?: boolean;
  placeholder?: string;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [flash, setFlash] = useState(false);

  const submit = async (e?: FormEvent) => {
    e?.preventDefault();
    if (!text.trim()) return;
    setBusy(true);
    const ok = await onCapture(text);
    setBusy(false);
    if (ok) {
      setText("");
      setFlash(true);
      setTimeout(() => setFlash(false), 1500);
    }
  };

  return (
    <form onSubmit={(e) => void submit(e)} className="flex items-start gap-2">
      <textarea
        value={text}
        rows={1}
        maxLength={2000}
        disabled={disabled || busy}
        aria-label="Capture to inbox"
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void submit();
          }
        }}
        className={`${inputClass} min-w-0 flex-1 resize-none py-2 leading-snug`}
      />
      <Button type="submit" variant="primary" disabled={disabled || busy || !text.trim()}>
        {flash ? "captured ✓" : "capture"}
      </Button>
    </form>
  );
}
