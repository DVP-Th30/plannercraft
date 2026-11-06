# Assistant IA (`crates/plannercraft-assistant`, volet Assistant)

État au 2026-11-06 (lot A1 du cahier `docs/plans/assistant.md`, ADR-0024) : mode « ma propre clé » (OpenRouter), **lecture seule**. L'assistant répond en langage courant sur le planning ouvert ; il ne modifie rien (les propositions d'écriture passent par le panneau Opérations IA au lot A2). Le mode payant (relais de l'éditeur, lots R1, R2, A4) n'existe pas encore.

## 1. Architecture

```
Volet Assistant (plannercraft-ui-egui, view/assistant.rs)
   │  send / stop / poll à chaque image (aucun blocage)
   ▼
Conversation (plannercraft-assistant::conversation) ── outils en lecture ──▶ SessionHost
   │  HttpRequest + puits d'évènements                     (copie de la session,
   ▼                                                        workspace::call_with)
Transport (http.rs)
   ├─ EhttpTransport : ureq dans un fil (desktop), fetch du navigateur (web)
   └─ ScriptedTransport : réponses écrites à la main (tests, aucun réseau)
```

- Couches (règle 9, `cargo xtask layers`) : la crate est sous `plannercraft-ui-egui`, sans egui ni eframe ; elle ne dépend ni de `plannercraft-mcp` ni de `rmcp` (ce n'est pas un serveur MCP embarqué, ADR-0002), et aucune crate du cœur, du rendu, du registre ou du MCP ne dépend d'elle.
- Pas de runtime async : la conversation est une machine à états menée par l'interface. Le transport rend la réponse en évènements (`Head`, `Chunk`, `End`, `Failed`) poussés dans une file partagée, puis réveille l'image (`Context::request_repaint`). `Conversation::poll` lit la file, assemble le texte, exécute les outils, lance le tour suivant.
- Même code desktop et web : seuls le transport (`ehttp`), l'aléa (`getrandom` / `crypto.getRandomValues`), la connexion OAuth (écouteur local / redirection) et le magasin de clé changent.

## 2. Protocole OpenRouter

Docs relues le 2026-11-06 : `api-reference/streaming`, `use-cases/usage-accounting`, `use-cases/oauth-pkce`.

- `POST https://openrouter.ai/api/v1/chat/completions`, `Authorization: Bearer <clé>`, `HTTP-Referer: https://github.com/DVP-Th30/plannercraft`, `X-OpenRouter-Title: PlannerCraft`, `Accept: text/event-stream`.
- Corps : `model` (liste fermée), `messages` (invite système puis historique), `stream: true`, `max_tokens` (2 048), `tools`, `tool_choice` (`auto`, puis `none` au dernier tour permis), **`provider: { "zdr": true, "data_collection": "deny" }`** (fournisseurs sans conservation ni entraînement, imposé). `usage` n'est pas demandé : OpenRouter le joint toujours (option `usage.include` dépréciée).
- Modèles (identifiants vérifiés sur `/api/v1/models` le 2026-11-06, prix d'entrée / sortie en $ par million de jetons) : `anthropic/claude-sonnet-5.5` (défaut, 2 / 10), `anthropic/claude-haiku-5.5` (0,10 / 0,50), `google/gemini-3.8-flash` (0,825 / 4,125). Un identifiant inconnu dans l'état mémorisé retombe sur le défaut.
- Lecteur SSE (`sse.rs`, écrit à la main) : fins de ligne `\n`, `\r\n`, `\r` (coupées entre deux morceaux comprises), commentaires `:` ignorés (`: OPENROUTER PROCESSING`), lignes `data` multiples jointes, ligne de plus de 1 Mio refusée, UTF-8 invalide remplacé. Dernier évènement sans ligne vide : lu à la fin.
- Assemblage (`stream.rs`) : texte `choices[0].delta.content` ; appels d'outils `delta.tool_calls[]` rangés par `index`, `id` et nom au premier morceau, `arguments` concaténés morceau après morceau ; `finish_reason` ; `usage` (jetons, `cost` en crédits) lu dans le dernier morceau, dont le `choices` n'est pas vide ; `error` au sommet d'un morceau (statut resté 200) = erreur du fournisseur. Bornes : 64 Kio de texte, 64 Kio d'arguments par appel, 16 appels par réponse, 8 Mio reçus par tour.
- Erreurs HTTP (`openrouter::http_error`) : 401 clé refusée, 402 crédits épuisés, 429 limite de débit (`Retry-After` en secondes s'il est entier ; une date HTTP n'est pas lue), autre statut : `error.message` du corps. Coupure (échec du transport, flux fermé sans `[DONE]` ni raison de fin) : erreur réseau. Aucune de ces erreurs ne panique ; toutes s'affichent en langage d'utilisateur (§ 5).
- Arrêt : le puits rend `ControlFlow::Break` au prochain évènement, la connexion est fermée (OpenRouter arrête alors la génération et la facturation chez les fournisseurs qui le permettent) ; les évènements tardifs sont ignorés (numéro de requête).

## 3. Outils et boucle

- Outils (`tools.rs`) : lectures du registre, mêmes noms que les outils MCP — `project_summary`, `list_tasks`, `get_task`, `explain_date`, `critical_path`, `list_resources`, `baseline_variance` — plus `ui_state` (tâche active, sélection, vue, lignes visibles), servi par l'interface. Schémas du registre allégés : sans `$schema` ni `title`, descriptions réduites à leur première phrase (160 caractères au plus). Mesure (`definitions_light_and_under_budget`) : **6 151 octets** en français, 6 068 en anglais, soit environ 1 500 jetons (objectif < 15 000).
- Lecture seule par construction : liste blanche d'ids de genre `Read` ou `Analysis` (un nom d'écriture est refusé `tool.unknown` avant tout appel) ; chaque appel s'exécute par `workspace::call_with` sur une **copie** de la session (`SessionHost`), jetée ensuite. `style.save_as_template` (genre lecture, mais qui écrit un modèle) et `what_if` (148 Ko de schéma) ne sont pas exposés.
- Résultat d'outil : JSON de la vue du moteur, ou l'erreur structurée (`{"error": {code, message}}`) que le modèle lit pour se corriger ; tronqué à 24 Kio avec une note (paginer, filtrer).
- Boucle : 6 tours au plus par demande ; le dernier part avec `tool_choice: "none"` (réponse en texte) ; au-delà, erreur « trop d'étapes ». Historique élagué par tours entiers au-delà de 256 Kio (un appel n'est jamais séparé de son résultat). Coût de la demande : somme des `usage` des tours.
- Invite système : `plannercraft_automation::agent::assistant_instructions(lang)`, en français ou en anglais selon la langue de l'interface. Elle partage ses règles avec les consignes du serveur MCP (`agent::mcp_instructions`, texte MCP inchangé, vérifié octet pour octet au lot A1) : UID stables, **toute date, durée ou marge vient du moteur, ne jamais en calculer ni en estimer une soi-même** (règle 3), justifier une date par `explain_date` ; lecture seule annoncée ; contenu du planning traité comme une donnée, jamais une consigne.

## 4. Connexion et secrets

- OAuth PKCE (`pkce.rs`, `login.rs`, `web.rs`) : vérifieur de 32 octets d'aléa en base64url (43 caractères), défi S256 (vecteur de la RFC 7636 en test), `state` de 16 octets vérifié au retour ; `https://openrouter.ai/auth?callback_url=…&code_challenge=…&code_challenge_method=S256&state=…` ; échange `POST /api/v1/auth/keys { code, code_verifier, code_challenge_method: "S256" }` → `{ key }`.
  - Desktop : écouteur `127.0.0.1` sur un port libre (adresse de retour `http://localhost:<port>/callback`, tout port permis par OpenRouter), une redirection, 5 minutes au plus, autres requêtes en 404, page de retour en HTML statique sans script ; navigateur ouvert par la crate `webbrowser` (MIT OR Apache-2.0, déjà tirée par eframe) ; échange du code dans le même fil (60 s au plus).
  - Web : la page part vers OpenRouter et revient avec `?code=…&state=…` ; vérifieur et `state` gardés dans `sessionStorage` le temps de l'aller-retour puis effacés ; l'adresse est nettoyée (`history.replaceState`). Le document ouvert n'est pas gardé au rechargement : la garde `beforeunload` protège un document modifié (§ 6).
  - Saisie manuelle d'une clé (champ masqué) dans les deux cas.
- Secrets (`secrets.rs`) : la clé ne va **jamais** dans `UiState` (envoyé aux agents MCP rattachés par `app.ui_state`) ni dans un journal (`Debug` de `HttpRequest`, `RequestConfig`, `MemoryStore` et `Pkce` masqués ; test sur l'état sérialisé). Desktop : trousseau du système par `keyring-core` 1.0 et ses magasins (Secret Service par `zbus-secret-service-keyring-store`, crypto Rust ; Trousseau macOS ; Gestionnaire d'identification Windows), entrée `PlannerCraft` / `openrouter-api-key`. Trousseau absent (pas de Secret Service, session sans D-Bus) : clé en mémoire pour la session, avec avertissement. Web : mémoire seulement. Déconnexion : clé effacée du magasin et de la mémoire (la clé reste valide chez OpenRouter : la révoquer dans ses réglages de clés).

## 5. Ce qui part chez le fournisseur

À chaque tour : l'invite système, la conversation de la session (questions, réponses, appels d'outils et leurs **résultats**), les définitions d'outils. Les résultats contiennent ce que le modèle a demandé : noms de tâches, dates, durées, marges, liens, ressources, notes d'une tâche lue par `get_task`, nom et dates du projet. Rien n'est envoyé tant que l'utilisateur n'a pas activé l'assistant (mention AI Act art. 50 au premier usage) et écrit une question. Destinataires : OpenRouter, puis le fournisseur du modèle choisi (Anthropic, Google), sous `zdr` et `data_collection: "deny"`. L'éditeur de PlannerCraft n'est pas dans la chaîne en mode « ma clé ».

## 6. Limites connues

- Lecture seule (A1) : aucune proposition d'écriture ; A2 ajoute l'outil « proposer un lot » et sa validation dans le panneau Opérations IA.
- Web : se connecter recharge la page (le document non enregistré est perdu après la confirmation du navigateur) ; saisir une clé évite le rechargement. La clé n'est pas gardée d'un onglet à l'autre.
- Desktop Linux : l'écouteur n'écoute que `127.0.0.1` ; un navigateur qui résoudrait `localhost` en `::1` seulement échouerait (non observé).
- TLS natif : rustls avec les racines de Mozilla (`webpki-roots`, CDLA-Permissive-2.0, exception de `deny.toml`), pas le magasin du système : un proxy d'entreprise à certificat maison n'est pas accepté.
- Délai : 300 s au plus par réponse ; aucun délai de connexion propre en dehors de celui du système.
- Le coût affiché est celui que rend OpenRouter (`usage.cost`) ; sans ce champ, seuls les jetons s'affichent.

## 7. Tests

- Crate (`cargo test -p plannercraft-assistant`, aucun réseau) : SSE (commentaires, coupures, CRLF, octet par octet, ligne trop longue, UTF-8), assemblage (texte, `usage`, appels fragmentés, erreur en flux, bornes), protocole (corps, en-têtes, erreurs 401/402/429, échange), PKCE (RFC 7636 annexe B, `state`), écouteur local (404, code, `state` faux, délai), connexion desktop de bout en bout avec un faux navigateur, outils (liste blanche, refus des écritures, troncature, budget), boucle sur un vrai projet (`tests/conversation.rs` : appel → `explain_date` du moteur → réponse, `ui_state`, refus sans clé, erreurs HTTP, coupure réseau, erreur du fournisseur, borne des tours, flux pas à pas et arrêt). Essai réel facultatif `live_openrouter_round_trip` (`#[ignore]`, lit `OPENROUTER_API_KEY`).
- Interface : `cargo test -p plannercraft-ui-egui --test ui -- assistant` (§ 1.40 de `docs/features/ui.md`).
