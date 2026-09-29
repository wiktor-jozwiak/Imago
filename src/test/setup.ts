import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Unmount rendered trees between tests so queries never see stale DOM.
afterEach(() => {
  cleanup();
});
