# Plan d’implémentation — Autoresearch Toolkit

Statut : proposition d’architecture et de séquencement, avant implémentation.
Référence de départ : commit `130f305`, version du toolkit `0.1.0`.
Responsable du projet et des nouvelles contributions : Metimer.

## 1. Objectif et résultat attendu

Transformer le toolkit en un système d’expérimentation utilisable depuis une CLI
ou un agent, avec un moteur qui exécute les vérifications, mesure les candidats,
applique les règles d’acceptation et conserve les preuves de ses décisions.

Le parcours cible est le suivant :

1. Préparer un objectif, des commandes, un périmètre et un budget.
2. Qualifier une référence reproductible.
3. Demander à l’agent une hypothèse et un changement dans un espace dédié.
4. Vérifier, mesurer et décider à partir des résultats produits par le moteur.
5. Répéter dans les limites autorisées, puis exporter les améliorations retenues.

L’agent reste responsable des hypothèses et des modifications proposées. Le cœur
ne dépend d’aucun fournisseur LLM et peut tester un patch préparé manuellement.
Les crédits des composants repris restent distribués avec ces composants.

## 2. Point de départ vérifié

| Composant | État actuel | Conséquence pour la migration |
| --- | --- | --- |
| `skills/autoresearch-scout` | Préparation portable et helper Python borné | Conserver le mode autonome, ajouter un parcours piloté par le moteur. |
| `skills/autoresearch-run` | Boucle décrite par des consignes | Transférer l’exécution et les décisions vérifiables au cœur. |
| `scripts/export.py` | Liste explicite de ressources, cinq formats | Conserver ces protections et distinguer exports de skills et distributions du moteur. |
| `tests/test_toolkit.py` | 23 tests réussis lors de la dernière validation | Garder cette suite pendant la migration. |
| Moteur Pi archivé | Exécution, Git, état, métriques et interface réunis dans un `index.ts` de plus de 3 000 lignes | Extraire progressivement les fonctions utiles, sans charger l’extension dans le cœur. |
| `jsonl.ts` archivé | Reconstruction permissive ; statut inconnu interprété comme `keep` | Écrire un schéma strict et un importeur historique distinct. |
| Décision Pi | `log_experiment` reçoit métrique et statut de l’agent | Lier la décision à une exécution identifiée et aux preuves du moteur. |
| Bruit Pi | MAD calculée sur les métriques de plusieurs candidats du segment | Mesurer le bruit avec des répétitions du même état. |
| Fonctions Pi existantes | Hooks, limites de reprise, dashboards, métriques secondaires | Réutiliser les présentations et concepts utiles ; tester les garanties avant portage. |

Sources locales : `originals/pi-autoresearch/extensions/pi-autoresearch/`,
`originals/pi-autoresearch/README.md`, `skills/`, `scripts/` et `tests/`.
Les constats constituent une lecture ciblée, pas une certification du moteur archivé.

## 3. Décisions d’architecture proposées

### Socle technique

- Cœur en TypeScript strict, compilé en JavaScript ESM et déclarations de types.
- Node.js 22 comme minimum de départ ; tester aussi Node.js 24. Fixer les versions
  mineures et dépendances réellement utilisées au lot 1, puis les consigner dans
  un lockfile unique.
- Trois paquets locaux : `core`, `cli`, `adapter-pi`. La racine reste privée.
  Les noms publiables seront vérifiés avant publication, sans réserver de nom
  ni déclencher de téléchargement à l’exécution.
- Tests TypeScript compilés, exécutés avec le runner Node ; tests Python conservés.
- Cœur sans dépendance Pi, sans SDK LLM et sans serveur nécessaire. La validation
  runtime des contrats est centralisée ; les types TypeScript seuls ne valident
  jamais les entrées externes. JSON Schema documente le contrat, avec tests de parité
  entre schéma et validation runtime.
- Linux et macOS en première version ; WSL documenté. Windows natif reste hors
  du périmètre tant que la supervision de processus n’y est pas testée.

### Isolation

Le mode par défaut utilise un dépôt expérimental indépendant, créé localement
avec objets indépendants, sans alternates ni hardlinks et sans distant configuré.
Il fixe un commit source et n’importe aucun changement non commité implicitement.
Un projet sale reste exploitable à partir de son HEAD si ce choix est explicite ;
l’inclusion de modifications locales exige un snapshot déclaré et validé.

Les worktrees pourront devenir une optimisation ultérieure. Ils partagent une
partie des métadonnées du dépôt : ils ne constituent pas la séparation retenue
par défaut. [Documentation Git : worktrees](https://github.com/git/htmldocs/blob/gh-pages/git-worktree.adoc)

Un seul candidat est actif par session. Chaque candidat repart d’un snapshot de
l’état accepté. Un rejet conserve ses preuves et abandonne son espace ; il ne
réinitialise pas le checkout utilisateur. Le nettoyage est une opération distincte,
bornée aux répertoires possédés par la session et interdite si leur identité est douteuse.

### Limite de confiance

Un dépôt séparé protège contre les opérations Git accidentelles du moteur. Il
n’empêche pas une commande arbitraire, exécutée avec les droits de l’utilisateur,
d’accéder au reste de la machine. Les contrôles de fichiers détectent les écarts
avant/après ; ils ne prouvent pas l’absence d’une modification transitoire malveillante.

Les garanties de la première version portent donc sur les opérations du moteur
et sur la validité des candidats qu’il accepte. L’exécution de code non fiable
avec blocage du réseau ou des accès externes nécessite un backend isolé séparé.

## 4. Organisation cible

```text
packages/
  core/
    src/
      contracts/       # configurations, événements, résultats et erreurs
      session/         # transitions, budgets, orchestration
      storage/         # journal, snapshots, verrous, reprise
      workspace/       # dépôt expérimental, snapshots, patches
      policy/          # périmètre, fichiers protégés, invariants
      execution/       # processus, sorties, annulation
      measurement/     # collecte, protocole, comparaisons
      decisions/       # acceptation, rejet, indécision
      history/         # index et recherche d’expériences
      reports/         # restitution et export des résultats
  cli/
    src/               # commandes et présentation texte/JSON
  adapter-pi/
    src/               # outils Pi, événements, vues
schemas/               # contrats JSON publics versionnés
skills/                # skills existants et intégration moteur
scripts/export.py      # exports portables existants
tests/
  test_toolkit.py
  fixtures/            # projets synthétiques, sans données privées
  integration/
  recovery/
docs/
  IMPLEMENTATION_PLAN.md
  architecture.md
  cli.md
  migration.md
  compatibility.md
THIRD_PARTY_NOTICES.md
LICENSE
originals/             # références conservées, non chargées
```

Les sorties compilées sont ignorées par Git et construites pour les distributions.
Les rapports locaux d’audit restent ignorés ; les notices d’attribution, contrats
et guides utiles aux utilisateurs sont versionnés.

## 5. Contrats, état et cycle d’une expérience

### Configuration immuable après qualification

`SessionConfig` contient au minimum :

- Version du schéma, identifiant de session, objectif et identité du dépôt source.
- Commit et empreinte du snapshot initial, méthode d’inclusion des changements locaux.
- Chemins autorisés, chemins protégés et sorties générées autorisées.
- Commandes sous forme `executable + argv`, répertoire relatif et environnement déclaré.
- Étapes de préparation des dépendances et autorisations réseau explicites.
- Métrique principale : nom, unité, direction, domaine de valeurs et seuil utile.
- Métriques secondaires : rôle informatif ou contrainte, unité et seuil.
- Protocole : répétitions, warmups, ordre de comparaison, cache, entrées et seeds.
- Vérifications obligatoires et invariants comportementaux.
- Budgets : tentatives, durée cumulée active, échéance UTC, délais par étape,
  quotas de sorties et d’artefacts. Budget LLM optionnel seulement si l’hôte le fournit.
- Politique de hooks, de conservation, d’export et de commits explicites.

Toute modification de méthode crée une nouvelle révision et invalide la référence
précédente. Elle ne remet pas silencieusement les budgets à zéro.

### États

Session : `created → qualifying → ready → active → paused/stopped/completed`.
Un incident nécessitant une intervention produit `recovery_required`.

Expérience : `prepared → editing → sealed → checking → measuring → evaluated`.
Les issues sont `kept`, `discarded`, `inconclusive`, `failed` ou `cancelled`.
`sealed` capture un artefact de candidat et une empreinte ; toute modification
ultérieure du code mesuré invalide la preuve et impose une nouvelle exécution.

Chaque commande mutante porte un identifiant d’opération. Un appel répété ne crée
ni seconde expérience ni double acceptation. La promotion de l’état accepté compare
la révision attendue avec la révision courante avant de la modifier.

### Séparation des faits et commentaires

- Le moteur enregistre les codes de sortie, mesures brutes, hashes et décisions.
- L’agent ajoute une hypothèse et des commentaires, identifiés comme tels.
- Un verdict transmis par l’agent ne peut pas remplacer une décision du moteur.
- L’absence de vérifications, d’une métrique requise ou de preuve valide interdit `kept`.
- Les timestamps, identifiants et formats inconnus ne reçoivent aucun défaut favorable.

## 6. Stockage, concurrence et récupération

L’état géré par le moteur est placé dans `.auto/engine/sessions/<id>/` du projet
pilote, hors du code candidat. Les fichiers `.auto/prompt.md` et `context.md`
restent des documents de travail ; les contrats structurés font autorité.

```text
session.json                 # contrat et identité
events.jsonl                 # événements séquencés, append-only
state.json                   # projection reconstruisible
lock/                        # propriétaire, nonce, identité du processus
control/                     # demandes d’arrêt adressées au superviseur
artifacts/<hash>/            # patches, snapshots, résultats bornés
experiments/<id>/            # manifeste et preuves de chaque essai
workspaces/                  # dépôt expérimental et candidats possédés
```

- Un rédacteur par session ; un verrou distinct empêche deux benchmarks simultanés
  dans le même groupe de ressources de mesure.
- Acquisition atomique, identifiant aléatoire, hôte, PID et identité de démarrage.
  Un PID seul ne suffit pas à prouver qu’un verrou est périmé.
- Les requêtes d’arrêt sont écrites atomiquement et consommées par le superviseur,
  y compris entre deux échantillons. Aucun service permanent n’est nécessaire.
- Écrire les artefacts avant l’événement qui les référence ; synchroniser le journal
  avant de publier une transition ; remplacer les projections par renommage atomique
  dans le même système de fichiers. Définir et tester les garanties de durabilité.
- Rejouer les événements pour reconstruire l’état. Une dernière ligne tronquée est
  signalée et conservée pour inspection ; une corruption intermédiaire bloque la reprise.
- Après crash, un essai en cours devient interrompu, jamais accepté. Si l’identité
  d’un processus survivant est incertaine, bloquer la reprise au lieu de tuer un PID au hasard.
- Distinguer verrou de session, verrou de benchmark et verrou Git. Les fichiers
  locaux sur NFS et l’exécution distribuée sont hors périmètre initial.

## 7. Exécution, périmètre et budgets

Le runner lance directement un exécutable avec ses arguments. Un shell est un
choix explicite, par exemple `bash` avec un script déclaré et protégé. Les commandes
internes Git neutralisent les variables de redirection de dépôt et les hooks/configs
externes susceptibles de modifier leur comportement ; elles n’utilisent pas le
répertoire courant implicite de l’agent.

Chaque exécution possède un groupe de processus POSIX, des sorties en streaming
bornées et une séquence d’arrêt TERM puis KILL après un délai court. Il faut attendre
la terminaison et traiter aussi les descendants qui gardent les sorties ouvertes.
L’arrêt du seul parent ne suffit pas. [Documentation Node.js : child processes](https://nodejs.org/docs/latest-v22.x/api/child_process.html)

Le coût de préparation, checks, hooks, warmups, mesures et re-mesures est imputé
au budget. À la reprise, conserver la durée déjà consommée et l’échéance initiale.
L’horloge monotone mesure les durées du processus courant ; les incohérences d’horloge
entre reprises bloquent l’extension involontaire du budget. Un arrêt forcé peut
dépasser l’échéance du seul délai de terminaison déclaré.

La vérification de périmètre couvre ajouts, suppressions, renommages, changements
de mode et liens symboliques. Normaliser les chemins, refuser les traversées et
les collisions de casse pertinentes. Les liens vers l’extérieur, sous-modules et
filtres Git non gérés sont refusés avec une explication en première version.
Les sorties de build permises sont séparées du code candidat et exclues du patch.

Les tests, fixtures, scripts de mesure, configurations et dépendances sont protégés
par défaut. Une exception doit être dans le contrat avant qualification ; une
modification des vérifications de référence ne vaut pas une amélioration comparable.
Comparer le contenu et les résultats, pas uniquement le nombre de tests.

## 8. Mesure et politique d’acceptation

### Première politique, simple et explicable

- Séparer temps mural d’une commande et métrique rapportée par un benchmark.
- Fixer protocole, unités, domaine et précision ; rejeter doublons ambigus,
  métriques manquantes, NaN et infinis. Zéro et valeurs négatives dépendent du domaine.
- Qualifier la référence sur plusieurs séries et conserver les échantillons bruts.
- Comparer le candidat au meilleur état accepté, mesuré de nouveau dans les mêmes
  conditions. Alterner l’ordre référence/candidat selon un plan équilibré enregistré.
- Utiliser un nombre d’échantillons fixé avant comparaison, un minimum de cinq
  paires pour les workloads courts et un seuil d’amélioration utile explicite.
  Un protocole plus court pour workload coûteux doit être déclaré et qualifié.
- Estimer le bruit sur des répétitions à code constant. Si le gain ne dépasse pas
  le seuil utile et la marge de bruit déclarée, produire `inconclusive`.
- Ne pas appeler ce critère une probabilité statistique. Pour les résultats limites,
  autoriser une seule confirmation de taille prédéfinie, dans le budget restant.
- Réexécuter le meilleur résultat sur une série finale distincte avant de présenter
  un gain confirmé. Sans budget de confirmation, le résultat reste provisoire.

### Approfondissement ultérieur

Introduire les intervalles sur différences appariées, la gestion des comparaisons
multiples et un protocole adapté aux tests successifs seulement après validation
statistique documentée et simulations sur données synthétiques. Aucun rééchantillonnage
illimité jusqu’à obtenir un résultat favorable.

Les contraintes secondaires sont appliquées dès la première version qui les expose :
une vitesse améliorée avec une limite mémoire dépassée reste rejetée. La gestion
d’un ensemble de compromis multi-objectifs vient après le moteur à objectif principal.

## 9. API et commandes cibles

Les noms suivants sont proposés ; ces commandes n’existent pas encore.

| Commande | Effet et condition |
| --- | --- |
| `autoresearch doctor` | Vérifie les capacités locales et les prérequis, sans installer de dépendances. |
| `autoresearch init --config <fichier>` | Valide le contrat et prépare une session et son espace déclaré. |
| `autoresearch baseline --session <id>` | Exécute les vérifications et qualifie la référence. |
| `autoresearch prepare --session <id> --hypothesis <texte>` | Réserve une tentative et renvoie le chemin candidat et ses règles. |
| `autoresearch seal --experiment <id>` | Capture le candidat, son patch et ses empreintes après les éditions. |
| `autoresearch run --experiment <id>` | Exécute le candidat scellé, calcule la décision et promeut un gain valide. |
| `autoresearch status --session <id>` | Affiche état, budget, preuves et action suivante. |
| `autoresearch stop --session <id>` | Demande l’annulation à son superviseur et empêche une nouvelle tentative. |
| `autoresearch resume --session <id>` | Réconcilie les preuves et les budgets ; ne démarre pas seul une hypothèse. |
| `autoresearch history --session <id>` | Recherche les expériences et leurs raisons de décision. |
| `autoresearch report --session <id>` | Produit une synthèse locale à partir des faits du journal. |
| `autoresearch export-result --session <id> --output <dossier>` | Exporte patch, preuves sélectionnées et reproduction dans un nouveau dossier. |
| `autoresearch import-legacy --source <chemin>` | Importe une session ancienne dans une nouvelle session, sans activer la boucle. |
| `autoresearch gc --session <id> --dry-run` | Liste les espaces possédés pouvant être nettoyés ; suppression séparée explicite. |

Chaque commande propose une sortie JSON stable et des codes de sortie documentés :
usage/configuration, conflit d’état, prérequis indisponible, échec d’exécution,
timeout/annulation et corruption. Une expérience rejetée est un résultat métier,
pas une erreur de transport. Les commandes mutantes acceptent une clé d’idempotence.

L’API du cœur expose les mêmes opérations sans dépendre de la CLI. Aucun appel
`log(status=keep, metric=...)` n’est exposé comme autorité d’acceptation.

## 10. Lots d’implémentation et critères de sortie

### Lot 0 — Caractériser et attribuer

Travaux : cartographier les fonctions reprises, créer `THIRD_PARTY_NOTICES.md`,
préserver `originals/`, documenter les différences intentionnelles et capturer
des fixtures historiques anonymisées ou synthétiques. Tester parseurs, segmentation,
directions, diagnostics et reconstruction sans lancer une session LLM.

Sortie : chaque reprise a une attribution ; chaque comportement conservé ou changé
est identifié. Le défaut « statut inconnu → keep » possède un cas de test négatif.
La liste de compatibilité historique est écrite avant le portage.

### Lot 1 — Poser les paquets et les contrats

Dépendance : lot 0. Travaux : build TypeScript, lockfile, paquets core/cli/Pi,
types et validateurs, codes d’erreur, schémas, importations publiques et CI minimale.

Sortie : import du cœur sans Pi installé ; build reproductible ; entrées invalides
refusées ; compilation et tests Python existants verts. Une commande de diagnostic
en lecture seule fonctionne depuis un dossier extérieur au dépôt.

### Lot 2 — État durable et budgets

Dépendance : lot 1. Travaux : transitions, journal, snapshots, verrous, identifiants
d’opérations, projection reconstruisible, comptabilité des tentatives et du temps.

Sortie : deux rédacteurs ne peuvent pas acquérir la même session ; répétition d’une
opération sans double effet ; crash à chaque frontière d’écriture récupérable ou
signalé ; aucun budget renouvelé implicitement à la reprise.

### Lot 3 — Dépôt expérimental et politique de fichiers

Dépendances : lots 1–2. Travaux : copie Git indépendante, identité de la source,
snapshot de départ et accepté, espaces candidats, inventaire des fichiers,
contrôle des chemins et protection des vérifications. Préparer un protocole de
patch incluant nouveaux fichiers, suppressions, renommages, binaire et modes.

Sortie : les essais ne changent ni HEAD, ni index, ni refs, ni fichiers du dépôt
source par leurs opérations internes. Test avec changements utilisateur préexistants.
Un candidat hors périmètre ou contenant un lien externe ne peut pas être scellé.
L’export d’un patch reproduit exactement le snapshot accepté dans une copie neuve.

### Lot 4 — Supervision de processus et quotas

Dépendances : lots 2–3. Travaux : runner argv, environnement déclaré, groupes POSIX,
annulation, délais, sorties bornées, stockage limité, absence de shell implicite.
Déclarer les préparations de dépendances avec caches privés ou explicitement partagés ;
aucune installation automatique déduite d’un manifeste.

Sortie : tests réels parent/enfant, descendant gardant stdout ouvert, commande qui
ignore TERM, absence d’exécutable, sortie massive, disque plein simulé et interruption.
Une commande triviale fonctionne hors du dépôt du toolkit. Un budget épuisé interdit
le lancement suivant ; l’arrêt termine les processus possédés dans la marge prévue.

### Lot 5 — Référence et comparaisons

Dépendance : lot 4. Travaux : parseur strict, mesures brutes, qualification, warmups,
politique de cache, alternance référence/candidat, seuils, contraintes secondaires
et collecte de l’environnement. Introduire le verrou de benchmark local.

Sortie : workloads déterministes et bruités, valeurs invalides et unités incohérentes
couverts. La variance de candidats différents n’entre pas dans l’estimation du bruit.
Un environnement ou workload changé invalide la référence ; résultat indécis explicite.

### Lot 6 — Orchestration et acceptation

Dépendances : lots 2–5. Travaux : cycle prepare/seal/run, checks obligatoires,
contrôle d’intégrité avant/après, liaison résultat-candidat-référence, promotion
atomique et export sans commit implicite. Conserver les preuves des rejets.

Sortie : scénario complet baseline → candidat meilleur → candidat régressif →
candidat hors périmètre → arrêt. Seul le premier candidat valide est retenu.
Un résultat d’un ancien candidat ou une requête répétée ne peut pas promouvoir
un autre état. Une interruption après décision mais avant projection se reconstruit.

### Lot 7 — CLI et skills

Dépendance : lot 6. Travaux : commandes publiques, JSON stable, aide, diagnostics,
intégration du parcours moteur dans les deux skills et documentation des modes.
Le mode portable actuel reste utilisable sans Node ; absence du moteur annoncée,
sans téléchargement ni bascule silencieuse depuis une session moteur active.

Sortie : un utilisateur teste un patch sans Pi ni LLM. Les skills distinguent
préparation, autorisation d’optimiser et budget. Tests d’acceptation depuis un
répertoire neuf avec chemins contenant espaces et caractères non ASCII.

### Lot 8 — Migration et reprise

Dépendances : lots 2 et 7. Travaux : import du journal Pi et du journal portable,
détection des versions, conversion dans une nouvelle session, rapport d’anomalies,
reprise contrôlée après timeout et fermeture de l’hôte.

Sortie : fichiers anciens inchangés ; aucun script importé exécuté automatiquement ;
résultats importés marqués historiques sans preuve actuelle. Toute session importée
requalifie sa référence avant optimisation. Rejet des statuts inconnus et données
invalides ; aucune promesse de reprise des changements non committés sans snapshot.

### Lot 9 — Adaptateur Pi et hooks

Dépendances : lots 7–8. Travaux : outils préfixés pour éviter les collisions avec
le moteur original, annulation et cycle de vie transmis au cœur, vues fondées sur
ses événements. Garder l’auto-reprise désactivée par défaut ; une activation explicite
reste soumise aux mêmes budgets et limites après reprise.

L’API Pi fournit un signal d’annulation aux outils et des événements de session ;
l’adaptateur devra les relier à la session moteur correspondante, sans dupliquer
la décision. [Documentation Pi : extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)

Hooks : contrats before/after versionnés, runner et budgets communs, sorties bornées,
fichiers de hooks protégés. Un hook ne peut ni réécrire la méthode ni forcer `kept`.
Échec du before bloque l’essai ; échec du after est enregistré sans effacer le verdict.

Sortie : chargement réel sur une version Pi explicitement testée ; propagation
de stop/shutdown ; absence de double boucle ; hooks bloqués ou interrompus sans
processus ordinaires restant actifs. Les versions Pi compatibles sont bornées,
sans peerDependencies génériques `*` dans la nouvelle distribution.

### Lot 10 — Mémoire et résultats partageables

Dépendances : lots 6–8. Travaux : index local reconstruisible, recherche par
chemins/hypothèses/motifs d’échec, détection de doublons exacts par empreinte du
patch, parent accepté et protocole. Proximité textuelle informative, jamais motif
de rejet automatique d’une nouvelle hypothèse.

Ajouter rapports Markdown/JSON et paquet de résultat : base, patch cumulatif,
mesures, limites et commandes de reproduction. Logs bruts, chemins personnels,
variables d’environnement et données d’entrée sont exclus par défaut ; seules
les preuves explicitement sélectionnées sont exportées. Prévisualisation disponible.

Sortie : un résultat se reproduit dans une copie vierge ; un changement de base
ne produit pas de faux doublon ; absence de données privées synthétiques dans
les exports. Le rapport distingue gains exploratoires et confirmation finale.

### Lot 11 — Distribution et qualification de version

Dépendances : lots 0–10. Travaux : construire les artefacts compilés, inclure
licences et notices, adapter l’exporteur avec inventaires explicites par profil,
tests depuis les paquets fabriqués, guide d’installation et matrice de compatibilité.

Deux profils explicites : skills seuls, et moteur avec CLI/adaptateur sélectionné.
Le nom et le contenu annoncés d’un export correspondent réellement à son profil.
La publication du paquet racine privé n’est pas activée par inadvertance.

Sortie : tous les scénarios de la section 11 passent ; deux projets de démonstration
reproductibles fonctionnent ; aucune dépendance au checkout de développement ou aux
rapports ignorés ; aucun appel réseau implicite à l’import. Préparer une version
candidate et ses notes. La publication et le push restent une action distincte.

### Lot 12 — Extensions après stabilisation

Dépendance : lot 11. Travaux indépendants à prioriser selon les usages observés :

- Comparaisons statistiques avancées et objectifs avec compromis explicites.
- Vue HTML locale et comparaison de sessions ; aucune commande mutante exposée
  par défaut sur un serveur, échappement des sorties et écoute locale si serveur.
- Backend d’exécution isolé avec environnement reproductible, réseau et ressources
  réellement contraints. Validation séparée des effets de ce backend sur les mesures.
- Ordonnancement de plusieurs sessions : génération de candidats éventuellement
  simultanée, benchmarks séquentiels par défaut pour éviter leur interférence.
- Adaptateurs natifs supplémentaires seulement lorsqu’ils apportent plus que la CLI.
- Support Windows natif après implémentation et tests d’arrêt de l’arbre de processus.

Chaque extension reçoit son propre contrat, ses critères de sortie et sa revue
avant d’élargir les garanties annoncées.

## 11. Matrice de validation

| Domaine | Vérification exigée |
| --- | --- |
| Contrats | Versions inconnues, champs invalides, limites négatives/non finies, états illégaux. |
| Git | Source propre/sale, index partiellement préparé, nouveaux fichiers, suppressions, binaires, modes, liens, noms inhabituels. |
| Fichiers | Fichiers protégés altérés, renommage vers une zone interdite, sorties générées, collisions de casse, liens externes. |
| Processus | Échec, timeout, SIGINT, descendants, TERM ignoré, sorties illimitées, limites disque. |
| Budget | Checks et warmups imputés, fin de budget entre échantillons, reprise sans remise à zéro, horloge incohérente. |
| Mesures | Référence instable, régression, gain clair, gain indécis, zéro autorisé/interdit, métriques manquantes ou dupliquées. |
| Concurrence | Deux rédacteurs, deux benchmarks, requête répétée, tentative de promotion sur une base périmée. |
| Crash | Arrêt injecté avant/après écriture d’artefact, événement, promotion et projection ; dernière ligne tronquée. |
| Migration | Pi ancien, portable ancien, session vide, corruption, statut inconnu, absence de preuve de code. |
| Adaptateurs | Cœur sans Pi, CLI hors dépôt, Pi réel, arrêt de session, dépendances et versions déclarées. |
| Distribution | Installation depuis artefact, licence/notices présentes, rapports privés exclus, aucune ressource locale requise. |

Ne pas utiliser des seuils de vitesse serrés pour faire réussir la CI. Tester les
décisions sur données déterministes ; réserver les mesures de performance réelles
aux scénarios de qualification avec bruit enregistré. Tester les arrêts de processus
avec de vrais processus, en complément des mocks.

Matrice initiale proposée : Linux et macOS, Node.js 22 et 24 ; Python 3.10 et une
version plus récente fixée en CI. WSL possède un smoke test documenté séparément.
La version Pi testée et sa version minimale sont consignées avant release.

## 12. Jalons, effort et ordre des commits

| Jalon | Lots | Produit vérifiable |
| --- | --- | --- |
| A — Fondations | 0–4 | Cœur testable, dépôt expérimental, stockage, budgets et runner. |
| B — Moteur utilisable | 5–7 | Expérience complète via CLI et skills, sans Pi requis. |
| C — Intégration | 8–10 | Migration, Pi, hooks, mémoire et résultats exportables. |
| D — Version candidate | 11 | Paquets installables, documentation et qualification. |
| E — Élargissement | 12 | Capacités avancées retenues selon les usages. |

Chemin critique : `0 → 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9/10 → 11`.
Certaines définitions de contrats, fixtures et vues peuvent être préparées en
parallèle, mais leurs intégrations respectent les dépendances. Ce plan ne lance
aucun travail délégué automatiquement.

Ordre de grandeur indicatif pour A à D : 35–55 jours de travail d’une personne,
incluant tests et corrections d’intégration, hors lot 12. À réestimer après les
lots 0–1 : l’état des dépendances Pi, la fidélité des patches et la récupération
de processus sont les principales sources d’incertitude. Ce n’est pas un engagement
de calendrier ni une estimation de temps d’exécution d’un agent.

Prévoir un lot reviewable par capacité : caractérisation, contrats, journal,
isolation, runner, mesure, décision, CLI, migration, Pi/hooks, historique/export,
distribution. Scinder les lots volumineux pour éviter les commits mélangeant
refactoring, nouveaux comportements et modifications d’interface.
Identité Git conservée : `Metimer <metinamerwane@gmail.com>`, sans co-auteur.
Les attributions des sources reprises restent dans les notices et licences.

## 13. Risques et arbitrages

| Risque | Traitement prévu |
| --- | --- |
| Dupliquer deux moteurs divergents | Conserver l’archive comme référence ; un seul nouveau cœur fait autorité pour le mode moteur. |
| Coût disque des candidats | Quotas, inventaire des espaces possédés, collecte explicite ; mesurer avant d’introduire des worktrees partagés. |
| Dépendances coûteuses ou générées | Préparation déclarée, caches identifiés, empreintes ; ne pas confondre temps de setup et métrique cible. |
| Faux gains dus au bruit | Comparaison appariée, protocole fixé, seuil utile, résultat indécis et confirmation finale. |
| Faux sentiment de sandbox | Documenter le modèle de confiance ; backend isolé livré et testé séparément. |
| Reprise ambiguë | Ne pas promouvoir, conserver les preuves et demander une intervention ciblée. |
| Dérive API Pi | Adaptateur mince, versions bornées et test de chargement réel avant release. |
| Compromis mémoire/vitesse caché | Contraintes secondaires contractuelles, arrêt de l’acceptation si elles manquent. |
| Journaux sensibles | Sorties bornées, environnement sélectionné, export par inventaire et sans logs bruts par défaut. |

## 14. Définition de « terminé »

La première version complète est prête lorsque :

1. Une session de bout en bout fonctionne depuis la CLI et depuis l’adaptateur Pi
   sans logique de décision dupliquée.
2. Une expérience ne peut être retenue sans checks valides, métriques conformes,
   périmètre respecté, référence comparable et preuves liées au candidat scellé.
3. Les scénarios de crash, annulation, double appel et budget épuisé passent sur
   les plateformes annoncées.
4. Le dépôt utilisateur est préservé par toutes les opérations internes de la suite
   et les limites face à des commandes non fiables sont clairement documentées.
5. Un patch accepté se reproduit depuis l’artefact distribué dans un dépôt neuf.
6. Les skills seuls restent utilisables et les sessions historiques s’importent
   sans écrasement ni lancement automatique.
7. La version candidate contient documentation, versions compatibles, licences,
   attributions et notes de migration ; ses paquets sont testés après fabrication.

Le premier travail d’implémentation sera le lot 0, puis les contrats du lot 1.
Ce document prépare cette exécution ; il ne modifie pas encore le moteur.
