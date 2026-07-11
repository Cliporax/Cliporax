import { useEffect, useRef } from "react";

export interface ComboboxOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface ComboboxOptions {
  options: ComboboxOption[];
  value?: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  searchable?: boolean;
  theme?: "light" | "dark";
  ariaLabel?: string;
}

export interface ComboboxInstance {
  element: HTMLDivElement;
  setValue: (value: string | undefined) => void;
  setOptions: (options: ComboboxOption[]) => void;
  setDisabled: (disabled: boolean) => void;
  destroy: () => void;
}

const colors = (theme: "light" | "dark") => ({
  surface: theme === "dark" ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.8)",
  surfaceSelected: theme === "dark" ? "rgba(59,130,246,0.2)" : "rgba(59,130,246,0.1)",
  border: theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.08)",
  text: theme === "dark" ? "#e2e8f0" : "#27272a",
  muted: theme === "dark" ? "#94a3b8" : "#71717a",
  accent: theme === "dark" ? "#60a5fa" : "#2563eb",
  shadow: theme === "dark" ? "0 12px 28px rgba(0,0,0,0.35)" : "0 12px 28px rgba(15,23,42,0.14)",
});

/**
 * Creates the shared, non-native combobox used by the host UI and DOM plugins.
 * Plugins should keep the returned instance and call destroy() when their view unmounts.
 */
export function createCombobox(initialOptions: ComboboxOptions): ComboboxInstance {
  let options = initialOptions.options;
  let value = initialOptions.value;
  let disabled = initialOptions.disabled ?? false;
  let isOpen = false;
  let query = "";
  let activeIndex = -1;
  const theme = initialOptions.theme ?? "dark";
  const palette = colors(theme);
  const id = `cliporax-combobox-${Math.random().toString(36).slice(2, 10)}`;

  const element = document.createElement("div");
  element.style.cssText = "position:relative;width:100%;min-width:0;font-family:inherit;";

  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.setAttribute("role", "combobox");
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-controls", id);
  trigger.setAttribute("aria-expanded", "false");
  trigger.setAttribute("aria-label", initialOptions.ariaLabel ?? initialOptions.placeholder ?? "Select an option");
  trigger.style.cssText = [
    "display:flex", "align-items:center", "justify-content:space-between", "gap:8px",
    "width:100%", "min-height:36px", "padding:6px 10px", "border-radius:8px",
    `border:1px solid ${palette.border}`, `background:${palette.surface}`, `color:${palette.text}`,
    "font:inherit", "font-size:14px", "line-height:20px", "text-align:left", "cursor:pointer",
    "outline:none", "transition:background-color 150ms ease,border-color 150ms ease,box-shadow 150ms ease",
  ].join(";");
  const triggerLabel = document.createElement("span");
  triggerLabel.style.cssText = "min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;";
  const chevron = document.createElement("span");
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = "⌄";
  chevron.style.cssText = `flex:none;color:${palette.muted};font-size:18px;line-height:1;transition:transform 150ms ease;`;
  trigger.append(triggerLabel, chevron);

  const popup = document.createElement("div");
  popup.id = id;
  popup.setAttribute("role", "listbox");
  popup.style.cssText = [
    "position:absolute", "z-index:1000", "top:calc(100% + 4px)", "left:0", "width:100%",
    "max-height:240px", "overflow:auto", "padding:4px", "border-radius:8px",
    `border:1px solid ${palette.border}`, `background:${theme === "dark" ? "#1e293b" : "#ffffff"}`,
    `box-shadow:${palette.shadow}`, "display:none",
  ].join(";");
  const search = document.createElement("input");
  search.type = "text";
  search.placeholder = "Search options";
  search.setAttribute("aria-label", "Search options");
  search.style.cssText = [
    "width:100%", "height:36px", "margin-bottom:4px", "padding:0 8px", "border-radius:6px",
    `border:1px solid ${palette.border}`, `background:${palette.surface}`, `color:${palette.text}`,
    "font:inherit", "font-size:13px", "outline:none",
  ].join(";");
  const list = document.createElement("div");
  popup.append(list);
  element.append(trigger, popup);

  const visibleOptions = () => options.filter((option) =>
    option.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  );
  const selectedOption = () => options.find((option) => option.value === value);

  const updateTrigger = () => {
    const selected = selectedOption();
    triggerLabel.textContent = selected?.label ?? initialOptions.placeholder ?? "Select an option";
    triggerLabel.style.color = selected ? palette.text : palette.muted;
    trigger.disabled = disabled;
    trigger.style.opacity = disabled ? "0.5" : "1";
    trigger.style.cursor = disabled ? "not-allowed" : "pointer";
  };

  const close = () => {
    if (!isOpen) return;
    isOpen = false;
    query = "";
    activeIndex = -1;
    popup.style.display = "none";
    chevron.style.transform = "rotate(0deg)";
    trigger.setAttribute("aria-expanded", "false");
    document.removeEventListener("pointerdown", onOutsidePointerDown);
  };

  const select = (nextValue: string) => {
    const option = options.find((candidate) => candidate.value === nextValue);
    if (!option || option.disabled) return;
    value = nextValue;
    updateTrigger();
    initialOptions.onChange(nextValue);
    close();
    trigger.focus();
  };

  const moveActiveIndex = (direction: 1 | -1) => {
    const visible = visibleOptions();
    for (let offset = 1; offset <= visible.length; offset += 1) {
      const index = (activeIndex + direction * offset + visible.length) % visible.length;
      if (!visible[index].disabled) return index;
    }
    return -1;
  };

  const renderList = () => {
    list.replaceChildren();
    const visible = visibleOptions();
    if (visible.length === 0) {
      const empty = document.createElement("div");
      empty.textContent = "No options found";
      empty.style.cssText = `padding:8px;color:${palette.muted};font-size:13px;`;
      list.append(empty);
      return;
    }
    visible.forEach((option, index) => {
      const optionButton = document.createElement("button");
      optionButton.type = "button";
      optionButton.setAttribute("role", "option");
      optionButton.setAttribute("aria-selected", String(option.value === value));
      optionButton.disabled = Boolean(option.disabled);
      optionButton.textContent = option.label;
      optionButton.style.cssText = [
        "display:flex", "align-items:center", "width:100%", "min-height:36px", "padding:6px 8px",
        "border:0", "border-radius:6px", "font:inherit", "font-size:13px", "text-align:left",
        `background:${option.value === value ? palette.surfaceSelected : "transparent"}`,
        `color:${option.disabled ? palette.muted : palette.text}`,
        `cursor:${option.disabled ? "not-allowed" : "pointer"}`,
        `opacity:${option.disabled ? "0.5" : "1"}`,
      ].join(";");
      if (index === activeIndex) optionButton.style.outline = `2px solid ${palette.accent}`;
      optionButton.addEventListener("mouseenter", () => { activeIndex = index; renderList(); });
      optionButton.addEventListener("click", () => select(option.value));
      list.append(optionButton);
    });
  };

  const open = () => {
    if (disabled || isOpen) return;
    isOpen = true;
    activeIndex = visibleOptions().findIndex((option) => option.value === value && !option.disabled);
    popup.style.display = "block";
    chevron.style.transform = "rotate(180deg)";
    trigger.setAttribute("aria-expanded", "true");
    if (initialOptions.searchable) {
      popup.prepend(search);
      search.focus();
    }
    renderList();
    document.addEventListener("pointerdown", onOutsidePointerDown);
  };

  const onOutsidePointerDown = (event: PointerEvent) => {
    if (!element.contains(event.target as Node)) close();
  };

  trigger.addEventListener("click", () => (isOpen ? close() : open()));
  trigger.addEventListener("focus", () => {
    trigger.style.borderColor = palette.accent;
    trigger.style.boxShadow = `0 0 0 2px ${theme === "dark" ? "rgba(96,165,250,0.3)" : "rgba(37,99,235,0.2)"}`;
  });
  trigger.addEventListener("blur", () => {
    trigger.style.borderColor = palette.border;
    trigger.style.boxShadow = "none";
  });
  trigger.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      open();
      activeIndex = moveActiveIndex(event.key === "ArrowDown" ? 1 : -1);
      renderList();
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (isOpen && activeIndex >= 0) select(visibleOptions()[activeIndex].value);
      else open();
    }
    if (event.key === "Escape") close();
  });
  search.addEventListener("input", () => { query = search.value; activeIndex = -1; renderList(); });
  search.addEventListener("keydown", (event) => {
    if (event.key === "Escape") { event.preventDefault(); close(); trigger.focus(); }
  });
  updateTrigger();

  return {
    element,
    setValue: (nextValue) => { value = nextValue; updateTrigger(); renderList(); },
    setOptions: (nextOptions) => { options = nextOptions; updateTrigger(); renderList(); },
    setDisabled: (nextDisabled) => { disabled = nextDisabled; updateTrigger(); },
    destroy: () => { close(); element.replaceChildren(); },
  };
}

export interface ComboboxProps extends ComboboxOptions {
  className?: string;
}

/** React wrapper around the DOM implementation so host and plugin UI stay identical. */
export function Combobox({ className, ...options }: ComboboxProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const instanceRef = useRef<ComboboxInstance | null>(null);

  useEffect(() => {
    const instance = createCombobox(options);
    instanceRef.current = instance;
    hostRef.current?.append(instance.element);
    return () => instance.destroy();
    // The instance receives controlled updates below; recreate only when its callback/theme contract changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options.onChange, options.theme, options.placeholder, options.searchable, options.ariaLabel]);

  useEffect(() => {
    instanceRef.current?.setOptions(options.options);
  }, [options.options]);
  useEffect(() => {
    instanceRef.current?.setValue(options.value);
  }, [options.value]);
  useEffect(() => {
    instanceRef.current?.setDisabled(options.disabled ?? false);
  }, [options.disabled]);

  return <div ref={hostRef} className={className} />;
}
