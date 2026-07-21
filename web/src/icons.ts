/** Inline SVG icons for HUD toolbar / file controls (stroke-based, currentColor). */

const attrs = 'xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"';

function svg(paths: string): string {
  return `<svg ${attrs}>${paths}</svg>`;
}

export const icons = {
  upload: svg(
    `<path d="M12 16V6"/><path d="M8 10l4-4 4 4"/><path d="M4 18h16"/>`,
  ),
  image: svg(
    `<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="8.5" cy="10.5" r="1.5"/><path d="M21 15l-5-5-8 8"/>`,
  ),
  select: svg(
    `<path d="M4 4l7 16 2.5-6.5L20 11z"/>`,
  ),
  pan: svg(
    `<path d="M12 5v14"/><path d="M5 12h14"/><path d="M9 8l3-3 3 3"/><path d="M9 16l3 3 3-3"/><path d="M8 9l-3 3 3 3"/><path d="M16 9l3 3-3 3"/>`,
  ),
  wall: svg(
    `<path d="M4 18V8l8-4 8 4v10"/><path d="M12 4v14"/><path d="M4 12h16"/>`,
  ),
  door: svg(
    `<path d="M6 20V5a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v15"/><path d="M6 20h12"/><circle cx="14.5" cy="12" r="0.9" fill="currentColor" stroke="none"/>`,
  ),
  window: svg(
    `<rect x="4" y="5" width="16" height="14" rx="1"/><path d="M12 5v14"/><path d="M4 12h16"/>`,
  ),
  trash: svg(
    `<path d="M4 7h16"/><path d="M9 7V5h6v2"/><path d="M7 7l1 12h8l1-12"/>`,
  ),
  zoomIn: svg(
    `<circle cx="11" cy="11" r="6"/><path d="M21 21l-4.2-4.2"/><path d="M11 8v6"/><path d="M8 11h6"/>`,
  ),
  zoomOut: svg(
    `<circle cx="11" cy="11" r="6"/><path d="M21 21l-4.2-4.2"/><path d="M8 11h6"/>`,
  ),
  zoomFit: svg(
    `<path d="M4 9V5h4"/><path d="M20 9V5h-4"/><path d="M4 15v4h4"/><path d="M20 15v4h-4"/><rect x="8" y="8" width="8" height="8" rx="1"/>`,
  ),
  detect: svg(
    `<circle cx="12" cy="12" r="3"/><path d="M12 3v3"/><path d="M12 18v3"/><path d="M3 12h3"/><path d="M18 12h3"/><path d="M5.6 5.6l2.1 2.1"/><path d="M16.3 16.3l2.1 2.1"/><path d="M16.3 7.7l2.1-2.1"/><path d="M5.6 18.4l2.1-2.1"/>`,
  ),
  simplify: svg(
    `<path d="M4 7h16"/><path d="M7 12h10"/><path d="M10 17h4"/>`,
  ),
  save: svg(
    `<path d="M5 4h11l3 3v13H5z"/><path d="M8 4v5h8"/><path d="M8 20v-6h8v6"/>`,
  ),
  build: svg(
    `<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9z"/><path d="M12 12l8-4.5"/><path d="M12 12v9"/><path d="M12 12L4 7.5"/>`,
  ),
  reload: svg(
    `<path d="M4 12a8 8 0 0 1 13.5-5.8"/><path d="M20 4v6h-6"/><path d="M20 12a8 8 0 0 1-13.5 5.8"/><path d="M4 20v-6h6"/>`,
  ),
  spark: svg(
    `<path d="M12 3v4"/><path d="M12 17v4"/><path d="M3 12h4"/><path d="M17 12h4"/><path d="M12 8l1.5 3.5L17 13l-3.5 1.5L12 18l-1.5-3.5L7 13l3.5-1.5z"/>`,
  ),
} as const;

export type IconName = keyof typeof icons;

/** Icon-only toolbar button markup. */
export function iconBtn(
  opts: {
    id?: string;
    tool?: string;
    icon: IconName;
    title: string;
    extraClass?: string;
    active?: boolean;
  },
): string {
  const classes = ["tool-btn", "icon-btn", opts.extraClass].filter(Boolean).join(" ");
  const id = opts.id ? ` id="${opts.id}"` : "";
  const tool = opts.tool ? ` data-tool="${opts.tool}"` : "";
  const active = opts.active ? " active" : "";
  return `<button type="button"${id}${tool} class="${classes}${active}" title="${opts.title}" aria-label="${opts.title}">${icons[opts.icon]}</button>`;
}

/** Text button with leading icon. */
export function labeledBtn(
  opts: {
    id: string;
    icon: IconName;
    label: string;
    className?: string;
    disabled?: boolean;
    title?: string;
  },
): string {
  const cls = opts.className ?? "btn";
  const disabled = opts.disabled ? " disabled" : "";
  const title = opts.title ? ` title="${opts.title}"` : "";
  return `<button type="button" class="${cls}" id="${opts.id}"${disabled}${title}>${icons[opts.icon]}<span>${opts.label}</span></button>`;
}

/** Custom file picker: hidden native input + unified icon button + filename. */
export function filePicker(opts: {
  id: string;
  accept: string;
  multiple?: boolean;
  buttonLabel?: string;
  emptyLabel?: string;
  icon?: IconName;
}): string {
  const multi = opts.multiple ? " multiple" : "";
  const btnLabel = opts.buttonLabel ?? "选择文件";
  const empty = opts.emptyLabel ?? "未选择文件";
  const icon = icons[opts.icon ?? "upload"];
  return `
    <div class="file-field" data-file-field="${opts.id}">
      <input type="file" id="${opts.id}" class="file-input-hidden" accept="${opts.accept}"${multi} />
      <button type="button" class="btn secondary file-btn" data-file-trigger="${opts.id}" title="${btnLabel}" aria-label="${btnLabel}">
        ${icon}
        <span class="file-btn-label">${btnLabel}</span>
      </button>
      <span class="file-name" data-file-name="${opts.id}" title="${empty}">${empty}</span>
    </div>
  `;
}

/** Wire custom file pickers inside a root element. */
export function bindFilePickers(root: HTMLElement): void {
  root.querySelectorAll<HTMLButtonElement>("[data-file-trigger]").forEach((btn) => {
    const id = btn.dataset.fileTrigger!;
    const input = root.querySelector<HTMLInputElement>(`#${CSS.escape(id)}`);
    const nameEl = root.querySelector<HTMLElement>(`[data-file-name="${id}"]`);
    if (!input) return;

    btn.addEventListener("click", () => input.click());

    input.addEventListener("change", () => {
      if (!nameEl) return;
      const files = input.files;
      if (!files?.length) {
        nameEl.textContent = "未选择文件";
        nameEl.title = "未选择文件";
        nameEl.classList.remove("has-file");
        return;
      }
      const label =
        files.length === 1 ? files[0]!.name : `已选 ${files.length} 个文件`;
      nameEl.textContent = label;
      nameEl.title = Array.from(files)
        .map((f) => f.name)
        .join(", ");
      nameEl.classList.add("has-file");
    });
  });
}
