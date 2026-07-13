/**
 * Configuration loader for the RuView MCP server.
 *
 * All settings can be overridden via environment variables.  No config file is
 * required — the server is designed to work out of the box with a locally-running
 * sensing-server on the default port.
 */

import os from "node:os";
import path from "node:path";
import { existsSync } from "node:fs";
import type { RuviewConfig } from "./types.js";

function env(key: string): string | undefined {
  return process.env[key];
}

function envOrDefault(key: string, fallback: string): string {
  return env(key) ?? fallback;
}

/**
 * Load the effective RuviewConfig from environment variables.
 *
 * Environment variables:
 *   RUVIEW_SENSING_SERVER_URL   — base URL of the sensing-server  (default: http://localhost:3000)
 *   RUVIEW_API_TOKEN            — Bearer token for /api/v1/* routes (no default; auth disabled when absent)
 *   RUVIEW_POSE_COG_BINARY      — path to cog-pose-estimation binary
 *   RUVIEW_COUNT_COG_BINARY     — path to cog-person-count binary
 *   RUVIEW_APPS_DIR             — local edge-module root (default: /var/lib/ruview/apps)
 *   RUVIEW_JOBS_DIR             — directory for job logs (default: ~/.ruview/jobs)
 */
export function loadConfig(): RuviewConfig {
  return {
    sensingServerUrl: envOrDefault(
      "RUVIEW_SENSING_SERVER_URL",
      "http://localhost:3000"
    ),
    apiToken: env("RUVIEW_API_TOKEN"),
    poseCogBinary: envOrDefault(
      "RUVIEW_POSE_COG_BINARY",
      detectCogBinary("cog-pose-estimation")
    ),
    countCogBinary: envOrDefault(
      "RUVIEW_COUNT_COG_BINARY",
      detectCogBinary("cog-person-count")
    ),
    jobsDir: envOrDefault(
      "RUVIEW_JOBS_DIR",
      path.join(os.homedir(), ".ruview", "jobs")
    ),
  };
}

/**
 * Ordered cog-binary candidate paths for a host of the given CPU architecture.
 * The native-arch build is probed FIRST: an appliance that ships both
 * `cog-<id>-arm` and `cog-<id>-x86_64` must never hand back the wrong-arch
 * binary (ADR-264 F8/O7 — the pre-review order tried `-arm` unconditionally).
 * The `/usr/local/bin` and bare-name (PATH) fallbacks follow, arch-agnostic.
 *
 * Pure and arch-injectable so the ordering is unit-testable.
 */
export function cogBinaryCandidates(
  name: string,
  arch: string = process.arch
): string[] {
  const id = name.replace("cog-", "");
  const roots = [envOrDefault("RUVIEW_APPS_DIR", "/var/lib/ruview/apps")];
  // Read-only compatibility fallback for installations made before the fork
  // was decoupled. It is never selected as a new default.
  if (!roots.includes("/var/lib/cognitum/apps")) roots.push("/var/lib/cognitum/apps");
  const candidates = roots.flatMap((root) => {
    const dir = `${root}/${id}`;
    const arm = `${dir}/cog-${id}-arm`;
    const x86 = `${dir}/cog-${id}-x86_64`;
    return arch === "arm64" ? [arm, x86] : [x86, arm];
  });
  return [...candidates, `/usr/local/bin/${name}`];
}

/**
 * Locate a cog binary in the common appliance install locations, probing each
 * candidate in native-arch-first order. Falls back to the bare name (PATH
 * resolution at spawn time) when no candidate exists.
 */
function detectCogBinary(name: string): string {
  for (const candidate of cogBinaryCandidates(name)) {
    if (existsSync(candidate)) return candidate;
  }
  return name; // bare name — rely on PATH; spawn fails gracefully if absent
}
