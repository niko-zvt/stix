import { describe, expect, it } from "vitest";
import { shouldSinkOnEscape } from "./captureEscape";

const base = {
  defaultPrevented: false,
  inLinkPopover: false,
  isCopyMenuOpen: false,
  isAutocompleteOpen: false,
  showPicker: false,
  isSaving: false,
  isPinning: false,
};

describe("shouldSinkOnEscape", () => {
  it("does not sink when escape was already handled by editor", () => {
    expect(shouldSinkOnEscape({ ...base, defaultPrevented: true })).toBe(false);
  });

  it("does not sink while slash autocomplete is open", () => {
    expect(shouldSinkOnEscape({ ...base, isAutocompleteOpen: true })).toBe(false);
  });

  it("does not sink when folder picker is visible", () => {
    expect(shouldSinkOnEscape({ ...base, showPicker: true })).toBe(false);
  });

  it("does not sink when copy menu is open", () => {
    expect(shouldSinkOnEscape({ ...base, isCopyMenuOpen: true })).toBe(false);
  });

  it("does not sink while saving or pinning is in progress", () => {
    expect(shouldSinkOnEscape({ ...base, isSaving: true })).toBe(false);
    expect(shouldSinkOnEscape({ ...base, isPinning: true })).toBe(false);
  });

  it("does not sink when in link popover", () => {
    expect(shouldSinkOnEscape({ ...base, inLinkPopover: true })).toBe(false);
  });

  it("sinks only when no overlay or transient state is active", () => {
    expect(shouldSinkOnEscape(base)).toBe(true);
  });
});
