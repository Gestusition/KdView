# ruview

**Ambient intelligence from WiFi CSI.** Detect human presence, count
people, read breathing and heart rate, and estimate skeletal pose —
using only the WiFi signal already in your home. No cameras. No
wearables. Works through walls and in the dark.

`ruview` is the brand-facing meta-package for the
[KdView](https://github.com/Gestusition/KdView) sensing stack. It installs
the compiled PyO3 wheel published as
[`wifi-densepose`](https://pypi.org/project/wifi-densepose/) and
re-exports its full API under the `ruview` namespace — so you can
write either of these and they do the same thing:

```python
from ruview import BreathingExtractor, SensingClient
from wifi_densepose import BreathingExtractor, SensingClient
```

## Install

```bash
pip install ruview                 # core DSP
pip install "ruview[client]"       # + WebSocket/MQTT clients
```

## Usage

```python
from ruview import BreathingExtractor

br = BreathingExtractor.esp32_default()  # 56 subcarriers @ 100 Hz, 30s window
for residuals, weights in csi_source:
    est = br.extract(residuals=residuals, weights=weights)
    if est is not None:
        print(f"{est.value_bpm:.1f} BPM  (confidence={est.confidence:.2f})")
```

Full API + WebSocket / MQTT / Home Assistant integration docs:
[wifi-densepose on PyPI](https://pypi.org/project/wifi-densepose/).

## Why two PyPI names?

Historic: `wifi-densepose` is the technical / academic name (the
project started as a WiFi-based DensePose implementation).
`ruview` is the brand the v2 ambient-intelligence platform ships
under. Both are the same code. You pick the import that reads
better in your project.

## Links

- **Repository** — https://github.com/Gestusition/KdView
- **Modernization plan** — [ADR-117](https://github.com/Gestusition/KdView/blob/main/docs/adr/ADR-117-pip-wifi-densepose-modernization.md)
- **Issues** — https://github.com/Gestusition/KdView/issues

## License

MIT.
