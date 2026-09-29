/** Accessible progress bar (global and per file). */
export function ProgressBar(props: {
  value: number;
  label: string;
  size?: "thin" | "normal";
  tone?: "accent" | "success" | "danger";
  indeterminate?: boolean;
}) {
  const percent = Math.round(Math.min(1, Math.max(0, props.value)) * 100);
  return (
    <div
      className={`progress progress--${props.size ?? "normal"} progress--${props.tone ?? "accent"}${props.indeterminate ? " progress--indeterminate" : ""}`}
      role="progressbar"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={props.indeterminate ? undefined : percent}
    >
      <div className="progress__fill" style={{ width: props.indeterminate ? undefined : `${percent}%` }} />
    </div>
  );
}
