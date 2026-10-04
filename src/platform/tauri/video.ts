import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  VideoBounds,
  NativeVideoControlLabels,
  VideoControl,
  MpvVideoEvent
} from "./contracts/video";

export async function beginVideoOpen(): Promise<number> {
  return await invoke<number>("begin_video_open");
}

export async function cancelVideoOpen(requestId: number): Promise<void> {
  await invoke("cancel_video_open", { requestId });
}

export async function setVideoControlLabels(
  sessionId: number,
  controlLabels: NativeVideoControlLabels
): Promise<void> {
  await invoke("set_video_control_labels", { sessionId, controlLabels });
}

export async function openVideo(
  assetId: number,
  requestId: number,
  bounds: VideoBounds,
  controlLabels: NativeVideoControlLabels,
  onEvent: (event: MpvVideoEvent) => void
): Promise<number> {
  const channel = new Channel<MpvVideoEvent>();
  channel.onmessage = onEvent;
  return await invoke<number>("open_video", {
    assetId,
    requestId,
    bounds,
    controlLabels,
    onEvent: channel
  });
}

export async function setVideoBounds(sessionId: number, bounds: VideoBounds): Promise<void> {
  await invoke("set_video_bounds", { sessionId, bounds });
}

export async function controlVideo(sessionId: number, command: VideoControl): Promise<void> {
  await invoke("control_video", { sessionId, command });
}

export async function closeVideo(sessionId: number): Promise<void> {
  await invoke("close_video", { sessionId });
}
