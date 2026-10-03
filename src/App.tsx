import {
  lazy,
  Suspense,
  useState,
  useEffect,
  useCallback,
  useRef,
} from "react";
import { matchesShortcut } from "@/utils/matchShortcut";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { StickedNote, StixSettings } from "@/types";
import { isMarkdownEffectivelyEmpty } from "@/utils/normalizeMarkdownForCopy";
import type { NoteWindowGeometry } from "@/utils/noteGeometry";
import { shouldHideCaptureOnBlur } from "@/utils/blurAutoHide";
import { resolveCaptureFolder } from "@/utils/folderSelection";
import { useLanguageSync, useTranslation } from "@/hooks/useTranslation";
import { resolveWindowInfo } from "@/utils/windowRouting";

const SettingsModal = lazy(() => import("./components/SettingsModal"));
const PostIt = lazy(() => import("./components/PostIt"));
const CommandPalette = lazy(() => import("./components/CommandPalette"));
const AppleNotesPicker = lazy(
  () => import("./components/AppleNotesPicker"),
);
const EditorWindow = lazy(() => import("./components/EditorWindow"));
const PENDING_UPDATE_KEY = "stix_pending_update_version";

function WindowLoading() {
  return <div className="w-full h-full bg-bg" aria-busy="true" />;
}

export default function App() {
  useLanguageSync();
  const { t } = useTranslation();
  const [currentFolder, setCurrentFolder] = useState("");
  const [stickedNote, setStickedNote] = useState<StickedNote | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const contentRef = useRef("");
  const blurIgnoreUntilRef = useRef(0);
  const pendingBlurHideRef = useRef<number | null>(null);
  const skipNextBlurHideRef = useRef(false);
  const windowInfo = resolveWindowInfo(window.location.search);

  const resolveFolder = useCallback(
    async (requestedFolder?: string, settingsFromEvent?: StixSettings) => {
      const folders = await invoke<string[]>("list_folders");
      const settings =
        settingsFromEvent ?? (await invoke<StixSettings>("get_settings"));
      return resolveCaptureFolder({
        requestedFolder: requestedFolder?.trim(),
        defaultFolder: settings.default_folder?.trim(),
        availableFolders: folders,
      });
    },
    [],
  );

  // Initialize capture window with a valid folder (requested/default/fallback).
  useEffect(() => {
    if (windowInfo.type !== "postit") return;

    let cancelled = false;

    const initialize = async () => {
      try {
        const folder = await resolveFolder();
        if (!cancelled) {
          setCurrentFolder(folder);
        }
      } catch {
        if (!cancelled) {
          setCurrentFolder("");
        }
      }
    };

    void initialize();

    return () => {
      cancelled = true;
    };
  }, [windowInfo.type, resolveFolder]);

  // Load sticked note data if this is a sticked window
  useEffect(() => {
    if (windowInfo.type !== "sticked" || !windowInfo.id) return;

    // If viewing mode, fetch content via command
    if (windowInfo.viewing) {
      const fetchViewingContent = async () => {
        try {
          const data = await invoke<{
            id: string;
            content: string;
            folder: string;
            path: string;
          }>("get_viewing_note_content", { id: windowInfo.id });
          setStickedNote({
            id: data.id,
            content: data.content,
            folder: data.folder,
            position: null,
            size: null,
            created_at: "",
            updated_at: "",
            originalPath: data.path,
          });
          setCurrentFolder(data.folder);
        } catch (error) {
          console.error("Failed to load viewing note content:", error);
          setLoadError(String(error));
        }
      };

      fetchViewingContent();
      return;
    }

    // Regular sticked note - load from storage
    invoke<StickedNote>("get_sticked_note", { id: windowInfo.id })
      .then((note) => {
        setStickedNote(note);
        setCurrentFolder(note.folder);
      })
      .catch((error) => {
        console.error("Failed to load sticked note:", error);
        setLoadError(String(error));
      });
  }, [windowInfo.type, windowInfo.id, windowInfo.viewing]);

  // Hide postit on blur only when editor is empty
  useEffect(() => {
    if (windowInfo.type !== "postit") return;

    const handleFocus = () => {
      (window as unknown as { __stixSunk?: boolean }).__stixSunk = false;
      // Use Math.max so a focus event arriving after shortcut-triggered
      // never shortens the 500ms grace period down to 300ms.
      blurIgnoreUntilRef.current = Math.max(
        blurIgnoreUntilRef.current,
        Date.now() + 300,
      );
      if (pendingBlurHideRef.current !== null) {
        window.clearTimeout(pendingBlurHideRef.current);
        pendingBlurHideRef.current = null;
      }
    };
    window.addEventListener("focus", handleFocus);

    const unlisten = listen("postit-blur", async () => {
      if (skipNextBlurHideRef.current) {
        skipNextBlurHideRef.current = false;
        return;
      }
      // Escape sinks the capture window on purpose. The blur that follows
      // must not hide it, even when the note is still empty.
      if ((window as unknown as { __stixSunk?: boolean }).__stixSunk) {
        return;
      }
      // Dictation holds the window open: when the mic is active or
      // the setup modal is mounted, a blur is expected (TCC prompt,
      // download dialog, etc.) and must never hide the postit.
      if (
        (window as unknown as { __stixDictationHoldOpen?: boolean })
          .__stixDictationHoldOpen
      ) {
        return;
      }
      if (pendingBlurHideRef.current !== null) {
        window.clearTimeout(pendingBlurHideRef.current);
      }
      pendingBlurHideRef.current = window.setTimeout(async () => {
        pendingBlurHideRef.current = null;

        const nowMs = Date.now();
        if (nowMs < blurIgnoreUntilRef.current) return;
        if (
          (window as unknown as { __stixDictationHoldOpen?: boolean })
            .__stixDictationHoldOpen
        ) {
          return;
        }

        const isFocused = await getCurrentWindow()
          .isFocused()
          .catch(() => false);
        if (isFocused) return;

        const shouldHide = shouldHideCaptureOnBlur({
          content: contentRef.current,
          nowMs,
          ignoreUntilMs: blurIgnoreUntilRef.current,
        });
        if (shouldHide) {
          await invoke("hide_window");
        }
      }, 140);
    });

    return () => {
      window.removeEventListener("focus", handleFocus);
      if (pendingBlurHideRef.current !== null) {
        window.clearTimeout(pendingBlurHideRef.current);
        pendingBlurHideRef.current = null;
      }
      unlisten.then((fn) => fn());
    };
  }, [windowInfo.type]);

  // Listen for shortcut triggers from Rust backend
  useEffect(() => {
    if (windowInfo.type !== "postit") return;

    const unlisten = listen<string>("shortcut-triggered", (event) => {
      globalThis.performance?.mark?.("stix:capture-shortcut-received");
      requestAnimationFrame(() => {
        globalThis.performance?.mark?.("stix:capture-visible");
        try {
          globalThis.performance?.measure?.(
            "stix:shortcut-to-visible",
            "stix:capture-shortcut-received",
            "stix:capture-visible",
          );
        } catch {
          // Performance marks are diagnostic only and must never affect capture.
        }
      });
      skipNextBlurHideRef.current = true;
      blurIgnoreUntilRef.current = Date.now() + 500;
      void resolveFolder(event.payload)
        .then(setCurrentFolder)
        .catch(() => {});
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [windowInfo.type, resolveFolder]);

  // Keep capture folder aligned with settings updates.
  useEffect(() => {
    if (windowInfo.type !== "postit") return;

    const unlisten = listen<StixSettings>("settings-changed", (event) => {
      void resolveFolder(undefined, event.payload)
        .then(setCurrentFolder)
        .catch(() => {});
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [windowInfo.type, resolveFolder]);

  // In-window copies of the global Ctrl+Option shortcuts.
  useEffect(() => {
    if (windowInfo.type !== "postit") return;

    const handleKeyDown = async (e: KeyboardEvent) => {
      if (matchesShortcut("Ctrl+Option+Comma", e)) {
        e.preventDefault();
        try {
          await invoke("open_settings");
        } catch (error) {
          console.error("Failed to open settings:", error);
        }
      }
      // Cmd/Ctrl+K still opens the command menu from inside the capture window.
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        try {
          await invoke("open_command_palette");
        } catch (error) {
          console.error("Failed to open command palette:", error);
        }
      }

      if (matchesShortcut("Ctrl+Option+E", e)) {
        e.preventDefault();
        try {
          await invoke("open_editor");
        } catch (error) {
          console.error("Failed to open editor:", error);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [windowInfo.type]);

  // Silent auto-update on startup (postit window only, production builds)
  useEffect(() => {
    if (windowInfo.type !== "postit") return;
    // Skip in dev: downloadAndInstall() extracts to a temp dir and spawns
    // a second Stix process (the released build), causing version conflicts.
    if (window.location.port) return;

    const runAutoUpdate = async () => {
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const settings = await invoke<StixSettings>("get_settings");
        if (settings.auto_update_enabled === false) {
          console.debug("Auto-update skipped: disabled in settings");
          return;
        }

        const update = await check({ timeout: 15_000 });
        if (!update) {
          localStorage.removeItem(PENDING_UPDATE_KEY);
          return;
        }

        const pendingVersion = localStorage.getItem(PENDING_UPDATE_KEY);
        if (pendingVersion === update.version) {
          console.log(
            `Update v${update.version} already installed and pending restart`,
          );
          return;
        }

        console.log(
          `Stix update available: v${update.currentVersion} → v${update.version}`,
        );
        await update.downloadAndInstall();
        localStorage.setItem(PENDING_UPDATE_KEY, update.version);
        console.log(
          `Update v${update.version} installed; will apply on next restart`,
        );
      } catch (error) {
        console.error("Auto-update failed:", error);
      }
    };

    void runAutoUpdate();
  }, [windowInfo.type]);

  const handleSave = useCallback(
    async (
      content: string,
      preferredFolder?: string,
      geometry?: NoteWindowGeometry,
    ): Promise<string | undefined> => {
      if (isMarkdownEffectivelyEmpty(content)) return undefined;

      const resolvedFolder = await resolveFolder(
        preferredFolder ?? currentFolder,
      );

      if (resolvedFolder !== currentFolder) {
        setCurrentFolder(resolvedFolder);
      }

      const result = await invoke<{ path: string }>("save_note", {
        folder: resolvedFolder,
        content,
        ...(geometry ? { geometry } : {}),
      });
      return result.path || undefined;
    },
    [currentFolder, resolveFolder],
  );

  const handleClose = useCallback(async () => {
    try {
      await invoke("hide_window");
    } catch (error) {
      console.error("Failed to hide window:", error);
    }
  }, []);

  const handleFolderChange = useCallback((folder: string) => {
    setCurrentFolder(folder);
  }, []);

  const handleContentChange = useCallback((content: string) => {
    contentRef.current = content;
  }, []);

  const handleOpenSettings = useCallback(async () => {
    try {
      await invoke("open_settings");
    } catch (error) {
      console.error("Failed to open settings:", error);
    }
  }, []);

  // Render settings if this is that window type
  if (windowInfo.type === "settings") {
    return (
      <Suspense fallback={<WindowLoading />}>
        <SettingsModal isOpen={true} onClose={() => {}} isWindow={true} />
      </Suspense>
    );
  }

  // Render command palette if this is that window type
  if (windowInfo.type === "command-palette") {
    return (
      <Suspense fallback={<WindowLoading />}>
        <CommandPalette />
      </Suspense>
    );
  }

  // Render full editor mode if this is that window type
  if (windowInfo.type === "editor") {
    return (
      <Suspense fallback={<WindowLoading />}>
        <EditorWindow />
      </Suspense>
    );
  }

  // Render Apple Notes picker if this is that window type
  if (windowInfo.type === "apple-notes-picker") {
    return (
      <Suspense fallback={<WindowLoading />}>
        <AppleNotesPicker />
      </Suspense>
    );
  }

  // Render sticked note if this is a sticked window
  if (windowInfo.type === "sticked") {
    if (loadError) {
      return (
        <div className="w-full h-full flex flex-col items-center justify-center bg-bg rounded-[14px] gap-3 p-6">
          <div className="text-coral text-sm font-medium">
            {t("note.failedToLoad")}
          </div>
          <div className="text-stone text-xs text-center max-w-[280px]">
            {loadError}
          </div>
          <button
            onClick={async () => {
              await getCurrentWindow().close();
            }}
            className="mt-2 px-4 py-2 text-xs bg-line hover:bg-line/70 text-ink rounded-lg transition-colors"
          >
            {t("common.close")}
          </button>
        </div>
      );
    }

    if (!stickedNote) {
      return (
        <div className="w-full h-full flex items-center justify-center bg-bg rounded-[14px]">
          <div className="text-stone text-sm">{t("common.loading")}</div>
        </div>
      );
    }

    return (
      <Suspense fallback={<WindowLoading />}>
        <PostIt
          folder={currentFolder}
          onSave={handleSave}
          onClose={handleClose}
          onFolderChange={handleFolderChange}
          isSticked={true}
          stickedId={stickedNote.id}
          initialContent={stickedNote.content}
          isViewing={windowInfo.viewing}
          originalPath={stickedNote.originalPath}
        />
      </Suspense>
    );
  }

  return (
    <Suspense fallback={<WindowLoading />}>
      <PostIt
        folder={currentFolder}
        onSave={handleSave}
        onClose={handleClose}
        onFolderChange={handleFolderChange}
        onOpenSettings={handleOpenSettings}
        onContentChange={handleContentChange}
      />
    </Suspense>
  );
}
