$ErrorActionPreference = 'Stop'
$python = 'C:\Users\Adam_Echalard\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
& $python -m pip install customtkinter pypdf pillow pyinstaller
& $python convert_icon.py
& $python -m PyInstaller --noconfirm --clean --onefile --windowed --name PDFool --icon PDFool.ico --runtime-hook rthook_tkinter.py --add-data 'PDFool.ico;.' --add-data 'C:\Users\Adam_Echalard\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\Lib\tkinter;tkinter' --add-binary 'C:\Users\Adam_Echalard\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\DLLs\_tkinter.pyd;.' --add-data 'C:\Users\Adam_Echalard\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\tcl;tcl' app.py
Write-Host "Application créée : $PWD\dist\PDFool.exe"
