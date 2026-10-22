const mediaMimeTypes: Record<"audio" | "video", Record<string, string>> = {
  audio: {
    mp3: "audio/mpeg",
    wav: "audio/wav",
    m4a: "audio/mp4",
    aac: "audio/aac",
    flac: "audio/flac",
    ogg: "audio/ogg",
    opus: "audio/ogg; codecs=opus",
  },
  video: {
    mp4: "video/mp4",
    webm: "video/webm",
    mov: "video/quicktime",
  },
};
const allMediaMimeTypes = { ...mediaMimeTypes.audio, ...mediaMimeTypes.video };

export function mediaMimeType(extension: string, kind?: "audio" | "video") {
  const mimeTypes = kind ? mediaMimeTypes[kind] : allMediaMimeTypes;
  return mimeTypes[extension] ?? "application/octet-stream";
}
