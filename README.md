# PDFool

Dépôt GitHub : **https://github.com/Les-Doubles-A/PDFool**

Boîte à outils PDF pour Windows, 100 % locale : aucun fichier ne quitte l'ordinateur. Application native écrite en Rust (interface egui, manipulation PDF avec lopdf, rendu des pages avec hayro), livrée en un seul exécutable sans dépendance.

## Fonctionnalités

| Catégorie | Outils |
| --- | --- |
| Organiser | Fusionner (avec un signet par document), séparer (page par page, blocs de N pages, plages), extraire des pages, supprimer des pages, réorganiser (ordre libre, inversion, entrelacement recto/verso), pivoter, insérer des pages blanches |
| Modifier | Filigrane texte (taille, opacité, angle, couleur, position), numéros de page / en-têtes / pieds de page (`{n}`, `{total}`, `{fichier}`, `{date}`), logo ou signature image, rognage des marges, métadonnées (modification ou anonymisation) |
| Convertir | Images vers PDF (JPEG intégrés sans perte, transparence PNG conservée), texte vers PDF, PDF vers images (PNG/JPEG, résolution réglable), PDF vers texte, extraction des images |
| Optimiser | Compression (structure + recompression JPEG des images, 4 niveaux), aplatissement en PDF image |
| Sécurité et infos | Protection par mot de passe (AES 128 bits, permissions d'impression/copie/modification), retrait du mot de passe, informations détaillées du document |

Autres points :

- aperçu des pages à droite de la fenêtre, glisser-déposer des fichiers, traitement par lots ;
- sélection de pages souple : `1-3, 7`, `10-` (jusqu'à la fin), `fin`, `paires`, `impaires`, `5-1` (ordre inverse) ;
- les fichiers d'origine ne sont jamais modifiés ni écrasés ;
- thème clair / sombre / système.

## Installation

Téléchargez **`PDFool-Setup-2.0.0.exe`** depuis la [dernière release](https://github.com/Les-Doubles-A/PDFool/releases/latest), puis lancez-le.

L'installation se fait pour l'utilisateur courant, sans droits administrateur (par défaut dans `%LOCALAPPDATA%\Programs\PDFool`), avec un raccourci dans le menu Démarrer et une désinstallation depuis les Paramètres Windows.

L'installeur n'est pas signé : si Windows SmartScreen l'affiche, cliquez sur « Informations complémentaires » puis « Exécuter quand même ».

## Compiler

Prérequis : [Rust](https://rustup.rs) (édition 2024, toolchain MSVC).

```powershell
cargo run --release          # lancer l'application
cargo test                   # tests unitaires et de bout en bout
.\build.ps1                  # tests + exécutable dans dist\ + installeur si Inno Setup 6 est présent
```

L'installeur est généré par [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`iscc installer\PDFool.iss`) dans `installer\Output\`.

## Organisation du code

| Fichier | Rôle |
| --- | --- |
| `src/app.rs` | Interface graphique |
| `src/jobs.rs` | Catalogue des outils et exécution des traitements |
| `src/pdf.rs` | Chargement, pages, fusion, métadonnées, chiffrement, informations |
| `src/edit.rs` | Filigrane, numérotation, tampon image |
| `src/convert.rs` | Conversions et rendu des pages |
| `src/compress.rs` | Compression |
| `src/images.rs` | Intégration et extraction d'images |
| `src/pages.rs` | Analyse des sélections de pages |
| `src/text.rs` | Encodage WinAnsi et métriques Helvetica |

## Limites connues

- L'extraction de texte dépend de la façon dont le PDF a été produit (les PDF scannés sans OCR ne contiennent pas de texte).
- Le texte ajouté (filigrane, numéros) utilise la police Helvetica standard : les caractères hors alphabet latin (Windows-1252) sont remplacés par `?`.
