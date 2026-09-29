import { useEffect, useState } from "react";

import { runningInTauri } from "../services/backend";
import { subscribeThumbnail } from "../services/thumbnails";

/** Object URL of a thumbnail, or `null` while loading / unavailable. */
export function useThumbnail(path: string | null, size = 96): string | null {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    setUrl(null);
    if (!path || !runningInTauri()) return undefined;
    return subscribeThumbnail(path, size, setUrl);
  }, [path, size]);

  return url;
}
