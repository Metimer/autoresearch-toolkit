# Autoresearch Toolkit

Extraction locale indépendante de l'IHM : scout personnalisé adapté et workflow
d'optimisation portable. Version initiale 0.1.0, pas une certification tous agents.

## Deux modes distincts

| Mode | Contenu | Exécution |
| --- | --- | --- |
| Portable | skills autoresearch-scout et autoresearch-run, helper de mesure | L'agent pilote les expériences avec ses outils habituels |
| Pi original | originals/pi-autoresearch, copie exacte du paquet installé 1.8.1 | Extension Pi uniquement, outils et interface natifs |

Le scout d'origine est archivé dans originals/autoresearch-scout. Ces archives
sont présentes dans l'extraction complète, jamais dans les exports portables.
Le paquet racine ne charge PAS l'extension originale : son manifeste Pi ne
déclare que les skills. Les deux modes ne doivent pas piloter la même session.

Les sources originales sont préservées, pas auditées intégralement. Le moteur Pi
contient des opérations larges de restauration/nettoyage Git. Ne le charger que
dans un checkout jetable dédié après examen. Les garde-fous du mode portable ne
modifient pas rétroactivement ce moteur. Voir [la provenance](PROVENANCE.md).

## Utilisation commune à tous les agents

Un agent sachant lire des fichiers, éditer et exécuter des commandes peut recevoir :

> Lis skills/autoresearch-scout/SKILL.md dans ce toolkit. Prépare une baseline pour
> mon dépôt, sans optimiser ni commiter. Objectif : [objectif concret].

Puis, une fois la baseline approuvée :

> Utilise autoresearch-run. Maximum 5 expériences et 20 minutes ; uniquement les
> chemins autorisés dans .auto/prompt.md. Ne commite pas.

Remplacer les chemins par ceux de l'installation réelle. Les skills référencent
leurs ressources relativement à leur propre dossier, pas au checkout du toolkit.
Le fichier .auto/prompt.md doit contenir le périmètre et la méthodologie observés.
La préparation s'arrête avant toute optimisation. Pas de serveur MCP, d'appel
LLM, de clé ni de téléchargement requis par les helpers. L'agent et les commandes
du projet peuvent eux-mêmes nécessiter réseau, dépendances et autorisations.

## Adaptateurs et installation volontaire

Les manifestes du paquet racine référencent les mêmes sources skills/. Les
formats ont été vérifiés dans les documentations officielles, mais aucune
installation ni session LLM multi-hôte n'a été exécutée.

| Agent | Format fourni | Alternative locale |
| --- | --- | --- |
| Codex | .codex-plugin/plugin.json | Copier les deux dossiers de skills dans .agents/skills du projet |
| Claude Code | .claude-plugin/plugin.json | Charger le plugin local dans une nouvelle session |
| Cursor | .cursor-plugin/plugin.json | Copier les deux dossiers dans .cursor/skills du projet |
| Pi | package.json avec pi.skills | Charger le paquet de skills via Pi |
| Autres | plugin.json Agent Plugins + SKILL.md | Fournir les instructions manuellement si le format n'est pas supporté |

Choisir UN mode de découverte par agent pour éviter les doublons de noms.
Ne pas écraser un scout existant lors d'une installation ; examiner le diff et
conserver l'original. Aucune configuration globale ou marketplace n'est modifiée
par cette extraction.

Pour produire des paquets séparés, depuis l'extraction complète :

```sh
python3 scripts/export.py --agent codex --output dist/codex/autoresearch-toolkit
python3 scripts/export.py --agent claude --output dist/claude/autoresearch-toolkit
python3 scripts/export.py --agent cursor --output dist/cursor/autoresearch-toolkit
python3 scripts/export.py --agent pi --output dist/pi/autoresearch-toolkit
python3 scripts/export.py --agent generic --output dist/generic/autoresearch-toolkit
```

L'export refuse une destination existante, y compris un lien symbolique ; il ne
fusionne pas avec un agent installé. En cas d'erreur disque, une sortie partielle
peut rester pour inspection. Déplacer/inspecter cette sortie avant de réessayer.
Les exports ne contiennent pas l'exporteur ni les tests : les maintenir ici.
Seuls les fichiers déclarés dans `PORTABLE_FILES` de `scripts/export.py` et le
manifeste de l'agent sont distribués. Tout ajout de ressource à un skill doit
être examiné puis ajouté à cette liste. Les fichiers locaux supplémentaires,
y compris `.env`, `.auto/`, clés et caches, sont omis sans dépendre du `.gitignore`.
Chaque composant des chemins sources sous la racine est vérifié : un lien
symbolique, une ressource absente ou de type incorrect bloque l'export avant
création de la destination. Une destination dans `skills/` est également refusée.
La liste contrôle les chemins distribués ; examiner aussi le contenu des fichiers
autorisés avant diffusion. Les sources doivent rester stables pendant la copie.

Exemple de chargement temporaire Claude Code, depuis le projet à travailler
(à lancer volontairement, peut démarrer une session avec votre fournisseur LLM) :

```sh
claude --plugin-dir /chemin/vers/autoresearch-toolkit
```

Pour les autres hôtes, utiliser le gestionnaire de plugins/paquets de la version
installée ou la découverte locale ci-dessus. Pas de promesse de support natif
pour un agent inconnu : le mode manuel reste le dénominateur commun.

## Mesure et limites

Python 3.10+, bibliothèque standard uniquement ; helper de mesure pour macOS/Linux
(WSL pour Windows). Les tests synthétiques n'exécutent pas les tests de l'IHM.

```sh
python3 -m unittest discover -s tests -v
python3 skills/autoresearch-scout/scripts/measure.py --name bench_ms --runs 3 --warmup 1 --timeout 5 --budget 20 -- python3 -c 'sum(range(10000))'
```

Le helper mesure le temps mural d'une commande, warmups exclus, médiane et bornes
incluses. Il ne remplace pas un microbenchmark : lancement de processus et attente
ajoutent du bruit pour les workloads très courts. Il ne mesure pas directement
la mémoire, la taille ou le débit ; ces métriques demandent un measure.sh adapté.
Sur échec/timeout, aucun METRIC n'est publié. Les sorties de la commande vont sur
stderr. Un timeout/interruption tue le groupe de processus lancé ; un programme
qui se détache volontairement peut s'y soustraire. Ce n'est PAS une sandbox.

Les limites de mesure sont appliquées par le code. Le périmètre d'édition, la
protection des tests, les décisions et le budget global de la boucle sont des
consignes exécutées par l'agent, pas un contrôleur autonome inviolable. Pas de
hooks de relance, de reprise automatique après quota ni de dashboard dans le
mode portable. Une reprise requiert une nouvelle invocation explicite.

## Versionnement

Versionner skills/, scripts/, tests/, manifestes et provenance dans un dépôt dédié.
Les .auto/ des projets sont locales et ignorées ; ne pas exporter les sessions,
clés, conversations, configurations de fournisseurs ou données de benchmark.
Les fichiers personnalisés sont sous [licence MIT](LICENSE), copyright 2026
metinam. Chaque export inclut cette licence. La copie du moteur conserve sa
propre licence MIT et ses attributions dans `originals/pi-autoresearch/LICENSE`.
Voir [l'audit avant mise en dépôt](AUDIT.md) dans l'extraction complète.

## Sources des formats

Vérifiées le 14 septembre 2026 :

- [Codex skills](https://learn.chatgpt.com/docs/build-skills) et [plugins](https://learn.chatgpt.com/docs/build-plugins).
- [Claude Code plugin structure](https://github.com/anthropics/claude-code/blob/main/plugins/plugin-dev/skills/plugin-structure/examples/minimal-plugin.md) et [chargement local](https://github.com/anthropics/claude-code/blob/main/plugins/plugin-dev/commands/create-plugin.md).
- [Cursor skills](https://cursor.com/docs/skills) et [plugins](https://cursor.com/docs/plugins).
- [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md).
