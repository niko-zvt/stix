import { describe, expect, it } from "vitest";
import { SETTINGS_SOCIAL_LINKS } from "./settingsSocialLinks";
import { en } from "@/i18n/locales/en";
import { zhCN } from "@/i18n/locales/zh-CN";

describe("SETTINGS_SOCIAL_LINKS", () => {
  it("opens Stix issues from Help", () => {
    const byId = new Map(SETTINGS_SOCIAL_LINKS.map((link) => [link.id, link]));

    expect(byId.get("help")?.href).toBe("https://github.com/niko-zvt/stix/issues");
    expect(byId.size).toBe(1);
  });

  it("defines accessibility labels for each entry", () => {
    for (const link of SETTINGS_SOCIAL_LINKS) {
      expect(link.ariaLabelKey.trim().length).toBeGreaterThan(0);
      expect(link.labelKey.trim().length).toBeGreaterThan(0);
    }
  });

  it("announces the support address in both locales", () => {
    for (const locale of [en, zhCN]) {
      expect(locale["social.helpTitle"].length).toBeGreaterThan(0);
    }
  });
});
