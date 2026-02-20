#!/usr/bin/env python3
"""Capture a desktop screenshot using the Mutter ScreenCast D-Bus API + GStreamer.

This helper must run as the graphical-session user (not root) so the D-Bus
session bus and PipeWire are accessible.

Usage:  python3 sc_capture_thumbnail.py <output_path> [quality]
Output: Prints "OK:<bytes>" on stdout on success, "ERROR:<msg>" on failure.
"""

import os
import subprocess
import sys

import gi

gi.require_version("GLib", "2.0")
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib


def main() -> None:
    output_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/sc_thumbnail.jpg"
    quality = sys.argv[2] if len(sys.argv) > 2 else "75"

    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    except Exception as e:
        print(f"ERROR:dbus_connect:{e}", flush=True)
        sys.exit(1)

    try:
        # Create ScreenCast session
        result = bus.call_sync(
            "org.gnome.Mutter.ScreenCast",
            "/org/gnome/Mutter/ScreenCast",
            "org.gnome.Mutter.ScreenCast",
            "CreateSession",
            GLib.Variant(
                "(a{sv})",
                [
                    {
                        "is-recording": GLib.Variant("b", False),
                        "disable-animations": GLib.Variant("b", True),
                    }
                ],
            ),
            GLib.VariantType("(o)"),
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
        session_path = result.unpack()[0]

        # RecordMonitor — empty string = primary monitor
        result = bus.call_sync(
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            "RecordMonitor",
            GLib.Variant(
                "(sa{sv})", ["", {"cursor-mode": GLib.Variant("u", 0)}]
            ),
            GLib.VariantType("(o)"),
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
        stream_path = result.unpack()[0]

        # Listen for PipeWireStreamAdded signal
        node_id_box: list = [None]
        loop = GLib.MainLoop()

        def _on_signal(_conn, _sender, _path, _iface, signal, params):
            if signal == "PipeWireStreamAdded":
                node_id_box[0] = params.unpack()[0]
                loop.quit()

        sub_id = bus.signal_subscribe(
            "org.gnome.Mutter.ScreenCast",
            "org.gnome.Mutter.ScreenCast.Stream",
            "PipeWireStreamAdded",
            stream_path,
            None,
            Gio.DBusSignalFlags.NONE,
            _on_signal,
        )

        # Start the session (triggers PipeWireStreamAdded)
        bus.call_sync(
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            "Start",
            None,
            None,
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )

        # Wait for signal (5 s timeout)
        GLib.timeout_add_seconds(5, lambda: (loop.quit(), False)[1])
        loop.run()
        bus.signal_unsubscribe(sub_id)

        if node_id_box[0] is None:
            _stop(bus, session_path)
            print("ERROR:no_pipewire_node", flush=True)
            sys.exit(1)

        pw_node = node_id_box[0]

        # GStreamer: capture one JPEG frame from the PipeWire node
        gst_cmd = [
            "gst-launch-1.0",
            "-e",
            "pipewiresrc",
            f"path={pw_node}",
            "do-timestamp=true",
            "keepalive-time=1000",
            "num-buffers=1",
            "!",
            "videoconvert",
            "!",
            "jpegenc",
            f"quality={quality}",
            "!",
            "filesink",
            f"location={output_path}",
        ]
        subprocess.run(
            gst_cmd,
            timeout=10,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        _stop(bus, session_path)

    except Exception as e:
        print(f"ERROR:capture:{e}", flush=True)
        sys.exit(1)

    if os.path.exists(output_path) and os.path.getsize(output_path) > 0:
        size = os.path.getsize(output_path)
        print(f"OK:{size}", flush=True)
    else:
        print("ERROR:empty_file", flush=True)
        sys.exit(1)


def _stop(bus, session_path: str) -> None:
    """Best-effort stop of the Mutter ScreenCast session."""
    try:
        bus.call_sync(
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            "Stop",
            None,
            None,
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
    except Exception:
        pass


if __name__ == "__main__":
    main()
