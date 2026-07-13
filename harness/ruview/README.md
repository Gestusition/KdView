# KdView WiFi-sensing operator harness

An AI agent harness that knows how to operate **RuView** (WiFi-DensePose): onboard a
newcomer, provision an ESP32 CSI node, calibrate a room, train pose models, and —
crucially — **refuse to overstate accuracy**. Minted from the RuView monorepo via
[`metaharness`](https://www.npmjs.com/package/metaharness) and hardened per **ADR-182**.

WiFi sensing infers *coarse* pose/presence/breathing from Channel State Information.
It is **not a camera**. Every accuracy number this harness emits must be MEASURED
against a baseline — that rule is enforced in code (`ruview_claim_check`).

## Quick start

```bash
node harness/ruview/bin/cli.js                       # onboard — pick a setup path
node harness/ruview/bin/cli.js claim-check --file REPORT.md
node harness/ruview/bin/cli.js verify                # deterministic proof
node harness/ruview/bin/cli.js doctor                # tools + optional kernel/host
node harness/ruview/bin/cli.js --help
```

The operator tools are pure Node and run with **zero install weight** — the
package has no dependencies at all (ADR-263 O3). `doctor` / `install` can
additionally use `@metaharness/kernel` + a host adapter if you install them
(`npm i @metaharness/kernel @metaharness/host-claude-code`); everything else
runs without them.

## Tools (`ruview_*`)

Exposed both as CLI verbs and as a repository-local MCP server (`node harness/ruview/bin/cli.js mcp start`):

| Tool | What it does |
|------|--------------|
| `ruview_onboard` | Pick docker-demo / repo-build / live-esp32; print the next command |
| `ruview_claim_check` | Lint text for untagged / overstated accuracy claims (guardrail) |
| `ruview_verify` | Run `verify.py` deterministic proof → VERDICT |
| `ruview_node_monitor` | Assert CSI is flowing on an ESP32 (read-only) |
| `ruview_calibrate` | ADR-151 room pipeline (baseline→enroll→train-room→room-watch) |
| `ruview_node_flash` | Build+flash firmware (Windows/ESP-IDF; mutating, guarded) |

Every tool is **fail-closed**: missing repo / python / binary / port → an honest
negative, never a fabricated success.

## Skills

Host-neutral playbooks in `skills/` (`onboard`, `provision-node`, `calibrate-room`,
`train-pose`, `verify`). `node harness/ruview/bin/cli.js skill <name>` prints one.

## Use as a Claude Code MCP server

The bundled `.claude/settings.json` registers the repository-local `ruview` MCP server.
From the repository root, run `node harness/ruview/bin/cli.js install --host claude-code`
if you want the harness to install its host configuration.

## Hosts

claude-code (bundled), and via metaharness host adapters: codex, opencode, copilot,
pi-dev, hermes, rvm, github-actions.

## License

MIT. See `LICENSE` for upstream copyright attribution.
