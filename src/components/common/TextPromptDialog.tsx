import { useEffect, useRef, useState } from "react";

type Props = {
  title: string;
  label: string;
  initialValue: string;
  onCancel: () => void;
  onSubmit: (value: string) => void;
};

export function TextPromptDialog({ title, label, initialValue, onCancel, onSubmit }: Props) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);
  const cancelRef = useRef(onCancel);
  cancelRef.current = onCancel;
  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    inputRef.current?.focus();
    inputRef.current?.select();
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancelRef.current();
      }
    };
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("keydown", escape);
      previousFocus?.focus();
    };
  }, []);
  return (
    <div className="studio-prompt-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onCancel(); }}>
      <form className="studio-prompt-dialog" role="dialog" aria-modal="true" aria-labelledby="studio-prompt-title" onSubmit={(event) => { event.preventDefault(); onSubmit(value.trim()); }}>
        <h2 id="studio-prompt-title">{title}</h2>
        <label>{label}<input ref={inputRef} value={value} maxLength={120} onChange={(event) => setValue(event.target.value)} /></label>
        <footer><button type="button" className="outline" onClick={onCancel}>Cancel</button><button type="submit" disabled={!value.trim()}>Save</button></footer>
      </form>
    </div>
  );
}
