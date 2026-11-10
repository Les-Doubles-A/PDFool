"""PDFool - local PDF merge, split, extract and optimize desktop app."""
from __future__ import annotations

import os
import re
import sys
import threading
from pathlib import Path
from tkinter import filedialog, messagebox

import customtkinter as ctk
from pypdf import PdfReader, PdfWriter


APP_NAME = "PDFool"


def resource_path(name: str) -> Path:
    return Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent)) / name


class PDFool(ctk.CTk):
    def __init__(self) -> None:
        super().__init__()
        self.title(APP_NAME)
        self.geometry("820x650")
        self.minsize(710, 560)
        self.files: list[Path] = []
        self.busy = False
        self.operation = ctk.StringVar(value="Fusionner")
        self._set_icon()
        self._build()

    def _set_icon(self) -> None:
        icon = resource_path("PDFool.ico")
        if icon.exists():
            try:
                self.iconbitmap(str(icon))
            except Exception:
                pass

    def _build(self) -> None:
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(3, weight=1)
        header = ctk.CTkFrame(self, corner_radius=0, fg_color=("#F5F1ED", "#1E1D20"))
        header.grid(row=0, column=0, sticky="ew")
        ctk.CTkLabel(header, text="PDFool", font=ctk.CTkFont(size=32, weight="bold")).pack(anchor="w", padx=30, pady=(23, 0))
        ctk.CTkLabel(header, text="Les outils essentiels pour vos PDF, hors ligne et sans transfert.", text_color=("#625E5A", "#BBB4AF"), font=ctk.CTkFont(size=13)).pack(anchor="w", padx=30, pady=(0, 22))

        picker = ctk.CTkFrame(self, corner_radius=14)
        picker.grid(row=1, column=0, padx=25, pady=20, sticky="ew")
        picker.grid_columnconfigure(1, weight=1)
        ctk.CTkButton(picker, text="Parcourir les PDF", command=self.choose_files, height=40, font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, padx=18, pady=18)
        self.selection_label = ctk.CTkLabel(picker, text="Aucun document sélectionné", anchor="w", text_color=("#625E5A", "#BBB4AF"))
        self.selection_label.grid(row=0, column=1, padx=(0, 18), sticky="ew")

        tool_box = ctk.CTkFrame(self, corner_radius=14)
        tool_box.grid(row=2, column=0, padx=25, pady=(0, 15), sticky="ew")
        tool_box.grid_columnconfigure(0, weight=1)
        self.operation_picker = ctk.CTkSegmentedButton(tool_box, values=["Fusionner", "Séparer", "Extraire", "Compresser"], variable=self.operation, command=self._operation_changed)
        self.operation_picker.grid(row=0, column=0, padx=18, pady=(17, 8), sticky="ew")
        self.operation_help = ctk.CTkLabel(tool_box, text="Réunit les documents sélectionnés dans un seul PDF.", anchor="w", text_color=("#625E5A", "#BBB4AF"))
        self.operation_help.grid(row=1, column=0, padx=18, pady=(0, 6), sticky="ew")
        self.range_frame = ctk.CTkFrame(tool_box, fg_color="transparent")
        self.range_frame.grid(row=2, column=0, padx=18, pady=(0, 16), sticky="ew")
        ctk.CTkLabel(self.range_frame, text="Pages à extraire (ex. 1-3, 7, 10-12) :").pack(side="left")
        self.range_entry = ctk.CTkEntry(self.range_frame, placeholder_text="1-3, 7")
        self.range_entry.pack(side="left", fill="x", expand=True, padx=(12, 0))
        self.range_frame.grid_remove()

        list_box = ctk.CTkFrame(self, corner_radius=14)
        list_box.grid(row=3, column=0, padx=25, pady=(0, 15), sticky="nsew")
        list_box.grid_columnconfigure(0, weight=1); list_box.grid_rowconfigure(1, weight=1)
        ctk.CTkLabel(list_box, text="Documents sélectionnés", font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, padx=18, pady=(16, 7), sticky="w")
        self.file_list = ctk.CTkTextbox(list_box, activate_scrollbars=True, wrap="word")
        self.file_list.grid(row=1, column=0, padx=18, pady=(0, 16), sticky="nsew")
        self._update_files_view()

        footer = ctk.CTkFrame(self, fg_color="transparent")
        footer.grid(row=4, column=0, padx=25, pady=(0, 25), sticky="ew")
        footer.grid_columnconfigure(0, weight=1)
        self.progress = ctk.CTkProgressBar(footer); self.progress.set(0); self.progress.grid(row=0, column=0, padx=(0, 15), sticky="ew")
        self.progress_label = ctk.CTkLabel(footer, text="Prêt", width=110); self.progress_label.grid(row=0, column=1, padx=(0, 15))
        self.run_button = ctk.CTkButton(footer, text="Lancer l'outil", command=self.start, height=42, font=ctk.CTkFont(weight="bold")); self.run_button.grid(row=0, column=2)

    def _operation_changed(self, operation: str) -> None:
        help_text = {
            "Fusionner": "Réunit les documents sélectionnés dans un seul PDF.",
            "Séparer": "Crée un PDF distinct pour chaque page de chaque document.",
            "Extraire": "Crée un nouveau PDF avec les pages indiquées du premier document.",
            "Compresser": "Optimise les flux de contenu de chaque PDF sans modifier sa mise en page.",
        }
        self.operation_help.configure(text=help_text[operation])
        if operation == "Extraire": self.range_frame.grid()
        else: self.range_frame.grid_remove()

    def choose_files(self) -> None:
        selection = filedialog.askopenfilenames(title="Sélectionner des PDF", filetypes=[("Documents PDF", "*.pdf")])
        if selection:
            self.files = [Path(path) for path in selection]
            self._update_files_view()

    def _update_files_view(self) -> None:
        self.file_list.configure(state="normal")
        self.file_list.delete("1.0", "end")
        if self.files:
            self.file_list.insert("1.0", "\n".join(f"• {path.name}" for path in self.files))
            self.selection_label.configure(text=f"{len(self.files)} document(s) PDF sélectionné(s)")
        else:
            self.file_list.insert("1.0", "Choisissez un ou plusieurs documents PDF pour commencer.")
            self.selection_label.configure(text="Aucun document sélectionné")
        self.file_list.configure(state="disabled")

    def start(self) -> None:
        if self.busy:
            return
        if not self.files:
            messagebox.showwarning(APP_NAME, "Sélectionnez au moins un document PDF.")
            return
        if self.operation.get() == "Extraire":
            try: self._parse_page_range(self.range_entry.get())
            except ValueError as error:
                messagebox.showwarning(APP_NAME, str(error)); return
        output_dir = filedialog.askdirectory(title="Choisir le dossier de sortie")
        if not output_dir:
            return
        self.busy = True; self.progress.set(0); self.run_button.configure(state="disabled", text="Traitement…")
        operation, files = self.operation.get(), list(self.files)
        page_range = self.range_entry.get()
        threading.Thread(target=self._run, args=(operation, files, Path(output_dir), page_range), daemon=True).start()

    def _run(self, operation: str, files: list[Path], output_dir: Path, page_range: str) -> None:
        try:
            if operation == "Fusionner": output = self._merge(files, output_dir)
            elif operation == "Séparer": output = self._split(files, output_dir)
            elif operation == "Extraire": output = self._extract(files[0], output_dir, page_range)
            else: output = self._compress(files, output_dir)
            self.after(0, self._finish, output, None)
        except Exception as error:
            self.after(0, self._finish, [], str(error))

    def _set_progress(self, fraction: float, label: str) -> None:
        self.progress.set(fraction); self.progress_label.configure(text=label)

    def _merge(self, files: list[Path], output_dir: Path) -> list[Path]:
        writer = PdfWriter()
        for index, file in enumerate(files, 1):
            self.after(0, self._set_progress, (index - 1) / len(files), f"Lecture {index}/{len(files)}")
            writer.append(str(file))
        output = self._unique_path(output_dir, "PDFool_fusion.pdf")
        with output.open("wb") as stream: writer.write(stream)
        self.after(0, self._set_progress, 1, "Terminé")
        return [output]

    def _split(self, files: list[Path], output_dir: Path) -> list[Path]:
        result: list[Path] = []
        total = sum(len(PdfReader(str(file)).pages) for file in files)
        done = 0
        for file in files:
            reader = PdfReader(str(file))
            for page_number, page in enumerate(reader.pages, 1):
                writer = PdfWriter(); writer.add_page(page)
                output = self._unique_path(output_dir, f"{file.stem}_page_{page_number}.pdf")
                with output.open("wb") as stream: writer.write(stream)
                result.append(output); done += 1
                self.after(0, self._set_progress, done / total, f"Page {done}/{total}")
        return result

    def _extract(self, file: Path, output_dir: Path, page_range: str) -> list[Path]:
        reader = PdfReader(str(file)); pages = self._parse_page_range(page_range)
        writer = PdfWriter(); valid = [page for page in pages if page <= len(reader.pages)]
        if not valid: raise ValueError("Aucune page demandée n'existe dans ce document.")
        for number in valid: writer.add_page(reader.pages[number - 1])
        output = self._unique_path(output_dir, f"{file.stem}_extrait.pdf")
        with output.open("wb") as stream: writer.write(stream)
        self.after(0, self._set_progress, 1, "Terminé")
        return [output]

    def _compress(self, files: list[Path], output_dir: Path) -> list[Path]:
        result: list[Path] = []
        for index, file in enumerate(files, 1):
            reader, writer = PdfReader(str(file)), PdfWriter()
            for page in reader.pages:
                try: page.compress_content_streams()
                except Exception: pass
                writer.add_page(page)
            output = self._unique_path(output_dir, f"{file.stem}_optimise.pdf")
            with output.open("wb") as stream: writer.write(stream)
            result.append(output)
            self.after(0, self._set_progress, index / len(files), f"PDF {index}/{len(files)}")
        return result

    @staticmethod
    def _parse_page_range(value: str) -> list[int]:
        if not value.strip(): raise ValueError("Indiquez les pages à extraire, par exemple : 1-3, 7.")
        pages: set[int] = set()
        for part in value.replace(" ", "").split(","):
            match = re.fullmatch(r"(\d+)(?:-(\d+))?", part)
            if not match: raise ValueError("Format de pages invalide. Exemple : 1-3, 7, 10-12.")
            start, end = int(match.group(1)), int(match.group(2) or match.group(1))
            if start < 1 or end < start: raise ValueError("Les numéros de page doivent être positifs et croissants.")
            pages.update(range(start, end + 1))
        return sorted(pages)

    @staticmethod
    def _unique_path(folder: Path, filename: str) -> Path:
        candidate = folder / filename; index = 2
        while candidate.exists():
            candidate = folder / f"{Path(filename).stem}_{index}.pdf"; index += 1
        return candidate

    def _finish(self, output: list[Path], error: str | None) -> None:
        self.busy = False; self.run_button.configure(state="normal", text="Lancer l'outil")
        if error:
            self.progress_label.configure(text="Erreur")
            messagebox.showerror(APP_NAME, f"Le traitement n'a pas pu être terminé.\n\n{error}")
        else:
            self.progress.set(1); self.progress_label.configure(text="Terminé")
            messagebox.showinfo(APP_NAME, f"Traitement terminé : {len(output)} fichier(s) créé(s).")


if __name__ == "__main__":
    ctk.set_appearance_mode("System")
    ctk.set_default_color_theme("blue")
    PDFool().mainloop()
