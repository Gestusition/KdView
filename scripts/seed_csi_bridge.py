#!/usr/bin/env python3
"""Compatibility launcher for the renamed gateway CSI bridge.

New automation should invoke ``gateway_csi_bridge.py`` and use
``--gateway-url``. This launcher intentionally contains no vendor endpoint and
may be removed only after downstream fleet scripts have migrated.
"""

from gateway_csi_bridge import main


if __name__ == "__main__":
    main()
