import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";

export interface NoteWindowGeometry {
  width: number;
  height: number;
  offset_x: number;
  offset_y: number;
  screen_x: number;
  screen_y: number;
  screen_w: number;
  screen_h: number;
}

type Point = { x: number; y: number };
type Size = { width: number; height: number };

/** Current sticker frame, relative to the monitor it sits on. */
export async function readNoteGeometry(): Promise<NoteWindowGeometry | undefined> {
  try {
    const win = getCurrentWindow() as {
      outerPosition?: () => Promise<Point>;
      innerSize?: () => Promise<Size>;
      scaleFactor?: () => Promise<number>;
    };
    if (
      !win.outerPosition ||
      !win.innerSize ||
      !win.scaleFactor ||
      typeof availableMonitors !== "function"
    ) {
      return undefined;
    }
    const [position, size, scale, monitors] = await Promise.all([
      win.outerPosition(),
      win.innerSize(),
      win.scaleFactor(),
      availableMonitors(),
    ]);
    if (!Number.isFinite(scale) || scale <= 0 || monitors.length === 0) return undefined;
    const monitor =
      monitors.find(
        (item) =>
          position.x >= item.position.x &&
          position.y >= item.position.y &&
          position.x < item.position.x + item.size.width &&
          position.y < item.position.y + item.size.height,
      ) ?? monitors[0];
    return {
      width: size.width / scale,
      height: size.height / scale,
      offset_x: position.x - monitor.position.x,
      offset_y: position.y - monitor.position.y,
      screen_x: monitor.position.x,
      screen_y: monitor.position.y,
      screen_w: monitor.size.width,
      screen_h: monitor.size.height,
    };
  } catch {
    return undefined;
  }
}
