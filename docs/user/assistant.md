# Assistant IA : se connecter, acheter des crédits, ce qui est envoyé

Guide court de l'assistant intégré (version « ma propre clé »). Référence technique : `docs/features/assistant.md`, ADR-0024 (`docs/decisions.md`).

## 1. Ce que fait l'assistant

Vous lui posez une question en langage courant sur le planning ouvert : « Quand finit le projet ? », « Pourquoi la tâche 12 commence-t-elle le 4 janvier ? », « Quelles tâches sont critiques ? ». Il lit le planning avec les mêmes commandes que l'application (résumé du projet, liste des tâches, une tâche, chemin critique, « Pourquoi cette date ? », ressources, écarts à la ligne de base, tâche sélectionnée) et répond.

- **Il propose, vous décidez.** Il peut proposer des modifications (créer un planning, décaler, lier, renommer, affecter, mettre en forme) ; rien n'est appliqué sans votre accord (§ 5 bis).
- **Il ne calcule pas les dates.** Les dates, durées et marges qu'il cite viennent du moteur de PlannerCraft ; s'il ne trouve pas une information, il doit le dire.
- C'est une IA : relisez ses réponses comme celles d'un collègue pressé.

## 2. Ouvrir et activer

1. Cliquez le bouton **✦ Assistant IA**, en haut à droite de la barre des onglets du ruban (toujours visible, même ruban réduit ou sans projet ouvert), ou appuyez sur **Ctrl+J** (**Cmd+J** sur macOS) ; aussi **Affichage → Fractionner l'affichage → Assistant IA** ou menu Affichage → Assistant IA. Le volet s'ouvre à droite, onglet « Assistant ». Le même bouton, le même raccourci ou **Échap** (question vide, rien en cours) le referment ; § 7 bis.
2. Lisez la mention : *les réponses sont générées par une IA et peuvent contenir des erreurs ; votre planning est envoyé à OpenRouter et au fournisseur du modèle*. Elle reste affichée tant que la conversation est vide.
3. Choisissez votre **accès** :
   - **Compte PlannerCraft** (recommandé) : aucune clé à gérer, vous achetez des crédits à PlannerCraft (§ 3 bis) ;
   - **Ma clé OpenRouter** : vous connectez votre propre compte OpenRouter et lui payez directement vos appels (§ 3).

   Rien n'est envoyé avant ce choix et votre première question. Le choix se change à tout moment dans le menu « … » (en haut à droite du volet), rubrique « Accès à l'assistant ». « Désactiver l'assistant » (même menu) revient en arrière et oublie la conversation du projet.

## 3 bis. Compte PlannerCraft
<a id="compte-plannercraft"></a>

### Créer un compte, se connecter

Pas de mot de passe : vous recevez un lien et un code par e-mail.

1. Saisissez votre adresse e-mail et cliquez **Recevoir le lien**. Le compte est créé à la première connexion réussie (un compte neuf reçoit un petit crédit d'essai).
2. Ouvrez l'e-mail « PlannerCraft » (le code figure aussi dans son objet) :
   - **sur ordinateur**, saisissez le **code à 8 caractères** dans le volet, puis **Valider** (majuscules, espaces et tirets indifférents) ;
   - **dans la version web**, cliquez simplement le lien : la page de PlannerCraft s'ouvre connectée (le code marche aussi).
3. Le lien et le code valent 15 minutes et une seule fois ; « Renvoyer le code » en envoie un nouveau (l'ancien ne vaut plus), « Changer d'adresse » revient à la saisie.

Une fois connecté, le **solde** s'affiche sous le titre du volet (« Solde : 4,37 € ») ; une alerte discrète « Solde bas » apparaît quand il reste peu de crédits. Le menu « … » donne l'adresse du compte, **Consommation**, **Actualiser le solde** et **Se déconnecter du compte**.

### Acheter des crédits

- **Acheter des crédits ▾** (sous le titre, ou dans la conversation quand les crédits sont épuisés) : packs de **5 €, 20 € ou 50 €**. La page de paiement (Stripe) s'ouvre dans votre navigateur ; PlannerCraft ne voit jamais votre carte.
- Au retour dans l'application, le solde se met à jour ; sinon, **Actualiser le solde** (bouton ⟳ à côté, ou menu « … »). Dans la version web, la page de paiement s'ouvre dans un nouvel onglet : revenez à l'onglet de PlannerCraft.
- Chaque réponse indique son coût en euros dans son pied (« Sonnet 5.5 · 2,3 k jetons · 0,03 € » ; « < 0,01 € » pour une toute petite réponse : les fractions de centime sont reportées sur la suivante). **Consommation** liste vos demandes des 30 derniers jours et leur total (heures en temps universel).
- Le prix des modèles (par million de jetons) s'affiche au survol de la liste des modèles.

### Sécurité et confidentialité

- **Aucune clé** de fournisseur d'IA dans l'application : vos questions passent par le serveur PlannerCraft, qui les transmet à OpenRouter puis au fournisseur du modèle, sans conservation ni entraînement imposés. **Le serveur PlannerCraft ne conserve pas vos conversations** : il ne garde que la date, le modèle, le nombre de jetons et le coût de chaque demande (comptabilité, 13 mois).
- Votre **session** (jeton de connexion, valable 90 jours) est gardée dans le trousseau du système sur ordinateur, en mémoire dans la version web (un rechargement de la page demande de se reconnecter) ; jamais dans les préférences ni dans vos fichiers.
- **Se déconnecter du compte** révoque la session sur le serveur et l'efface de l'appareil. Une session expirée ou refusée ramène à l'écran de connexion avec un message.

## 3. Se connecter à OpenRouter (accès « Ma clé OpenRouter »)

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
- Sous chaque réponse : le modèle, le nombre de jetons et le **coût** : en euros débités de votre solde avec un compte PlannerCraft, en dollars annoncés par OpenRouter avec votre clé (survolez pour le détail, dont la part lue dans le cache, moins chère). Au survol (ou au clavier avec Tab) : **Copier** (le texte de la réponse) et **Régénérer** (dernière réponse).
- **Erreur** : un encadré explique ce qui s'est passé ; **Réessayer** renvoie la question.
- **Modèle** (liste en haut du volet) : Claude Sonnet 5.5 (par défaut, le plus sûr), Claude Haiku 5.5 (beaucoup moins cher, plus simple), Gemini 3.8 Flash. Le choix est mémorisé. Avec un compte PlannerCraft, la liste vient du serveur et le prix s'affiche au survol.

## 5. La conversation

- **L'assistant se souvient de la conversation** : vous pouvez enchaîner (« et pour les fondations ? »). À chaque question, il relit le planning, même s'il en a déjà parlé : si vous avez modifié le projet entre-temps, sa réponse tient compte des nouvelles dates.
- La conversation est **gardée pour chaque projet**, sur cet ordinateur (ou dans ce navigateur pour la version web) : rouvrez le projet, elle revient, votre dernière question en haut. Elle n'est **jamais** enregistrée dans le fichier du projet : un planning que vous envoyez à quelqu'un ne transporte pas vos échanges. La clé OpenRouter n'y figure jamais. Vingt conversations au plus sont gardées ; les plus anciennes disparaissent.
- **Nouvelle conversation** (icône crayon en haut du volet) range la conversation actuelle et repart de zéro. C'est utile quand le sujet change : les échanges précédents sont renvoyés à chaque question (allégés, et en partie en cache), donc une longue conversation coûte un peu plus cher.

## 5 bis. Les propositions de modifications

Demandez en langage courant : « Crée le planning d'une maison de 120 m² », « Décale le second œuvre de deux semaines », « Lie la plomberie à l'électricité », « Mets les jalons en gras », « Regroupe par lot ».

- L'assistant prépare **un lot** de modifications et l'affiche sous sa réponse : une carte « Proposition : <libellé> » avec « 18 tâches modifiées · aucune date déplacée » (tâches que le lot change, y compris celles qu'il déplace dans la liste, puis tâches dont les dates bougent). **Rien n'est encore appliqué.**
- **Voir** ouvre l'onglet Opérations IA : la table avant/après (début, fin, marge, chemin critique), les nouvelles valeurs, les anciennes barrées. Toutes les tâches modifiées y figurent, même celles dont les dates ne bougent pas (avancement, nom, place dans la liste…) ; la colonne **Modification** dit ce qui change pour chacune (« % achevé : 0 % → 100 % », « N° : 20 → 18 », « Notes : modifiées », trois lignes au plus puis « + n autres »). Une tâche décalée par le recalcul, sans modification directe, a cette cellule vide. Quand des dates bougent, cette colonne passe en dernier (faites défiler le tableau vers la droite) et tient sur une ligne, pour que Début et Fin restent visibles. Les dates sont celles du moteur de PlannerCraft : l'assistant ne donne jamais de date, il propose des durées (en jours ouvrés) et des liens, le moteur place les dates.
- **Accepter** applique tout le lot d'un coup ; **Annuler** (Ctrl+Z / Cmd+Z) le défait d'un coup.
- **Ce qui change se voit** : si le lot écrit un champ dont la colonne n'est pas affichée (Noms ressources, Notes, Échéance, Type de contrainte…), la colonne est ajoutée à la fin du tableau des tâches (quatre au plus par lot) et la table défile jusqu'à elle ; la carte le dit (« Colonne affichée : Noms ressources »). Un lot qui ne fait que créer ou modifier des ressources propose **Voir les ressources** (feuille des ressources). Ces colonnes sont un réglage d'affichage : annuler le lot ne les retire pas (clic droit sur le titre → Masquer la colonne). Même chose pour une écriture d'un agent MCP acceptée dans l'onglet Opérations IA.
- **Refuser** n'applique rien. Vous pouvez écrire un **motif** dans le champ de la carte (« garde les durées actuelles ») : il est transmis à l'assistant avec votre message suivant, comme la décision elle-même. Il peut alors proposer autre chose.
- Une proposition attend votre décision **sans limite de temps**. Si vous modifiez le planning avant de décider, Accepter montre d'abord un **nouvel aperçu** (rien n'est appliqué sur l'ancien). Si vous fermez ou changez de projet, la proposition est **périmée** ; une nouvelle conversation la retire. Dans ces cas rien n'est appliqué.
- Si le moteur refuse le lot (un lien qui ferait une boucle, par exemple), l'assistant voit l'erreur et corrige (deux fois au plus) ; la ligne « Proposition refusée par le moteur » le signale.
- **Plannings de chantier** : l'assistant connaît les phases usuelles (études et permis, terrassement et fondations, gros œuvre, clos couvert, second œuvre, finitions, réception), met les jalons (permis obtenu, hors d'eau hors d'air, réception), des durées en jours ouvrés et des liens fin-début ; si votre champ Texte1 s'appelle « Lot », il y range le corps d'état. Relisez ses durées : ce sont des estimations à ajuster.
- **Dates** : l'assistant ne calcule jamais une date. Il ne pose une échéance ou une contrainte qu'à une date **que vous avez écrite** dans votre demande (« fixe la livraison au 15 mars 2027 ») ; s'il essaie une autre date, l'application la refuse et il doit corriger.

## 6. Ce qui est envoyé

À chaque question partent chez OpenRouter, puis chez le fournisseur du modèle choisi (Anthropic pour Claude, Google pour Gemini) — avec un compte PlannerCraft, en passant d'abord par le serveur PlannerCraft, qui ne conserve pas vos conversations (§ 3 bis) :

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
| Trousseau du système indisponible | La clé (ou la session) marche pour cette session ; il faudra la redonner au prochain lancement. |
| Crédits épuisés | Compte PlannerCraft : cliquez un pack (5 €, 20 €, 50 €) sous le message, payez, puis **Réessayer**. |
| Votre session a expiré / Session refusée | Compte PlannerCraft : reconnectez-vous (adresse, puis code reçu par e-mail). |
| Code ou lien refusé | Code mal saisi, expiré (15 minutes) ou déjà utilisé : « Renvoyer le code ». |
| Ce modèle n'est pas proposé avec le compte | Choisissez un autre modèle dans la liste. |
| Le service d'IA est momentanément indisponible | Réessayez dans un instant. |

## 7 bis. Le volet, toujours à portée

- **Déployer, réduire** : le volet n'est jamais fermé. Réduit, il reste à portée sur un **rail** étroit au bord droit de la fenêtre (icône ✦) : cliquez n'importe où sur le rail pour le déployer. Déployé, la **croix** en haut à droite du volet le réduit au rail. Le bouton ✦ en haut à droite du ruban (enfoncé quand le volet est déployé), **Ctrl+J** / **Cmd+J** (même pendant que vous tapez la question) et le menu Affichage → Assistant IA déploient ou réduisent le volet. Au déploiement, le curseur est dans la question. **Échap** réduit le volet quand la question est vide et qu'aucune réponse n'est en cours (sinon Échap arrête la réponse, ou ferme d'abord la fenêtre ou le menu ouvert). Sur la version web, Ctrl+J ouvre l'assistant et non les téléchargements du navigateur.
- **Réduire n'interrompt rien** : une réponse en cours continue volet réduit. Volet réduit, le bouton du ruban et le rail pulsent doucement tant qu'elle s'écrit (contour fixe avec « Réduire les animations ») ; une **pastille** s'allume sur les deux quand elle est terminée, en erreur, ou qu'une proposition vous attend (un nombre s'il y en a plusieurs). Elle s'efface au déploiement du volet.
- **Largeur** : tirez le bord gauche du volet (de 320 points à la moitié de la fenêtre) ; double-clic sur ce bord : largeur par défaut. La largeur et le volet déployé ou réduit sont retrouvés au lancement suivant.
- **Petite fenêtre** (moins de 900 points de large) : le rail reste à sa place ; le volet déployé passe par-dessus le planning, sans rétrécir la table.
- Le volet s'ouvre toujours sur l'onglet **Assistant** ; « Opérations IA » et « Pourquoi cette date ? » restent à côté. Une proposition de l'assistant reste dans la conversation (carte) ; une demande d'un agent MCP externe ouvre « Opérations IA » comme avant.

## 8. Confort

**Affichage → Thème → Réduire les animations** coupe les défilements animés, la pulsation de « Réflexion… » (et celle du bouton ✦ pendant une réponse) et les animations de l'interface.
