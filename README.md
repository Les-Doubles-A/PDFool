# PDFool

Dépôt GitHub : **https://github.com/Les-Doubles-A/PDFool**

Outil local pour fusionner, séparer, extraire et optimiser des PDF.

Exécutez `build.ps1` dans PowerShell pour générer `dist\PDFool.exe`. L’icône est automatiquement convertie depuis `PDFool.png` vers `PDFool.ico`.

<!-- installeur-release -->
## Installation

Téléchargez **`PDFool-Setup-1.0.0.exe`** depuis la [dernière release](https://github.com/Les-Doubles-A/PDFool/releases/latest), puis lancez-le.

L'installation se fait pour l'utilisateur courant, sans droits administrateur (par défaut dans `%LOCALAPPDATA%\Programs\PDFool`), avec un raccourci dans le menu Démarrer et une désinstallation depuis les Paramètres Windows.

L'installeur n'est pas signé : si Windows SmartScreen l'affiche, cliquez sur « Informations complémentaires » puis « Exécuter quand même ».

### Recréer l'installeur

Compiler l'application (`build.ps1 (PyInstaller)`), puis lancer [Inno Setup 6](https://jrsoftware.org/isinfo.php) : `iscc installer\PDFool.iss`. Le setup est créé dans `installer\Output\`.
