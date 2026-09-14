# Audit avant mise en dépôt — 14 septembre 2026

Les deux défauts d'export relevés ont été corrigés et la licence MIT a été choisie
par l'utilisateur. Le dossier est prêt pour la mise en dépôt après examen des
fichiers à ajouter. Aucun dépôt, commit ou distant n'a été créé.

## Constats corrigés

### P1 corrigé — Fichiers privés dans les exports

Dans la version auditée de `scripts/export.py`, tout le contenu de `skills/` était copié, à l'exception
des caches Python et de `.DS_Store`. Un `.env` ou un dossier `.auto/` placé dans
un skill était donc distribué. Le `.gitignore` n'est pas consulté par l'exporteur.

Reproduction exécutée dans un répertoire temporaire, avec uniquement des données
synthétiques : `skills/demo/.env` et `skills/demo/.auto/session.jsonl` se retrouvent
tous deux dans l'export Codex. Aucun de ces fichiers sensibles n'a été trouvé
dans l'arbre actuel.

Correction : `PORTABLE_FILES` définit explicitement les fichiers distribuables.
Tout fichier supplémentaire est omis, quel que soit son nom ou son emplacement
dans les skills. Un test injecte des configurations, sessions, clés et données
privées synthétiques et vérifie leur absence dans les cinq exports. Toute nouvelle
ressource requiert un ajout explicite à cette liste après examen de son contenu.

### P2 corrigé — Liens symboliques dans les chemins des ressources

Dans la version auditée de `scripts/export.py`, le contrôle portait sur le fichier
manifeste, sans contrôler son dossier parent. Un `.codex-plugin` symbolique vers
un dossier extérieur passait le contrôle et son `plugin.json` était copié.

Reproduction exécutée avec un manifeste externe synthétique : l'export réussit
et contient ce manifeste. Aucun lien symbolique n'est présent dans l'arbre actuel.

Correction : `validate_resource` contrôle chaque composant sous la racine du
toolkit, ainsi que le type attendu (dossier parent ou fichier final). Les tests
couvrent les trois dossiers de manifestes, les cinq manifestes, la documentation,
la licence et les chemins des skills, ainsi que les liens cassés et ressources
absentes ou de type incorrect. Le refus intervient avant création de la destination.
Une destination dans les skills sources est également refusée.

### Décision appliquée — Licence MIT

L'utilisateur a choisi MIT pour ses fichiers personnalisés. `LICENSE` à la racine
porte le copyright 2026 metinam et accompagne les cinq exports. Le `package.json`
racine déclare cette licence. La licence MIT du moteur tiers et ses attributions
sont conservées dans `originals/pi-autoresearch/LICENSE`.

## Correction effectuée

Le `.gitignore` initial ne protégeait ni les fichiers `.env`, ni certains fichiers
d'authentification usuels, ni les dépendances locales. Il exclut désormais ces
fichiers, les clés usuelles, les environnements Python et les caches courants.
Les `.env.example` et `.env.sample` restent versionnables et doivent contenir
uniquement des exemples sans secret. Ces exclusions ne remplacent pas l'examen
des fichiers avant ajout et ne protègent pas les exports.

## Vérifications exécutées

- `python3 -m unittest discover -s tests -v` : **22 tests réussis**, incluant les
  cinq exports et l'exécution du helper depuis chaque export.
- Cinq nouveaux tests de régression, avec plusieurs cas par test, couvrent les
  exclusions de fichiers privés, les ressources invalides et les destinations sources.
- Tous les fichiers JSON présents sont syntaxiquement valides. Cela ne valide
  pas les schémas ni le chargement dans les agents.
- Recherche textuelle de secrets, identifiants, chemins locaux et commandes
  sensibles : aucun secret évident identifié. Cette recherche par motifs
  ne constitue pas une garantie d'absence de secrets.
- Inventaire initial : 49 fichiers hors caches Python ; plus gros fichier
  de 116 981 octets. Aucun lien symbolique trouvé.
- Aucun dépôt Git existant : pas d'historique à inspecter.

## Limites et archives

La lecture approfondie a porté sur les deux helpers Python, leurs tests, les
skills portables, les manifestes et la documentation. Le moteur archivé a fait
l'objet d'une recherche ciblée, pas d'un audit complet ni d'une exécution.
Son code contient bien des restaurations globales et `git clean -fd`
(`originals/pi-autoresearch/extensions/pi-autoresearch/index.ts:2446-2447`),
déjà signalés dans le README. Le manifeste racine Pi ne charge que `skills/`.
Les archives originales n'ont pas été modifiées.

Les contrôles d'export supposent que les sources restent stables pendant la copie ;
ils ne verrouillent pas les fichiers contre des modifications concurrentes.
Ils contrôlent les chemins, pas l'absence de secrets dans le contenu des fichiers
explicitement distribués. Le moteur archivé et les installations réelles des
agents restent hors du périmètre de validation fonctionnelle.

Les installations réelles Codex, Claude Code, Cursor et Pi, leurs schémas actuels,
la comparaison avec les sources installées et la provenance amont n'ont pas été
revalidés pendant cet audit. Aucune dépendance installée, aucun service externe
contacté et aucune boucle d'optimisation lancée.
