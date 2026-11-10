"""Expose le dossier tkinter embarqué avant l'import de CustomTkinter."""
import sys

if getattr(sys, "frozen", False):
    sys.path.insert(0, sys._MEIPASS)
