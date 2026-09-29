import { type ReactNode, useId } from "react";

import { IconChevron } from "../icons";

/** On/off setting rendered as a switch, with an optional hint under the label. */
export function Switch(props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  hint?: string;
  disabled?: boolean;
}) {
  const id = useId();
  const hintId = props.hint ? `${id}-hint` : undefined;
  return (
    <div className={`switch-row${props.disabled ? " is-disabled" : ""}`}>
      <label htmlFor={id} className="switch-row__text">
        <span className="switch-row__label">{props.label}</span>
        {props.hint && (
          <span id={hintId} className="hint">
            {props.hint}
          </span>
        )}
      </label>
      <input
        id={id}
        type="checkbox"
        role="switch"
        className="switch"
        checked={props.checked}
        disabled={props.disabled}
        aria-describedby={hintId}
        onChange={(event) => props.onChange(event.target.checked)}
      />
    </div>
  );
}

type ChoiceOption<T extends string> = { value: T; label: string; description?: string; disabled?: boolean; title?: string };

/** Mutually exclusive options as a list of radio rows. */
export function RadioList<T extends string>(props: {
  label: string;
  value: T;
  options: ChoiceOption<T>[];
  onChange: (value: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="radio-list">
      <legend className="visually-hidden">{props.label}</legend>
      {props.options.map((option) => (
        <RadioOption
          key={option.value}
          variant="row"
          name={name}
          label={option.label}
          description={option.description}
          title={option.title}
          checked={option.value === props.value}
          disabled={option.disabled}
          onSelect={() => props.onChange(option.value)}
        />
      ))}
    </fieldset>
  );
}

/**
 * One radio button, drawn as a segment of a segmented control (`.segmented`
 * container) or as a row with a visible radio dot. Options of one group share `name`, so arrow keys move
 * between them even when they are split into several visual blocks.
 */
export function RadioOption(props: {
  variant: "segment" | "row";
  name: string;
  label: string;
  checked: boolean;
  onSelect: () => void;
  description?: string;
  disabled?: boolean;
  title?: string;
}) {
  const classes = [props.variant === "segment" ? "segment" : "radio-row"];
  if (props.checked) classes.push("is-selected");
  if (props.disabled) classes.push("is-disabled");
  const input = (
    <input
      type="radio"
      className={props.variant === "row" ? "radio" : undefined}
      name={props.name}
      checked={props.checked}
      disabled={props.disabled}
      onChange={props.onSelect}
    />
  );
  if (props.variant === "segment") {
    return (
      <label className={classes.join(" ")} title={props.title}>
        {input}
        <span>{props.label}</span>
      </label>
    );
  }
  return (
    <label className={classes.join(" ")} title={props.title}>
      {input}
      <span className="radio-row__text">
        <span className="radio-row__label">{props.label}</span>
        {props.description && <span className="radio-row__description">{props.description}</span>}
      </span>
    </label>
  );
}

/** Labelled select with a themed chevron. */
export function Select<T extends string>(props: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="field__label">
        {props.label}
      </label>
      <SelectBox id={id} value={props.value} onChange={props.onChange} options={props.options} />
    </div>
  );
}

/** Native select without the platform arrow; also used outside the settings panel. */
export function SelectBox<T extends string>(props: {
  id?: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  small?: boolean;
}) {
  return (
    <div className={`select${props.small ? " select--small" : ""}`}>
      <select id={props.id} value={props.value} onChange={(event) => props.onChange(event.target.value as T)}>
        {props.options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <IconChevron className="select__chevron" size={14} />
    </div>
  );
}

/** Number input that reports `null` when emptied. */
export function NumberField(props: {
  label: string;
  value: number | null;
  onChange: (value: number | null) => void;
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="field__label">
        {props.label}
      </label>
      <input
        id={id}
        type="number"
        inputMode="numeric"
        min={props.min}
        max={props.max}
        step={props.step ?? 1}
        placeholder={props.placeholder}
        value={props.value ?? ""}
        onChange={(event) => {
          const raw = event.target.value.trim();
          if (raw === "") {
            props.onChange(null);
            return;
          }
          const parsed = Number(raw);
          if (!Number.isFinite(parsed)) return;
          const clamped = Math.min(props.max ?? parsed, Math.max(props.min ?? parsed, parsed));
          props.onChange(clamped);
        }}
      />
    </div>
  );
}

/** Labelled text input. */
export function TextField(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  maxLength?: number;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="field__label">
        {props.label}
      </label>
      <input
        id={id}
        type="text"
        value={props.value}
        maxLength={props.maxLength}
        placeholder={props.placeholder}
        spellCheck={false}
        onChange={(event) => props.onChange(event.target.value)}
      />
    </div>
  );
}

/**
 * Collapsible settings group. The header always shows the title and, when the
 * card is collapsed, a one-line summary of the current values. `disabled`
 * locks the inputs (e.g. during a job) but still lets the card be toggled.
 */
export function SettingsCard(props: {
  title: string;
  icon: ReactNode;
  summary: string;
  open: boolean;
  onToggle: (open: boolean) => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  const id = useId();
  const titleId = `${id}-title`;
  const bodyId = `${id}-body`;
  return (
    <section className={`card${props.open ? " is-open" : ""}`} aria-labelledby={titleId}>
      <h3 className="card__heading">
        <button
          type="button"
          className="card__header"
          aria-expanded={props.open}
          aria-controls={bodyId}
          onClick={() => props.onToggle(!props.open)}
        >
          <span className="card__icon">{props.icon}</span>
          <span className="card__titles">
            <span id={titleId} className="card__title">
              {props.title}
            </span>
            {!props.open && <span className="card__summary">{props.summary}</span>}
          </span>
          <IconChevron className="card__chevron" size={16} />
        </button>
      </h3>
      {props.open && (
        <fieldset id={bodyId} className="card__body" disabled={props.disabled}>
          {props.children}
        </fieldset>
      )}
    </section>
  );
}

/** Small heading that splits a card body into groups. */
export function GroupTitle(props: { children: ReactNode }) {
  return <h4 className="group-title">{props.children}</h4>;
}
