# -*- mode: python ; coding: utf-8 -*-


a = Analysis(
    ['app.py'],
    pathex=[],
    binaries=[('C:\\Users\\Adam_Echalard\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\DLLs\\_tkinter.pyd', '.')],
    datas=[('PDFool.ico', '.'), ('C:\\Users\\Adam_Echalard\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\Lib\\tkinter', 'tkinter'), ('C:\\Users\\Adam_Echalard\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\tcl', 'tcl')],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=['rthook_tkinter.py'],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name='PDFool',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['PDFool.ico'],
)
