import { open } from "@tauri-apps/plugin-shell";
import { SETTINGS_SOCIAL_LINKS } from "@/utils/settingsSocialLinks";
import { useTranslation } from "@/hooks/useTranslation";

interface SettingsFooterLinksProps {
  appVersion: string;
}

export default function SettingsFooterLinks({ appVersion }: SettingsFooterLinksProps) {
  const { t } = useTranslation();
  const handleOpen = async (href: string) => {
    try {
      await open(href);
    } catch (error) {
      console.error("Failed to open settings link:", error);
    }
  };

  return (
    <div className="flex items-center gap-3 min-w-0">
      {appVersion && <span className="text-[11px] text-stone">v{appVersion}</span>}
      <div className="flex items-center gap-1.5">
        {SETTINGS_SOCIAL_LINKS.map((link) => (
          <button
            key={link.id}
            type="button"
            onClick={() => {
              void handleOpen(link.href);
            }}
            className="inline-flex items-center rounded-md px-2 py-1 text-[11px] text-stone transition-colors hover:bg-line hover:text-coral focus:outline-none focus-visible:ring-1 focus-visible:ring-coral/60"
            title={t(link.labelKey)}
            aria-label={t(link.ariaLabelKey)}
          >
            <span className="text-[11px]">{t(link.labelKey)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
