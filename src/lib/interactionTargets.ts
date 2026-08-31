function hasClosestMatch(target: EventTarget | null, selector: string) {
  const closest = (target as { closest?: (value: string) => Element | null } | null)?.closest;
  return Boolean(closest?.call(target, selector));
}

const TEXT_ENTRY_SELECTOR = "input,textarea,[contenteditable='true'],[role='textbox']";

const INTERACTIVE_SELECTOR = [
  "button",
  "select",
  "a[href]",
  TEXT_ENTRY_SELECTOR,
  "[role='button']",
  "[role='checkbox']",
  "[role='combobox']",
  "[role='radio']",
  "[role='slider']",
  "[role='spinbutton']",
  "[role='switch']",
].join(",");

export function isTextEntryTarget(target: EventTarget | null) {
  return hasClosestMatch(target, TEXT_ENTRY_SELECTOR);
}

export function isInteractiveKeyboardTarget(target: EventTarget | null) {
  return hasClosestMatch(target, INTERACTIVE_SELECTOR);
}
