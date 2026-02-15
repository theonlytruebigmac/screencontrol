#!/usr/bin/env python3
"""
ScreenControl Chat Window — persistent chat GUI for the agent.

Communication:
  stdin:  JSON lines  {"sender": "...", "content": "..."}
  stdout: JSON lines  {"content": "..."}

Styled to match the ScreenControl web UI palette.
"""

import json
import sys
import threading
import tkinter as tk
from tkinter import font as tkfont
from datetime import datetime


# ── ScreenControl palette (from globals.css) ──────────────────
BG           = "#1a1a1a"
SURFACE      = "#1e1e1e"
SURFACE_LIGHT= "#2a2a2a"
SURFACE_DARK = "#141414"
BORDER       = "#333333"
BORDER_LIGHT = "#3f3f3f"
PRIMARY      = "#e05246"
PRIMARY_DARK = "#c43d32"
TEXT         = "#e0e0e0"
TEXT_DIM     = "#888888"
TEXT_MUTED   = "#666666"
SUCCESS      = "#10b981"
ACCENT       = "#22d3ee"
INPUT_BG     = "#353535"


def _pick_font():
    """Find the best available font, preferring Inter."""
    available = set(tkfont.families())
    for candidate in [
        "Inter Variable", "Inter", "Inter Display",
        "Noto Sans", "DejaVu Sans", "Liberation Sans",
        "Ubuntu", "Roboto", "Cantarell",
        "Helvetica", "Arial",
    ]:
        if candidate in available:
            return candidate
    return "TkDefaultFont"


class ChatWindow:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("ScreenControl — Support Chat")
        self.root.geometry("420x520")
        self.root.minsize(340, 420)
        self.root.configure(bg=BG)

        self.font_family = _pick_font()

        try:
            self.root.iconname("ScreenControl")
        except Exception:
            pass

        self.root.attributes("-topmost", True)
        self.root.after(3000, lambda: self.root.attributes("-topmost", False))

        self._build_ui()
        self._start_stdin_reader()
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)

    # ── UI construction ──────────────────────────────────────
    def _build_ui(self):
        f = self.font_family

        # ── INPUT BAR (pack FIRST so it claims bottom space) ──
        input_bar = tk.Frame(self.root, bg=SURFACE, padx=10, pady=10)
        input_bar.pack(side=tk.BOTTOM, fill=tk.X)

        # Separator above input
        tk.Frame(self.root, bg=BORDER, height=1).pack(side=tk.BOTTOM, fill=tk.X)

        self.send_btn = tk.Button(
            input_bar, text=" Send ",
            bg=PRIMARY, fg="white",
            activebackground=PRIMARY_DARK, activeforeground="white",
            font=(f, 10, "bold"),
            borderwidth=0, padx=12, pady=6,
            cursor="hand2",
            command=lambda: self._on_send(None),
        )
        self.send_btn.pack(side=tk.RIGHT, padx=(8, 0))

        self.input_field = tk.Entry(
            input_bar,
            bg=INPUT_BG, fg=TEXT,
            insertbackground=TEXT,
            font=(f, 11),
            borderwidth=0, highlightthickness=1,
            highlightcolor=PRIMARY, highlightbackground=BORDER_LIGHT,
        )
        self.input_field.pack(side=tk.LEFT, fill=tk.X, expand=True, ipady=8)
        self.input_field.bind("<Return>", self._on_send)
        self.input_field.focus_set()

        # Placeholder text
        self._set_placeholder()
        self.input_field.bind("<FocusIn>", self._clear_placeholder)
        self.input_field.bind("<FocusOut>", self._restore_placeholder)

        # ── HEADER ────────────────────────────────────────────
        header = tk.Frame(self.root, bg=SURFACE, height=46)
        header.pack(side=tk.TOP, fill=tk.X)
        header.pack_propagate(False)

        # Red accent bar
        tk.Frame(header, bg=PRIMARY, width=4).pack(side=tk.LEFT, fill=tk.Y)

        tk.Label(
            header, text="  Support Chat",
            bg=SURFACE, fg=TEXT,
            font=(f, 13, "bold"),
        ).pack(side=tk.LEFT, pady=0)

        # Status badge
        sf = tk.Frame(header, bg=SURFACE)
        sf.pack(side=tk.RIGHT, padx=12)
        tk.Label(sf, text="●", bg=SURFACE, fg=SUCCESS,
                 font=(f, 9)).pack(side=tk.LEFT)
        tk.Label(sf, text="Connected", bg=SURFACE, fg=TEXT_DIM,
                 font=(f, 9)).pack(side=tk.LEFT, padx=(4, 0))

        # Separator below header
        tk.Frame(self.root, bg=BORDER, height=1).pack(side=tk.TOP, fill=tk.X)

        # ── CHAT AREA (fills remaining space) ─────────────────
        chat_frame = tk.Frame(self.root, bg=SURFACE_DARK)
        chat_frame.pack(side=tk.TOP, fill=tk.BOTH, expand=True)

        self.chat_area = tk.Text(
            chat_frame,
            wrap=tk.WORD,
            bg=SURFACE_DARK,
            fg=TEXT,
            insertbackground=TEXT,
            font=(f, 11),
            padx=14, pady=12,
            borderwidth=0, highlightthickness=0,
            state=tk.DISABLED, cursor="arrow",
            spacing1=2, spacing3=2,
        )
        scrollbar = tk.Scrollbar(
            chat_frame, command=self.chat_area.yview,
            bg=SURFACE_DARK, troughcolor=SURFACE_DARK,
            highlightbackground=SURFACE_DARK,
            activebackground=BORDER_LIGHT,
            width=6, borderwidth=0, elementborderwidth=0,
        )
        self.chat_area.configure(yscrollcommand=scrollbar.set)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.chat_area.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        # Text tags
        self.chat_area.tag_configure(
            "tech_name", foreground=PRIMARY, font=(f, 10, "bold"))
        self.chat_area.tag_configure(
            "tech_msg", foreground=TEXT, font=(f, 11))
        self.chat_area.tag_configure(
            "you_name", foreground=ACCENT, font=(f, 10, "bold"))
        self.chat_area.tag_configure(
            "you_msg", foreground="#c8dce8", font=(f, 11))
        self.chat_area.tag_configure(
            "timestamp", foreground=TEXT_MUTED, font=(f, 8))
        self.chat_area.tag_configure(
            "spacer", font=(f, 4))

    # ── Placeholder helpers ──────────────────────────────────
    def _set_placeholder(self):
        self.input_field.insert(0, "Type a message...")
        self.input_field.configure(fg=TEXT_MUTED)
        self._placeholder_active = True

    def _clear_placeholder(self, _event=None):
        if getattr(self, "_placeholder_active", False):
            self.input_field.delete(0, tk.END)
            self.input_field.configure(fg=TEXT)
            self._placeholder_active = False

    def _restore_placeholder(self, _event=None):
        if not self.input_field.get().strip():
            self._set_placeholder()

    # ── Message rendering ────────────────────────────────────
    def _append_message(self, sender: str, content: str, is_self: bool = False):
        self.chat_area.configure(state=tk.NORMAL)
        now = datetime.now().strftime("%I:%M %p")

        if is_self:
            self.chat_area.insert(tk.END, "You\n", "you_name")
            self.chat_area.insert(tk.END, f"{content}\n", "you_msg")
        else:
            self.chat_area.insert(tk.END, f"{sender}\n", "tech_name")
            self.chat_area.insert(tk.END, f"{content}\n", "tech_msg")

        self.chat_area.insert(tk.END, f"{now}\n", "timestamp")
        self.chat_area.insert(tk.END, "\n", "spacer")
        self.chat_area.configure(state=tk.DISABLED)
        self.chat_area.see(tk.END)

    # ── Send handler ─────────────────────────────────────────
    def _on_send(self, _event):
        if getattr(self, "_placeholder_active", False):
            return
        text = self.input_field.get().strip()
        if not text:
            return
        self.input_field.delete(0, tk.END)
        self._append_message("You", text, is_self=True)
        try:
            sys.stdout.write(json.dumps({"content": text}) + "\n")
            sys.stdout.flush()
        except (BrokenPipeError, IOError):
            pass

    def _on_close(self):
        self.root.destroy()

    # ── Stdin reader ─────────────────────────────────────────
    def _start_stdin_reader(self):
        def reader():
            try:
                for line in sys.stdin:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                        sender = data.get("sender", "Support")
                        content = data.get("content", "")
                        self.root.after(0, self._append_message, sender, content, False)
                        self.root.after(0, self._raise_window)
                    except json.JSONDecodeError:
                        pass
            except (EOFError, IOError):
                self.root.after(0, self.root.destroy)

        threading.Thread(target=reader, daemon=True).start()

    def _raise_window(self):
        self.root.deiconify()
        self.root.lift()
        self.root.focus_force()

    def run(self):
        self.root.mainloop()


if __name__ == "__main__":
    ChatWindow().run()
