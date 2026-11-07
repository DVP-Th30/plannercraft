# Assistant IA : se connecter, ce qui est envoyé

Guide court de l'assistant intégré (version « ma propre clé »). Référence technique : `docs/features/assistant.md`, ADR-0024 (`docs/decisions.md`).

## 1. Ce que fait l'assistant

Vous lui posez une question en langage courant sur le planning ouvert : « Quand finit le projet ? », « Pourquoi la tâche 12 commence-t-elle le 4 janvier ? », « Quelles tâches sont critiques ? ». Il lit le planning avec les mêmes commandes que l'application (résumé du projet, liste des tâches, une tâche, chemin critique, « Pourquoi cette date ? », ressources, écarts à la ligne de base, tâche sélectionnée) et répond.

- **Il propose, vous décidez.** Il peut proposer des modifications (créer un planning, décaler, lier, renommer, affecter, mettre en forme) ; rien n'est appliqué sans votre accord (§ 5 bis).
- **Il ne calcule pas les dates.** Les dates, durées et marges qu'il cite viennent du moteur de PlannerCraft ; s'il ne trouve pas une information, il doit le dire.
- C'est une IA : relisez ses réponses comme celles d'un collègue pressé.

## 2. Ouvrir et activer

1. **Affichage → Fractionner l'affichage → Assistant IA** (ou menu Affichage → Assistant IA) : le volet s'ouvre à droite, onglet « Assistant ».
2. Lisez la mention : *les réponses sont générées par une IA et peuvent contenir des erreurs ; votre planning est envoyé à OpenRouter et au fournisseur du modèle*. Elle reste affichée tant que la conversation est vide.
3. Cliquez **Activer l'assistant**. Rien n'est envoyé avant ce clic et votre première question. « Désactiver l'assistant » (menu « … » en haut à droite du volet) revient en arrière et oublie la conversation du projet.

## 3. Se connecter à OpenRouter

L'assistant passe par [OpenRouter](https://openrouter.ai), un service qui donne accès à plusieurs modèles d'IA. Il vous faut un compte OpenRouter avec des crédits : **OpenRouter vous facture vos questions**, PlannerCraft ne touche rien et ne voit pas vos échanges.

- **Se connecter à OpenRouter** : votre navigateur s'ouvre sur OpenRouter ; connectez-vous et autorisez PlannerCraft. Revenez à l'application : elle a reçu une clé rattachée à votre compte. Vous pouvez la plafonner ou la révoquer dans les réglages de clés d'OpenRouter.
- **Saisir une clé** : collez une clé créée vous-même sur openrouter.ai (elle commence par `sk-or-`), puis « Enregistrer la clé ».
- **Où est gardée la clé ?** Sur ordinateur, dans le trousseau du système (Trousseau de macOS, Gestionnaire d'identification de Windows, trousseau du bureau sous Linux) ; si le trousseau manque, pour la session seulement (un avertissement le dit). Dans la version web, en mémoire seulement : elle disparaît à la fermeture de l'onglet. Jamais dans les préférences de PlannerCraft ni dans vos fichiers.
- **Version web** : « Se connecter » quitte la page le temps de l'autorisation, puis la recharge. **Enregistrez votre projet avant**, ou utilisez « Saisir une clé », qui ne recharge rien.
- **Se déconnecter** (menu « … ») efface la clé de l'application et du trousseau. Elle reste valable chez OpenRouter tant que vous ne l'y révoquez pas.

## 4. Poser une question

- Écrivez dans le champ « Question sur le planning », puis Entrée (Maj+Entrée pour aller à la ligne) ou le bouton rond ↑. Volet vide : cliquez une suggestion (« Quel est le chemin critique ? »…) pour la placer dans le champ.
- **Votre question monte en haut du volet et la réponse s'écrit dessous** : vous lisez depuis le début, la vue ne bouge pas. Si la réponse dépasse le bas du volet, une pastille « Suite de la réponse » apparaît : cliquez-la pour suivre la fin. Dès que vous faites défiler vous-même, l'application ne déplace plus la vue.
- La barre au-dessus du champ dit ce que fait l'assistant (« Réflexion… », « Lecture : le chemin critique… », « Rédaction… ») et depuis combien de secondes. Sous la question, les lignes « A lu : … » disent ce qu'il a consulté (regroupées en « 3 lectures du planning » au-delà de deux : cliquez pour les voir).
- **Réflexion** : coupée par défaut. Menu « … » → **Montrer la réflexion** demande au modèle une courte réflexion visible (environ 1 000 jetons de plus par réponse, donc un peu plus cher) ; elle est repliée sous « Réflexion · 6 s », cliquez pour la lire. Certains modèles (Gemini) en montrent parfois d'eux-mêmes.
- **Arrêter** (le bouton carré, ou Échap dans le champ) coupe la réponse en cours (et sa facturation chez la plupart des fournisseurs) ; le début reste affiché, **Continuer** reprend là où elle s'est arrêtée.
- Sous chaque réponse : le modèle, le nombre de jetons et le **coût** annoncé par OpenRouter en dollars (survolez pour le détail, dont la part lue dans le cache, moins chère). Au survol (ou au clavier avec Tab) : **Copier** (le texte de la réponse) et **Régénérer** (dernière réponse).
- **Erreur** : un encadré explique ce qui s'est passé ; **Réessayer** renvoie la question.
- **Modèle** (liste en haut du volet) : Claude Sonnet 5.5 (par défaut, le plus sûr), Claude Haiku 5.5 (beaucoup moins cher, plus simple), Gemini 3.8 Flash. Le choix est mémorisé.

## 5. La conversation

- **L'assistant se souvient de la conversation** : vous pouvez enchaîner (« et pour les fondations ? »). À chaque question, il relit le planning, même s'il en a déjà parlé : si vous avez modifié le projet entre-temps, sa réponse tient compte des nouvelles dates.
- La conversation est **gardée pour chaque projet**, sur cet ordinateur (ou dans ce navigateur pour la version web) : rouvrez le projet, elle revient, votre dernière question en haut. Elle n'est **jamais** enregistrée dans le fichier du projet : un planning que vous envoyez à quelqu'un ne transporte pas vos échanges. La clé OpenRouter n'y figure jamais. Vingt conversations au plus sont gardées ; les plus anciennes disparaissent.
- **Nouvelle conversation** (icône crayon en haut du volet) range la conversation actuelle et repart de zéro. C'est utile quand le sujet change : les échanges précédents sont renvoyés à chaque question (allégés, et en partie en cache), donc une longue conversation coûte un peu plus cher.

## 5 bis. Les propositions de modifications

Demandez en langage courant : « Crée le planning d'une maison de 120 m² », « Décale le second œuvre de deux semaines », « Lie la plomberie à l'électricité », « Mets les jalons en gras », « Regroupe par lot ».

- L'assistant prépare **un lot** de modifications et l'affiche sous sa réponse : une carte « Proposition : <libellé> — 18 tâches touchées ». **Rien n'est encore appliqué.**
- **Voir** ouvre l'onglet Opérations IA : la table avant/après (début, fin, marge, chemin critique), les nouvelles valeurs, les anciennes barrées. Les dates sont celles du moteur de PlannerCraft : l'assistant ne donne jamais de date, il propose des durées (en jours ouvrés) et des liens, le moteur place les dates.
- **Accepter** applique tout le lot d'un coup ; **Annuler** (Ctrl+Z / Cmd+Z) le défait d'un coup.
- **Refuser** n'applique rien. Vous pouvez écrire un **motif** dans le champ de la carte (« garde les durées actuelles ») : il est transmis à l'assistant avec votre message suivant, comme la décision elle-même. Il peut alors proposer autre chose.
- Une proposition attend votre décision **sans limite de temps**. Si vous modifiez le planning avant de décider, Accepter montre d'abord un **nouvel aperçu** (rien n'est appliqué sur l'ancien). Si vous fermez ou changez de projet, la proposition est **périmée** ; une nouvelle conversation la retire. Dans ces cas rien n'est appliqué.
- Si le moteur refuse le lot (un lien qui ferait une boucle, par exemple), l'assistant voit l'erreur et corrige (deux fois au plus) ; la ligne « Proposition refusée par le moteur » le signale.
- **Plannings de chantier** : l'assistant connaît les phases usuelles (études et permis, terrassement et fondations, gros œuvre, clos couvert, second œuvre, finitions, réception), met les jalons (permis obtenu, hors d'eau hors d'air, réception), des durées en jours ouvrés et des liens fin-début ; si votre champ Texte1 s'appelle « Lot », il y range le corps d'état. Relisez ses durées : ce sont des estimations à ajuster.
- **Dates** : l'assistant ne calcule jamais une date. Il ne pose une échéance ou une contrainte qu'à une date **que vous avez écrite** dans votre demande (« fixe la livraison au 15 mars 2027 ») ; s'il essaie une autre date, l'application la refuse et il doit corriger.

## 6. Ce qui est envoyé

À chaque question partent chez OpenRouter, puis chez le fournisseur du modèle choisi (Anthropic pour Claude, Google pour Gemini) :

- vos questions et les réponses précédentes de la conversation (les plus anciennes allégées : sans ce que l'assistant avait lu) ;
- la tâche active, la sélection et la vue au moment de la question ;
- les modifications qu'il propose, le résumé de leur aperçu et votre décision (avec votre motif de refus s'il y en a un) ;
- **ce que l'assistant lit dans le planning** pour répondre : noms et dates des tâches, durées, marges, liens, ressources, notes d'une tâche consultée, nom et dates du projet, tâche sélectionnée.

PlannerCraft demande à OpenRouter de n'utiliser que des fournisseurs qui **ne conservent pas** vos données et **ne s'en servent pas pour entraîner** leurs modèles (options `zdr` et `data_collection: deny`). Le fichier du projet lui-même n'est jamais envoyé, ni les autres projets.

Si vos plannings contiennent des noms de personnes ou d'entreprises que vous ne voulez pas transmettre, n'activez pas l'assistant pour ces projets.

## 7. Messages d'erreur

| Message | Que faire |
|---|---|
| Clé refusée par OpenRouter | Reconnectez-vous, ou saisissez une autre clé (elle a peut-être été révoquée). |
| Crédits OpenRouter épuisés | Rechargez votre compte sur openrouter.ai. |
| Trop de demandes | Attendez le délai indiqué, puis renvoyez la question. |
| Connexion impossible | Vérifiez votre accès à Internet (ou le pare-feu de l'entreprise). |
| L'assistant a fait trop d'étapes | Posez une question plus simple ou plus précise. |
| Trousseau du système indisponible | La clé marche pour cette session ; il faudra la redonner au prochain lancement. |

## 8. Confort

**Affichage → Thème → Réduire les animations** coupe les défilements animés, la pulsation de « Réflexion… » et les animations de l'interface.
