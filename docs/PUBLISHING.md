# Publier Ultramarine, étape par étape

Le nom affiché est **Ultramarine**. Le nom npm proposé est
**`@metimer/ultramarine`**, version **1.0.0**. Le dépôt GitHub et les commandes
`autoresearch` conservent leurs noms actuels. Cette procédure prépare puis publie
une release ; aucun script du dépôt ne publie automatiquement.

## 1. Vérifier les comptes et le nom npm

Sur [npmjs.com](https://www.npmjs.com/), créer le compte si nécessaire, vérifier
l'adresse email et configurer la double authentification. Le compte doit être
`metimer` ou disposer des droits de publication sur l'organisation `metimer`.
Ne pas envoyer de mot de passe ou de token dans une conversation.

Depuis le terminal :

```sh
npm login --registry=https://registry.npmjs.org/
npm whoami --registry=https://registry.npmjs.org/
npm view @metimer/ultramarine name version --registry=https://registry.npmjs.org/
gh auth status
```

La vérification du 29 septembre 2026 a renvoyé E404 pour ce paquet public.
Un E404 ne prouve pas que le compte possède le scope : le vérifier sur npm.
Si le scope change, modifier `packaging/pi/package.json`, les commandes et les
liens documentés avant de construire et qualifier les artefacts finaux.
Si GitHub n'est pas connecté, exécuter `gh auth login` et suivre le navigateur.

Prérequis locaux pour reconstruire : Node.js 24, npm, Python 3.10+, Git,
GitHub CLI et Rust 1.81+. La CI utilise Rust 1.81 et les dépendances verrouillées.

## 2. Vérifier les logos intégrés

Les deux logos 2000 × 2000 sont inclus dans les distributions :

- `ultramarine_wback.png` : version transparente utilisée dans les README.
- `ultramarine.png` : version sur fond clair pour l'aperçu du catalogue Pi.

Le README npm et le champ `pi.image` utilisent des URL GitHub liées au tag
`v1.0.0`. Ces images seront accessibles après publication de la release GitHub,
avant la publication npm. Vérifier leur affichage à cette étape.

## 3. Enregistrer et pousser les changements

Dans le dépôt, relire `git diff` et `git status`. La liste suivante ajoute les
fichiers de cette préparation, y compris les deux logos :

```sh
git add README.md ultramarine.png ultramarine_wback.png \
  adapters/pi/README.md adapters/pi/package.json \
  adapters/pi/package-lock.json adapters/pi/tests/adapter.test.mjs \
  docs/PI_PACKAGE.md docs/PUBLISHING.md docs/GITHUB_RELEASE.md \
  docs/RELEASE_NOTES.md docs/harnesses/pi/README.md \
  packaging/pi scripts/package_pi.py scripts/qualify_npm.mjs \
  scripts/prepare_publication.py scripts/qualify.py scripts/export.py \
  tests/test_pi_package.py tests/test_publication.py .github/workflows/candidate.yml
git commit -m "Prepare Ultramarine Pi package and publication workflow"
git push origin release/1.0.0
git rev-parse HEAD
```

Conserver le SHA complet affiché. La procédure peut publier ce commit de la
branche de release directement. Si le projet est fusionné dans `main` avant
publication, utiliser le nouveau commit et attendre sa propre CI.
Pour un build local avec `--require-clean`, les deux logos et les changements
doivent être committés, et le dépôt ne doit contenir aucun fichier non suivi.

## 4. Attendre la CI sur ce commit

```sh
gh run list --repo Metimer/autoresearch-toolkit --commit SHA_COMPLET \
  --json databaseId,headSha,workflowName,status,conclusion,url
```

Remplacer `SHA_COMPLET` par le SHA de l'étape 3. Exiger `completed` et `success`
pour **Checks** et **Candidate packages** sur ce même commit.
Le second workflow doit avoir validé les quatre moteurs natifs, les cinq paquets
de skills et les deux contrôles npm Linux/macOS. Les succès antérieurs sur
`581f0d3` ne qualifient pas les nouveaux changements.

Noter le `databaseId` du workflow **Candidate packages** réussi : c'est
`RUN_ID` dans la commande suivante.

## 5. Récupérer et vérifier les fichiers à publier

Choisir des dossiers de sortie qui n'existent pas encore :

```sh
gh run download RUN_ID --repo Metimer/autoresearch-toolkit \
  --pattern 'candidate-*' --dir dist/ci-final
python3 scripts/prepare_publication.py \
  --artifacts dist/ci-final --output dist/publication --commit SHA_COMPLET
```

Le script exige tous les artefacts, vérifie les inventaires, checksums, rapports
et le commit sans changements locaux, puis regroupe les fichiers avec un
`SHA256SUMS` commun. Il ne crée ni tag, ni release, ni publication npm.
La réussite des workflows eux-mêmes doit avoir été vérifiée à l'étape 4.

## 6. Créer et publier la release GitHub du moteur

Relire `docs/GITHUB_RELEASE.md`, puis créer le brouillon :

```sh
gh release create v1.0.0 dist/publication/* \
  --repo Metimer/autoresearch-toolkit --target SHA_COMPLET --draft \
  --title "Ultramarine 1.0.0" --notes-file docs/GITHUB_RELEASE.md
```

Ouvrir les [releases GitHub](https://github.com/Metimer/autoresearch-toolkit/releases),
vérifier le commit cible et les fichiers joints. Publier avec le bouton
**Publish release**, ou :

```sh
gh release edit v1.0.0 --repo Metimer/autoresearch-toolkit --draft=false --latest
```

Si `v1.0.0` existe déjà, inspecter son commit avant de continuer ; ne pas déplacer
un tag publié. Les liens vers les binaires doivent fonctionner avant npm.

## 7. Publier le paquet npm qualifié

Depuis la racine du dépôt, publier exactement le tarball téléchargé de la CI :

```sh
npm publish dist/publication/metimer-ultramarine-1.0.0.tgz \
  --access public --registry=https://registry.npmjs.org/
```

Compléter la validation navigateur/2FA demandée par npm. Ne pas lancer
`npm publish` à la racine ou dans `adapters/pi` : ces manifestes sont privés.
La publication d'une version npm est définitive : une correction ultérieure
nécessitera une nouvelle version, avec les mises à jour de compatibilité requises.

## 8. Vérifier l'installation publique

```sh
npm view @metimer/ultramarine@1.0.0 name version dist.integrity \
  --registry=https://registry.npmjs.org/
pi install npm:@metimer/ultramarine@1.0.0
```

Redémarrer Pi et vérifier les skills `autoresearch-scout` et `autoresearch-run`,
le tool `autoresearch_engine` et la commande `/autoresearch-stop`.
Suivre le [guide d'installation](PI_PACKAGE.md) pour connecter le moteur et une
session réelle. Vérifier `status` avant un premier benchmark explicitement autorisé.
Cette installation enregistre le paquet dans la configuration personnelle Pi.

Rechercher ensuite Ultramarine sur [Pi Packages](https://pi.dev/packages).
Le mot-clé `pi-package` rend le paquet éligible ; son apparition n'est pas
nécessairement immédiate. Actualiser ensuite les mentions « publication à venir »
du README et des guides pour refléter la publication effective.

## Références

- [Format et découverte des packages Pi](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
- [Publication npm](https://docs.npmjs.com/cli/commands/npm-publish)
- [Téléchargement des artefacts GitHub](https://cli.github.com/manual/gh_run_download)
- [Création d'une release GitHub](https://cli.github.com/manual/gh_release_create)
