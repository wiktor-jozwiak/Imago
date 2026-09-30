import { useId, useState } from "react";

import { useT } from "../hooks/useT";
import { useImagoStore } from "../store/useImagoStore";
import type {
  AnimationPolicy,
  ConflictPolicy,
  FormatCapability,
  ImageFormat,
  OptimizeSettings,
  ResizeMode,
  TransparencyPolicy,
  WhenLarger,
} from "../types/imago";
import { FORMAT_LABELS, OUTPUT_FORMATS } from "../utils/defaults";
import {
  QUALITY_PRESETS,
  advancedSummary,
  formatSummary,
  metadataSummary,
  outputSummary,
  qualitySummary,
  resizeSummary,
} from "../utils/settingsSummary";
import { IconGear, IconImage, IconOutput, IconResize, IconSliders, IconTag } from "./icons";
import { GroupTitle, NumberField, RadioList, RadioOption, Select, SettingsCard, Switch, TextField } from "./ui/controls";

type FormatOption = "auto" | "original" | ImageFormat;
type CardId = "format" | "quality" | "resize" | "metadata" | "output";
/** Open/closed state and lock passed to every card. */
type CardState = { open: boolean; onToggle: (open: boolean) => void; disabled: boolean };

const NO_CAPABILITIES: FormatCapability[] = [];
const MB = 1024 * 1024;
const INITIALLY_OPEN: Record<CardId, boolean> = {
  format: true,
  quality: true,
  resize: false,
  metadata: false,
  output: false,
};

function formatOptionOf(settings: OptimizeSettings): FormatOption {
  return settings.format.mode === "fixed" ? settings.format.format : settings.format.mode;
}

function useSettings() {
  const settings = useImagoStore((state) => state.config.settings);
  const updateSettings = useImagoStore((state) => state.updateSettings);
  return { settings, updateSettings };
}

function FormatCard({ capabilities, card }: { capabilities: FormatCapability[]; card: CardState }) {
  const t = useT();
  const name = useId();
  const { settings, updateSettings } = useSettings();
  const value = formatOptionOf(settings);
  const capability = (format: ImageFormat) => capabilities.find((item) => item.format === format);
  const select = (next: FormatOption) =>
    updateSettings((current) => ({
      ...current,
      format: next === "auto" || next === "original" ? { mode: next } : { mode: "fixed", format: next },
    }));

  const selectedCap = value !== "auto" && value !== "original" ? capability(value) : undefined;
  const hints: string[] = [];
  if (value === "auto") hints.push(t("format.autoHint"));
  if (value === "original") hints.push(t("format.originalHint"));
  if (selectedCap?.experimental) hints.push(`${FORMAT_LABELS[selectedCap.format]}: ${t("format.experimental")}.`);
  if (selectedCap && !selectedCap.lossy) hints.push(t("format.losslessOnly"));
  if (value === "jpeg") hints.push(t("format.noAlpha"));

  return (
    <SettingsCard title={t("settings.format")} icon={<IconImage />} summary={formatSummary(settings, t)} {...card}>
      <fieldset className="format-picker">
        <legend className="visually-hidden">{t("settings.format")}</legend>
        <div className="radio-list">
          <RadioOption
            variant="row"
            name={name}
            label={t("format.modeAuto")}
            description={t("format.modeAutoHint")}
            checked={value === "auto"}
            onSelect={() => select("auto")}
          />
          <RadioOption
            variant="row"
            name={name}
            label={t("format.modeOriginal")}
            description={t("format.modeOriginalHint")}
            checked={value === "original"}
            onSelect={() => select("original")}
          />
        </div>
        <p className="format-picker__or">{t("format.specific")}</p>
        <div className="segmented" style={{ ["--columns" as string]: 3 }}>
          {OUTPUT_FORMATS.map((format) => {
            const cap = capability(format);
            const available = cap?.encode ?? false;
            return (
              <RadioOption
                key={format}
                variant="segment"
                name={name}
                label={FORMAT_LABELS[format]}
                checked={value === format}
                disabled={!available}
                title={
                  available ? undefined : `${t("format.unavailable")}${cap?.unavailableReason ? ` — ${cap.unavailableReason}` : ""}`
                }
                onSelect={() => select(format)}
              />
            );
          })}
        </div>
      </fieldset>
      {hints.map((hint) => (
        <p key={hint} className="hint">
          {hint}
        </p>
      ))}
    </SettingsCard>
  );
}

function QualityCard({ capabilities, card }: { capabilities: FormatCapability[]; card: CardState }) {
  const t = useT();
  const { settings, updateSettings } = useSettings();
  const sliderId = useId();
  const format = settings.format.mode === "fixed" ? settings.format.format : null;
  const losslessOnly = format ? capabilities.find((item) => item.format === format)?.lossy === false : false;
  const qualityDisabled = settings.compression.lossless || losslessOnly;
  const quality = settings.compression.quality;

  const setQuality = (next: number) =>
    updateSettings((current) => ({
      ...current,
      compression: { ...current.compression, quality: Math.min(100, Math.max(1, Math.round(next))) },
    }));

  return (
    <SettingsCard title={t("settings.quality")} icon={<IconSliders />} summary={qualitySummary(settings, t)} {...card}>
      <div className="segmented segmented--fit" role="group" aria-label={t("quality.presets")} style={{ ["--columns" as string]: 3 }}>
        {QUALITY_PRESETS.map((preset) => {
          const active = !qualityDisabled && preset.quality === quality;
          return (
            <button
              key={preset.key}
              type="button"
              className={`segment${active ? " is-selected" : ""}`}
              aria-pressed={active}
              disabled={qualityDisabled}
              title={`${t("settings.quality")}: ${preset.quality}`}
              onClick={() => setQuality(preset.quality)}
            >
              {t(`quality.${preset.key}`)}
            </button>
          );
        })}
      </div>
      <div className={`quality${qualityDisabled ? " is-disabled" : ""}`}>
        <label htmlFor={sliderId} className="visually-hidden">
          {t("settings.quality")}
        </label>
        <div className="quality__slider">
          <input
            id={sliderId}
            type="range"
            min={1}
            max={100}
            value={quality}
            disabled={qualityDisabled}
            style={{ ["--fill" as string]: `${quality}%` }}
            onChange={(event) => setQuality(Number(event.target.value))}
          />
          <div className="quality__scale" aria-hidden="true">
            <span>{t("quality.smaller")}</span>
            <span>{t("quality.better")}</span>
          </div>
        </div>
        <input
          type="number"
          className="quality__number"
          min={1}
          max={100}
          value={quality}
          disabled={qualityDisabled}
          aria-label={t("settings.quality")}
          onChange={(event) => {
            const parsed = Number(event.target.value);
            if (Number.isFinite(parsed) && parsed >= 1) setQuality(parsed);
          }}
        />
      </div>
      {settings.compression.lossless && <p className="hint">{t("quality.notUsed")}</p>}
      <div className="switch-list">
        <Switch
          label={t("settings.lossless")}
          hint={t("settings.losslessHint")}
          checked={settings.compression.lossless}
          onChange={(lossless) =>
            updateSettings((current) => ({ ...current, compression: { ...current.compression, lossless } }))
          }
        />
        <Switch
          label={t("settings.maxCompression")}
          hint={t("settings.maxCompressionHint")}
          checked={settings.compression.effort === "max"}
          onChange={(max) =>
            updateSettings((current) => ({
              ...current,
              compression: { ...current.compression, effort: max ? "max" : "balanced" },
            }))
          }
        />
      </div>
    </SettingsCard>
  );
}

function ResizeCard({ card }: { card: CardState }) {
  const t = useT();
  const { settings, updateSettings } = useSettings();
  const mode = settings.resize.mode;
  const setMode = (next: ResizeMode) =>
    updateSettings((current) => ({ ...current, resize: { ...current.resize, mode: next } }));

  const selectMode = (kind: ResizeMode["mode"]) => {
    if (kind === mode.mode) return;
    switch (kind) {
      case "none":
        return setMode({ mode: "none" });
      case "dimensions":
        return setMode({ mode: "dimensions", width: 1920, height: null, keepAspect: true });
      case "fit":
        return setMode({ mode: "fit", maxWidth: 2400, maxHeight: 2400 });
      case "percent":
        return setMode({ mode: "percent", percent: 50 });
    }
  };

  return (
    <SettingsCard title={t("resize.title")} icon={<IconResize />} summary={resizeSummary(settings, t)} {...card}>
      <RadioList
        label={t("resize.title")}
        value={mode.mode}
        onChange={selectMode}
        options={[
          { value: "none", label: t("resize.none") },
          { value: "fit", label: t("resize.fit") },
          { value: "dimensions", label: t("resize.dimensions") },
          { value: "percent", label: t("resize.percent") },
        ]}
      />
      {mode.mode === "dimensions" && (
        <>
          <div className="field-row">
            <NumberField label={t("resize.width")} min={1} max={65535} value={mode.width} onChange={(width) => setMode({ ...mode, width })} />
            <NumberField label={t("resize.height")} min={1} max={65535} value={mode.height} onChange={(height) => setMode({ ...mode, height })} />
          </div>
          <Switch label={t("resize.keepAspect")} checked={mode.keepAspect} onChange={(keepAspect) => setMode({ ...mode, keepAspect })} />
        </>
      )}
      {mode.mode === "fit" && (
        <>
          <div className="field-row">
            <NumberField label={t("resize.maxWidth")} min={1} max={65535} value={mode.maxWidth} onChange={(maxWidth) => setMode({ ...mode, maxWidth })} />
            <NumberField label={t("resize.maxHeight")} min={1} max={65535} value={mode.maxHeight} onChange={(maxHeight) => setMode({ ...mode, maxHeight })} />
          </div>
          <p className="hint">{t("resize.fitHint")}</p>
        </>
      )}
      {mode.mode === "percent" && (
        <NumberField
          label={t("resize.scale")}
          min={1}
          max={1000}
          value={mode.percent}
          onChange={(percent) => setMode({ mode: "percent", percent: percent ?? 100 })}
        />
      )}
      {mode.mode !== "none" && mode.mode !== "fit" && (
        <Switch
          label={t("resize.allowUpscale")}
          checked={settings.resize.allowUpscale}
          onChange={(allowUpscale) => updateSettings((current) => ({ ...current, resize: { ...current.resize, allowUpscale } }))}
        />
      )}
    </SettingsCard>
  );
}

function MetadataCard({ card }: { card: CardState }) {
  const t = useT();
  const { settings, updateSettings } = useSettings();
  const set = (patch: Partial<OptimizeSettings["metadata"]>) =>
    updateSettings((current) => ({ ...current, metadata: { ...current.metadata, ...patch } }));
  return (
    <SettingsCard title={t("metadata.title")} icon={<IconTag />} summary={metadataSummary(settings, t)} {...card}>
      <div className="switch-list">
        <Switch label={t("metadata.strip")} checked={settings.metadata.stripMetadata} onChange={(stripMetadata) => set({ stripMetadata })} />
        <Switch label={t("metadata.keepIcc")} checked={settings.metadata.keepIcc} onChange={(keepIcc) => set({ keepIcc })} />
        <Switch label={t("metadata.orient")} checked={settings.metadata.autoOrient} onChange={(autoOrient) => set({ autoOrient })} />
        <Switch
          label={t("metadata.timestamps")}
          checked={settings.metadata.preserveTimestamps}
          onChange={(preserveTimestamps) => set({ preserveTimestamps })}
        />
      </div>
    </SettingsCard>
  );
}

function OutputCard({ card }: { card: CardState }) {
  const t = useT();
  const { settings, updateSettings } = useSettings();
  const chooseOutputFolder = useImagoStore((state) => state.chooseOutputFolder);
  const output = settings.output;
  const set = (patch: Partial<OptimizeSettings["output"]>) =>
    updateSettings((current) => ({ ...current, output: { ...current.output, ...patch } }));
  const kind = output.location.kind === "file" ? "subfolder" : output.location.kind;

  return (
    <SettingsCard title={t("output.title")} icon={<IconOutput />} summary={outputSummary(settings, t)} {...card}>
      <RadioList
        label={t("output.title")}
        value={kind}
        options={[
          { value: "subfolder", label: t("output.subfolder") },
          { value: "directory", label: t("output.directory") },
          { value: "sameFolder", label: t("output.same") },
        ]}
        onChange={(next) => {
          if (next === "subfolder") set({ location: { kind: "subfolder", name: "optimized" } });
          if (next === "sameFolder") set({ location: { kind: "sameFolder" } });
          if (next === "directory") {
            if (output.location.kind === "directory") return;
            void chooseOutputFolder(t("output.chooseFolder"));
          }
        }}
      />
      {output.location.kind === "subfolder" && (
        <TextField
          label={t("output.folderName")}
          value={output.location.name}
          maxLength={64}
          onChange={(name) => set({ location: { kind: "subfolder", name: name.replace(/[\\/:*?"<>|]/g, "") || "optimized" } })}
        />
      )}
      {output.location.kind === "directory" && (
        <div className="folder-picker">
          <span className="folder-picker__path" title={output.location.path}>
            {output.location.path || t("output.noFolder")}
          </span>
          <button type="button" className="button button--small" onClick={() => void chooseOutputFolder(t("output.chooseFolder"))}>
            {t("actions.choose")}
          </button>
        </div>
      )}
      <TextField
        label={t("output.suffix")}
        value={output.suffix}
        maxLength={32}
        placeholder={t("output.suffixPlaceholder")}
        onChange={(suffix) => set({ suffix: suffix.replace(/[\\/:*?"<>|]/g, "") })}
      />
      <Select<ConflictPolicy>
        label={t("output.conflict")}
        value={output.conflict}
        onChange={(conflict) => set({ conflict, allowOverwriteOriginals: conflict === "overwrite" ? output.allowOverwriteOriginals : false })}
        options={[
          { value: "rename", label: t("conflict.rename") },
          { value: "skip", label: t("conflict.skip") },
          { value: "overwrite", label: t("conflict.overwrite") },
        ]}
      />
      <Switch label={t("output.structure")} checked={output.preserveStructure} onChange={(preserveStructure) => set({ preserveStructure })} />
    </SettingsCard>
  );
}

function AdvancedCard({ disabled }: { disabled: boolean }) {
  const t = useT();
  const { settings, updateSettings } = useSettings();
  const config = useImagoStore((state) => state.config);
  const updateConfig = useImagoStore((state) => state.updateConfig);
  const defaultWorkers = useImagoStore((state) => state.appInfo?.defaultWorkers ?? 1);
  const logDir = useImagoStore((state) => state.appInfo?.logDir);
  const openLogs = useImagoStore((state) => state.openLogs);
  const limits = settings.limits;
  const setLimits = (patch: Partial<OptimizeSettings["limits"]>) =>
    updateSettings((current) => ({ ...current, limits: { ...current.limits, ...patch } }));
  const setOutput = (patch: Partial<OptimizeSettings["output"]>) =>
    updateSettings((current) => ({ ...current, output: { ...current.output, ...patch } }));

  const workerOptions = [
    { value: "auto", label: t("advanced.workersAuto", { n: defaultWorkers }) },
    ...Array.from({ length: Math.max(defaultWorkers + 1, 16) }, (_, index) => ({ value: String(index + 1), label: String(index + 1) })),
  ];

  return (
    <SettingsCard
      title={t("advanced.title")}
      icon={<IconGear />}
      summary={advancedSummary(settings, config.scan, defaultWorkers, t)}
      open={config.ui.showAdvanced}
      disabled={disabled}
      onToggle={(open) => updateConfig((current) => ({ ...current, ui: { ...current.ui, showAdvanced: open } }))}
    >
      <GroupTitle>{t("advanced.groupPerformance")}</GroupTitle>
      <Select
        label={t("advanced.workers")}
        value={settings.workers === null ? "auto" : String(settings.workers)}
        options={workerOptions}
        onChange={(value) => updateSettings((current) => ({ ...current, workers: value === "auto" ? null : Number(value) }))}
      />
      <Switch
        label={t("advanced.fast")}
        checked={settings.compression.effort === "fast"}
        onChange={(fast) =>
          updateSettings((current) => ({ ...current, compression: { ...current.compression, effort: fast ? "fast" : "balanced" } }))
        }
      />

      <GroupTitle>{t("advanced.groupFiles")}</GroupTitle>
      <div className="switch-list">
        <Switch
          label={t("advanced.recursive")}
          checked={config.scan.recursive}
          onChange={(recursive) => updateConfig((current) => ({ ...current, scan: { ...current.scan, recursive } }))}
        />
        <Switch
          label={t("advanced.hidden")}
          checked={config.scan.includeHidden}
          onChange={(includeHidden) => updateConfig((current) => ({ ...current, scan: { ...current.scan, includeHidden } }))}
        />
      </div>

      <GroupTitle>{t("advanced.groupBehavior")}</GroupTitle>
      <Select<WhenLarger>
        label={t("advanced.whenLarger")}
        value={settings.output.whenLarger}
        onChange={(whenLarger) => setOutput({ whenLarger })}
        options={[
          { value: "keepOriginal", label: t("whenLarger.keep") },
          { value: "writeAnyway", label: t("whenLarger.write") },
        ]}
      />
      <Select<AnimationPolicy>
        label={t("advanced.animation")}
        value={settings.animation}
        onChange={(animation) => updateSettings((current) => ({ ...current, animation }))}
        options={[
          { value: "preserve", label: t("animation.preserve") },
          { value: "firstFrame", label: t("animation.firstFrame") },
        ]}
      />
      <Select<TransparencyPolicy>
        label={t("advanced.transparency")}
        value={settings.transparency}
        onChange={(transparency) => updateSettings((current) => ({ ...current, transparency }))}
        options={[
          { value: "preserve", label: t("transparency.preserve") },
          { value: "flattenWhite", label: t("transparency.flatten") },
        ]}
      />

      <GroupTitle>{t("advanced.limits")}</GroupTitle>
      <div className="field-row">
        <NumberField
          label={t("limits.fileSize")}
          min={1}
          max={16384}
          value={Math.round(limits.maxFileBytes / MB)}
          onChange={(value) => setLimits({ maxFileBytes: (value ?? 512) * MB })}
        />
        <NumberField
          label={t("limits.megapixels")}
          min={1}
          max={1000}
          value={Math.round(limits.maxPixels / 1_000_000)}
          onChange={(value) => setLimits({ maxPixels: (value ?? 150) * 1_000_000 })}
        />
      </div>
      <NumberField
        label={t("limits.memory")}
        min={64}
        max={262144}
        value={limits.memoryBudgetBytes === null ? null : Math.round(limits.memoryBudgetBytes / MB)}
        placeholder="2048"
        onChange={(value) => setLimits({ memoryBudgetBytes: value === null ? null : value * MB })}
      />
      <Switch
        label={t("advanced.largeImages")}
        hint={t("advanced.largeImagesHint")}
        checked={settings.largeImages === "stream"}
        onChange={(stream) => updateSettings((current) => ({ ...current, largeImages: stream ? "stream" : "reject" }))}
      />

      <GroupTitle>{t("advanced.groupSafety")}</GroupTitle>
      <div className="switch-list">
        <Switch label={t("advanced.fsync")} checked={settings.output.fsync} onChange={(fsync) => setOutput({ fsync })} />
        <Switch
          label={t("advanced.allowOriginals")}
          hint={t("advanced.allowOriginalsHint")}
          disabled={settings.output.conflict !== "overwrite"}
          checked={settings.output.allowOverwriteOriginals}
          onChange={(allowOverwriteOriginals) => setOutput({ allowOverwriteOriginals })}
        />
      </div>
      {logDir && (
        <div className="advanced-logs">
          <span>{t("advanced.logs")}</span>
          <button type="button" className="button button--small" onClick={() => void openLogs()}>
            {t("actions.openLogs")}
          </button>
        </div>
      )}
    </SettingsCard>
  );
}

/** Right-hand settings column: collapsible cards, the most used ones open. */
export function SettingsPanel() {
  const t = useT();
  const capabilities = useImagoStore((state) => state.appInfo?.capabilities ?? NO_CAPABILITIES);
  const running = useImagoStore((state) => state.job.running);
  const [open, setOpen] = useState(INITIALLY_OPEN);
  const card = (id: CardId): CardState => ({
    open: open[id],
    onToggle: (next) => setOpen((current) => ({ ...current, [id]: next })),
    disabled: running,
  });

  return (
    <aside className={`settings-panel${running ? " is-locked" : ""}`} aria-label={t("settings.title")}>
      <h2 className="settings-panel__title">{t("settings.title")}</h2>
      <div className="settings-panel__content">
        <FormatCard capabilities={capabilities} card={card("format")} />
        <QualityCard capabilities={capabilities} card={card("quality")} />
        <ResizeCard card={card("resize")} />
        <MetadataCard card={card("metadata")} />
        <OutputCard card={card("output")} />
        <AdvancedCard disabled={running} />
      </div>
    </aside>
  );
}
