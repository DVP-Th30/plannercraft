# Assistant IA : se connecter, ce qui est envoyé

Guide court de l'assistant intégré (version « ma propre clé », lecture seule). Référence technique : `docs/features/assistant.md`, ADR-0024 (`docs/decisions.md`).

## 1. Ce que fait l'assistant

Vous lui posez une question en langage courant sur le planning ouvert : « Quand finit le projet ? », « Pourquoi la tâche 12 commence-t-elle le 4 janvier ? », « Quelles tâches sont critiques ? ». Il lit le planning avec les mêmes commandes que l'application (résumé du projet, liste des tâches, une tâche, chemin critique, « Pourquoi cette date ? », ressources, écarts à la ligne de base, tâche sélectionnée) et répond.

- **Il ne modifie rien.** Pour changer le planning, il vous dit quoi faire ; vous le faites dans l'application.
- **Il ne calcule pas les dates.** Les dates, durées et marges qu'il cite viennent du moteur de PlannerCraft ; s'il ne trouve pas une information, il doit le dire.
- C'est une IA : relisez ses réponses comme celles d'un collègue pressé.

## 2. Ouvrir et activer

1. **Affichage → Fractionner l'affichage → Assistant IA** (ou menu Affichage → Assistant IA) : le volet s'ouvre à droite, onglet « Assistant ».
2. Lisez la mention : *vous échangez avec une IA ; votre planning est envoyé à OpenRouter et au fournisseur du modèle choisi*.
3. Cliquez **Activer l'assistant**. Rien n'est envoyé avant ce clic et votre première question. « Désactiver l'assistant » (en bas du volet) revient en arrière.

## 3. Se connecter à OpenRouter

L'assistant passe par [OpenRouter](https://openrouter.ai), un service qui donne accès à plusieurs modèles d'IA. Il vous faut un compte OpenRouter avec des crédits : **OpenRouter vous facture vos questions**, PlannerCraft ne touche rien et ne voit pas vos échanges.

- **Se connecter à OpenRouter** : votre navigateur s'ouvre sur OpenRouter ; connectez-vous et autorisez PlannerCraft. Revenez à l'application : elle a reçu une clé rattachée à votre compte. Vous pouvez la plafonner ou la révoquer dans les réglages de clés d'OpenRouter.
- **Saisir une clé** : collez une clé créée vous-même sur openrouter.ai (elle commence par `sk-or-`), puis « Enregistrer la clé ».
- **Où est gardée la clé ?** Sur ordinateur, dans le trousseau du système (Trousseau de macOS, Gestionnaire d'identification de Windows, trousseau du bureau sous Linux) ; si le trousseau manque, pour la session seulement (un avertissement le dit). Dans la version web, en mémoire seulement : elle disparaît à la fermeture de l'onglet. Jamais dans les préférences de PlannerCraft ni dans vos fichiers.
- **Version web** : « Se connecter » quitte la page le temps de l'autorisation, puis la recharge. **Enregistrez votre projet avant**, ou utilisez « Saisir une clé », qui ne recharge rien.
- **Se déconnecter** efface la clé de l'application et du trousseau. Elle reste valable chez OpenRouter tant que vous ne l'y révoquez pas.

## 4. Poser une question

- Écrivez dans le champ « Question sur le planning », puis Entrée (Maj+Entrée pour aller à la ligne) ou **Envoyer**.
- La réponse s'écrit au fur et à mesure ; « L'assistant lit le planning… » s'affiche quand il consulte le projet, et une ligne « A lu le planning : … » dit ce qu'il a lu.
- **Arrêter** coupe la réponse en cours (et sa facturation chez la plupart des fournisseurs).
- Sous chaque réponse : **coût de la demande** annoncé par OpenRouter, en dollars, et nombre de jetons.
- **Modèle** : Claude Sonnet 5.5 (par défaut, le plus sûr), Claude Haiku 5.5 (beaucoup moins cher, plus simple), Gemini 3.8 Flash. Le choix est mémorisé.
- **Nouvelle conversation** repart de zéro (utile quand le sujet change : chaque question renvoie toute la conversation, donc coûte plus cher quand elle s'allonge).

## 5. Ce qui est envoyé

À chaque question partent chez OpenRouter, puis chez le fournisseur du modèle choisi (Anthropic pour Claude, Google pour Gemini) :

- vos questions et les réponses précédentes de la conversation ;
- **ce que l'assistant lit dans le planning** pour répondre : noms et dates des tâches, durées, marges, liens, ressources, notes d'une tâche consultée, nom et dates du projet, tâche sélectionnée.

PlannerCraft demande à OpenRouter de n'utiliser que des fournisseurs qui **ne conservent pas** vos données et **ne s'en servent pas pour entraîner** leurs modèles (options `zdr` et `data_collection: deny`). Le fichier du projet lui-même n'est jamais envoyé, ni les autres projets.

Si vos plannings contiennent des noms de personnes ou d'entreprises que vous ne voulez pas transmettre, n'activez pas l'assistant pour ces projets.

## 6. Messages d'erreur

| Message | Que faire |
|---|---|
| Clé refusée par OpenRouter | Reconnectez-vous, ou saisissez une autre clé (elle a peut-être été révoquée). |
| Crédits OpenRouter épuisés | Rechargez votre compte sur openrouter.ai. |
| Trop de demandes | Attendez le délai indiqué, puis renvoyez la question. |
| Connexion impossible | Vérifiez votre accès à Internet (ou le pare-feu de l'entreprise). |
| L'assistant a fait trop d'étapes | Posez une question plus simple ou plus précise. |
| Trousseau du système indisponible | La clé marche pour cette session ; il faudra la redonner au prochain lancement. |
