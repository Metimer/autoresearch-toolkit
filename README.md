# Autoresearch Toolkit

Des skills pour guider un agent de développement dans l’optimisation mesurée
d’un projet : établir une mesure de référence, tester une hypothèse à la fois
et ne conserver que les améliorations vérifiées.

Le workflow repose sur deux étapes :

| Skill | Rôle |
| --- | --- |
| `autoresearch-scout` | Découvrir les tests et benchmarks, définir le périmètre et établir une référence reproductible. |
| `autoresearch-run` | Mener des expériences dans un budget défini, comparer les résultats et conserver les changements validés. |

## Prérequis

- Un agent capable de lire des fichiers, modifier du code et exécuter des commandes.
- Un projet cible sous Git, avec des vérifications de comportement exécutables.
- Python 3.10 ou supérieur pour les scripts du toolkit, sans dépendance tierce.
- macOS ou Linux pour le helper de mesure ; utiliser WSL sous Windows.

## Démarrage rapide

### 1. Préparer une référence

Depuis une session de votre agent ouverte sur le projet à optimiser :

> Lis `/chemin/vers/autoresearch-toolkit/skills/autoresearch-scout/SKILL.md`.
> Prépare une référence de performance pour ce dépôt, sans optimiser ni commiter.
> Objectif : réduire le temps d’exécution de [commande ou traitement].
> Budget de préparation : 10 minutes.

Le scout identifie les commandes réelles du projet, définit la métrique et les
fichiers autorisés, puis exécute les vérifications et plusieurs séries de mesures.
Il stocke le contexte, la méthode et les résultats dans `.auto/` du projet cible.
Une référence trop bruitée doit être améliorée avant de lancer les expériences.

### 2. Lancer les expériences

Une fois la référence validée, dans un checkout isolé pour les expériences :

> Lis `/chemin/vers/autoresearch-toolkit/skills/autoresearch-run/SKILL.md`.
> Utilise la session `.auto/` préparée pour ce dépôt.
> Maximum 5 expériences et 20 minutes, uniquement sur les chemins autorisés
> dans `.auto/prompt.md`. Ne commite pas.

L’agent formule une hypothèse, applique un changement limité, exécute les tests
et mesure son effet. Un gain doit dépasser le bruit observé et préserver le
comportement du projet. Les résultats sont consignés dans
`.auto/portable-log.jsonl` ; l’état des changements conservés est décrit dans
`.auto/portable-state.md` pour une reprise explicite.

Adaptez les chemins et les budgets à votre installation et à votre projet.
La préparation de la référence et le lancement des expériences sont deux demandes
distinctes. Les fichiers `.auto/` restent locaux au projet cible.

## Formats d’intégration

Le toolkit fournit les manifestes suivants, qui utilisent les mêmes skills :

| Hôte | Manifeste fourni |
| --- | --- |
| Codex | `.codex-plugin/plugin.json` |
| Claude Code | `.claude-plugin/plugin.json` |
| Cursor | `.cursor-plugin/plugin.json` |
| Pi | `package.json`, via `pi.skills` |
| Agent Plugins | `plugin.json` |

Utilisez le mécanisme de chargement de votre hôte ou fournissez directement le
chemin du skill à l’agent, comme dans les exemples ci-dessus. Choisissez un seul
mode de découverte pour éviter les doublons. La présence d’un manifeste ne garantit
pas son chargement dans toutes les versions de l’hôte.

## Exporter un paquet

Depuis la racine du dépôt, produisez un paquet pour l’hôte choisi :

```sh
python3 scripts/export.py --agent codex --output dist/codex/autoresearch-toolkit
```

Les valeurs acceptées par `--agent` sont `codex`, `claude`, `cursor`, `pi` et
`generic`. La destination doit être un nouveau dossier nommé
`autoresearch-toolkit`, situé hors des skills sources.

Chaque paquet contient les skills, leurs ressources, le manifeste sélectionné,
le README et la licence. L’export utilise une liste explicite de fichiers et
refuse les liens symboliques dans leurs chemins sources. Il ne remplace pas une
installation existante. En cas d’erreur de copie, une sortie partielle peut rester
sur disque ; inspectez-la avant de réessayer.

## Mesurer une commande

Le helper peut également être utilisé directement :

```sh
python3 skills/autoresearch-scout/scripts/measure.py \
  --name bench_ms --runs 5 --warmup 1 --timeout 10 --budget 60 \
  -- python3 -c 'sum(range(1000000))'
```

Il mesure le temps écoulé en millisecondes et publie trois lignes `METRIC` :
la médiane (`bench_ms` dans cet exemple), le minimum (`run_min_ms`) et le maximum
(`run_max_ms`). Les mesures d’échauffement sont exclues. Les sorties de la commande
vont sur stderr ; un échec ou un dépassement de délai ne produit aucune métrique.

Ce helper convient aux durées de commandes. Le lancement de processus ajoute du
bruit aux traitements très courts. La mémoire, la taille ou le débit nécessitent
une commande de mesure adaptée.

## Cadre d’exécution

Le workflow est piloté par l’agent. Le périmètre d’édition, la protection des tests
et le budget global sont des consignes qu’il doit respecter. Le helper impose
ses propres délais de mesure et termine le groupe de processus lancé en cas de
timeout ou d’interruption ; il ne constitue pas une sandbox.

Les scripts du toolkit ne demandent ni clé API ni connexion à un fournisseur LLM.
L’agent et les commandes du projet peuvent avoir leurs propres dépendances et
besoins réseau. Toute reprise de la boucle requiert une nouvelle invocation.

## Développement

Exécuter les tests depuis la racine du dépôt :

```sh
python3 -m unittest discover -s tests -v
```

La suite couvre les mesures, les échecs, les délais et les exports des cinq formats.
Pour ajouter une ressource à un skill, déclarez-la dans `PORTABLE_FILES` de
`scripts/export.py` afin de l’inclure dans les paquets distribués.

Les sources de référence dans `originals/` sont distinctes des skills maintenus
et ne sont ni chargées par les manifestes racine ni incluses dans les exports.

## Licence

[MIT](LICENSE). Les composants tiers conservés dans `originals/` gardent leurs
licences et attributions respectives.
