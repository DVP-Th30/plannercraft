# Cahier « Conversation de l'assistant » (lot A1b) — PlannerCraft

Référence : ADR-0024, `docs/plans/assistant.md`. Demande du propriétaire (2026-11-06) : contexte partagé avec les réponses précédentes, voir quand l'assistant réfléchit, réponse affichée en flux **sans que la vue saute en bas** (on lit depuis le début), différenciation nette entre ses questions et les réponses, niveau de finition des meilleures interfaces fin 2026. Recherche du 2026-11-06 (sources citées dans ADR-0024, paragraphe A1b).

Toute convention ci-dessous est un choix de conception (`hypothèse` au sens des fiches H-UI) ; les sources sont des produits et guides publics (NN/g, Setproduct, Cloudscape, AI Elements, discussions Zed et Cursor, tale-project PR #830, better_chat_scrolling_ai), jamais du code de logiciel exclu.

## 1. Mise en page (volet de 400–460 px)

- Marges 14 px ; 20 px entre deux tours, 10 px entre une question et sa réponse ; texte 14 px, interligne ≈ 1,5 ; méta 12 px. Jetons dans `theme.rs` (clair et sombre), contrastes vérifiés par test (texte ≥ 4,5:1).
- **En-tête** (36 px) : icône IA + « Assistant IA », sélecteur de modèle, bouton « Nouvelle conversation ».
- **État vide** : phrase de transparence (« Les réponses sont générées par une IA et peuvent contenir des erreurs. Votre planning est envoyé à OpenRouter et au fournisseur du modèle. », AI Act art. 50) et 3–4 suggestions métier cliquables qui préremplissent la saisie (« Quel est le chemin critique ? », « Quelles tâches sont en retard ? », « Où sont les surcharges de ressources ? », « Résume le planning »).
- **Message de l'utilisateur** : bloc aligné à droite, largeur max 85 %, marge intérieure 8 × 12 px, coins 12 px (4 px en bas à droite), fond accent à ~9 % (clair) / ~16 % (sombre), texte normal, sans Markdown ; heure en infobulle.
- **Message de l'assistant** : pleine largeur, sans cadre ni avatar ni prénom, dans l'ordre : bloc réflexion, lignes d'outils, texte Markdown, pied atténué « Sonnet 5.5 · 2,3 k jetons · 0,007 $ » ; actions au survol (et au focus clavier) : Copier, Régénérer (dernier message seulement).
- **Bloc réflexion** (si le modèle renvoie un raisonnement) : replié par défaut, « ▸ Réflexion… » pendant le flux (pulsation d'opacité, coupée si animations réduites), « ▸ Réflexion · 6 s » à la fin ; déplié : texte atténué 12,5 px, filet gauche 2 px, défilement interne au-delà de 200 px ; chevron toujours visible.
- **Lignes d'outils** : une ligne de 22 px par lecture, libellé métier au passé (« A lu : le chemin critique », jamais l'identifiant technique), état (en cours, ✓, ⚠ avec infobulle) ; à partir de 3 lectures consécutives, une ligne « ▸ 4 lectures du planning » dépliable ; jamais de JSON brut.
- **Barre d'état du flux** (24 px, au-dessus de la saisie, seulement pendant une génération) : « En attente… » (si rien après 800 ms), « Réflexion… », « Lecture : chemin critique… », « Rédaction… », avec chronomètre.
- **Saisie** : multiligne 2 à 8 lignes puis défilement ; Entrée envoie, Maj+Entrée va à la ligne, Échap arrête ; bouton rond 28 px à l'accent, qui devient un carré « Arrêter » pendant la génération ; on peut taper le message suivant pendant le flux, l'envoi reste grisé.
- **Erreur** : encadré dans le fil (ce qui s'est passé, en langage d'utilisateur) et un bouton unique « Réessayer ». **Arrêt** : texte partiel gardé, « Réponse interrompue », bouton « Continuer ».
- **Markdown** : `egui_commonmark` 0.26 (egui 0.37, MIT OR Apache-2.0, sans l'option de coloration syntaxique), derrière une fonction `render_markdown(ui, &str)` remplaçable ; consigne système : tableaux de 3 colonnes au plus, sinon listes ; titres limités à deux niveaux visuels. Vérifier `cargo deny`, le poids wasm et le coût de l'analyse à chaque image sur une réponse longue (cache par message si nécessaire).

## 2. Défilement (exigence n° 3 du propriétaire)

Trois modes : `Anchored(message)`, `Following`, `Free`. Un seul `ScrollArea` vertical à `id_salt` stable ; jamais `stick_to_bottom(true)` en permanence.

1. À l'envoi : `Anchored(question)` ; à l'image suivante, la question est amenée en haut de la vue (marge 8 px), sans animation si animations réduites.
2. Marge basse après le dernier message : `max(0, hauteur_vue − hauteur(question + réponse en cours) − 8)`, gardée jusqu'au prochain envoi (la retirer à la fin du flux ferait sauter la vue).
3. Pendant le flux en `Anchored` : le décalage ne bouge jamais ; la réponse pousse sous la question.
4. Geste de l'utilisateur (molette ou pavé sur la zone, barre de défilement tirée, PageUp/PageDown, décalage différent de plus de 1 px de celui imposé) ⇒ `Free` : le programme ne déplace plus la vue.
5. Pastille flottante centrée, 12 px au-dessus de la saisie, si la fin du contenu (marge basse exclue) est à plus de 48 px sous la vue, apparition après 300 ms : « ↓ Suite de la réponse » pendant le flux, « ↓ Dernier message » sinon ; clic pendant le flux ⇒ `Following` (colle au bas tant qu'on y est ; geste vers le haut ⇒ `Free`).
6. Redimensionnement du volet en `Anchored` : réancrer la question.
7. Réouverture d'une conversation : la dernière question en haut de la vue.
8. Rafraîchissement : `request_repaint_after(33 ms)` à chaque arrivée de texte, aucune boucle continue.

## 3. Mémoire de conversation

- L'historique de la conversation est renvoyé à chaque demande, dans cet ordre : consignes système et définitions d'outils (stables, **point de cache n° 1**), tours précédents (**point de cache n° 2** sur le dernier message d'historique), puis la question courante avec l'état utile de l'interface (sélection) **dans le dernier message seulement**.
- Allègement : texte final des réponses gardé ; résultats d'outils des tours anciens retirés (les 2 derniers tours intacts) ; budget d'historique ≈ 16 k jetons ou 20 tours, les plus anciens tours abandonnés au-delà (résumé par un petit modèle : plus tard).
- Cache de prompt OpenRouter (`cache_control` sur les blocs, 5 min) pour les modèles qui le prennent ; coût réel lu dans `usage` (jetons en cache compris) et affiché.
- **Persistance par document, stockage local de l'application** (pas dans le fichier projet : un planning partagé ne transporte pas la conversation) : clé = chemin canonique ou identifiant du document, au plus 20 conversations gardées par appareil, la plus ancienne supprimée ; jamais de clé d'API ni de jeton dedans ; « Nouvelle conversation » archive la précédente (liste des anciennes : plus tard). Sur le web : stockage du navigateur, même règle.
- Une nouvelle question quand le planning a changé depuis la réponse précédente : l'assistant relit par ses outils (consigne système), il ne se fie pas aux dates citées plus haut (règle 3).

## 4. Accessibilité

Texte accessible du message publié à la fin seulement (pas à chaque morceau), libellé d'état « réponse en cours » ; navigation clavier (Tab vers les actions, Copier/Régénérer accessibles sans souris) ; réglage « Réduire les animations » (pulsation, défilement animé) ; contrastes testés.

## 5. Hors périmètre (plus tard)

Résumé des anciens tours, liste et export des conversations, 👍/👎, modification d'une question envoyée, versions régénérées navigables, cache 1 h, questions de suivi proposées sous la réponse.
