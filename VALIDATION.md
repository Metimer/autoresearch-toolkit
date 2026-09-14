# Validation locale — 14 septembre 2026

- 22 tests unittest réussis après correction de l'audit : médiane/warmups, erreurs, timeouts, budget,
  arguments invalides, séparation diagnostics/métriques, commande sans shell,
  nettoyage du groupe de processus simulé, non-écrasement et exports.
- Régressions d'export : fichiers privés supplémentaires exclus pour les cinq
  agents, liens symboliques refusés sur les ressources et leurs parents,
  ressources absentes ou invalides refusées avant création de la destination,
  destination dans les skills sources refusée. Licence MIT présente dans chaque export.
- Les cinq exports ont été produits en répertoires temporaires. Le helper de
  mesure a été exécuté depuis chacun avec un workload Python synthétique.
- Le manifeste Codex et les deux skills portables passent les validateurs des
  skills système plugin-creator et skill-creator. PyYAML est absent du python3
  courant ; l'environnement Anaconda existant a servi à ces validateurs uniquement.
  Les helpers et tests du toolkit restent sans dépendance Python tierce.
- Les copies originales du moteur et du scout sont identiques à leurs sources
  locales selon diff -qr, sans modification des fichiers installés.

Non validé : chargement réel dans Pi/Claude Code/Cursor/Codex, sélection des skills
par un LLM, efficacité des consignes sur des expériences réelles, compatibilité
du moteur Pi avec d'autres versions de Pi, parité avec un hypothétique autre
moteur custom non identifié. Les tests de terminaison du groupe sont simulés ;
les timeouts de commandes simples sont aussi testés avec de vrais processus.

Pas de benchmark IHM, de connexion d'inférence, d'installation d'agent, de
modification de marketplace, de commit, ni de publication effectués.
