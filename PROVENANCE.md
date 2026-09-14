# Provenance et périmètre

Extraction le 14 septembre 2026 depuis la machine de l'utilisateur.

- Moteur : ~/.pi/agent/npm/node_modules/pi-autoresearch, package.json annonce 1.8.1.
  Copie intégrale dans originals/pi-autoresearch, licence MIT conservée.
  Dépôt déclaré : https://github.com/davebcn87/pi-autoresearch.
- Scout : ~/.pi/agent/skills/autoresearch-scout, copie intégrale dans
  originals/autoresearch-scout, sans modification.
- La copie installée n'a pas été comparée à un tarball amont : ses éventuelles
  modifications custom sont conservées, mais leur nature n'est pas certifiée.
  Aucun autre moteur custom distinct n'a été identifié dans les chemins inspectés.
  Une personnalisation située ailleurs nécessitera une extraction supplémentaire.

Les exports portables ne contiennent pas originals/. Le moteur original n'est pas
une implémentation multiplateforme : il dépend des API/extensions de Pi et de leurs
peerDependencies, non embarquées ici. Il n'a pas été démarré pendant l'extraction.

Adaptations maintenues dans skills/ :

- Scout découplé de la délégation et des commandes Pi ; commandes du projet
  découvertes dans leurs sources, plutôt qu'un catalogue de commandes supposées.
- Pas de création de branche, commit, effacement de session ou démarrage de boucle
  implicite. Respect de /.auto/ ignoré, des changements utilisateur et des limites.
- Échantillonnage avec horloge monotone, warmups, médiane, timeout par commande,
  budget total et échec explicite ; pas de commande npm générique par défaut.
- Checks réellement exécutés, invariant de comportement et méthode figée :
  pas de squelette de checks vide retournant un succès.
- Boucle portable agent-driven distincte des outils et hooks natifs Pi ; journal
  portable séparé pour ne pas prétendre à une compatibilité de schéma Pi.

Le 14 septembre 2026, l'utilisateur a choisi la licence MIT pour les nouveaux
fichiers et le scout personnalisé. La licence racine `LICENSE` porte le copyright
2026 metinam, auteur déclaré dans les manifestes, et accompagne chaque export.
Le moteur tiers conserve sa licence et ses attributions propres dans
`originals/pi-autoresearch/LICENSE` (Tobi Lutke, David Cortés). Les archives n'ont
pas été modifiées lors de cette décision ; la licence racine ne remplace pas
les mentions de droits du moteur tiers.
