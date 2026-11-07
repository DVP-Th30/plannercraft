# Cahier « Relais et compte PlannerCraft » (lots R1, A4, R2)

Référence : ADR-0024 (assistant, modèle économique) et ADR-0025 (choix du relais). Le relais est un service **privé** de l'éditeur (dépôt `DVP-Th30/plannercraft-relay`), l'application reste libre. Décisions du propriétaire (2026-11-06/07) : Stripe Managed Payments (repli Paddle), packs de crédits prépayés, Sonnet 5.5 par défaut avec liste fermée, données en UE plus tard, pas de demande préalable à OpenRouter.

## 1. Ce que fait le relais

- **Comptes** : connexion par **lien magique** envoyé par e-mail (aucun mot de passe). Un compte = une adresse e-mail, un solde de crédits, des jetons de session (révocables).
- **Crédits** : grand livre en centimes d'euro (entiers), jamais de flottant ; chaque demande débite le coût réel (lu dans `usage` d'OpenRouter, puis corrigé par `GET /generation` si disponible) multiplié par la marge configurée ; solde négatif impossible (refus avant appel si le solde est sous un plancher) ; achats par packs via Stripe Checkout ; remboursement manuel par l'éditeur.
- **Appels LLM** : un seul endpoint **métier** `POST /v1/assistant/chat` qui relaie **une** complétion au format de l'app (messages, outils de l'app, `stream: true`) vers OpenRouter avec la clé de l'éditeur, en imposant : modèle dans la liste fermée, `max_tokens` plafonné, `provider: { zdr: true, data_collection: "deny" }`, en-têtes d'attribution, nombre de complétions par « demande » borné (en-tête `X-PlannerCraft-Turn` ≤ 8, compte tenu par le relais par identifiant de demande), limites de débit par compte. **Jamais** un proxy générique : pas de modèle libre, pas de `provider` libre, pas d'autres routes OpenRouter.
- **Ce que le relais ne garde pas** : ni prompts, ni réponses, ni résultats d'outils. Il journalise seulement : compte, horodatage, modèle, jetons (entrée, sortie, cache), coût, identifiant de génération. Rétention 13 mois (comptabilité), suppression à la demande.
- **Pilotage** : un petit back-office en ligne de commande (`relay admin …`) : créditer, lister, bloquer un compte, exporter le grand livre en CSV.

## 2. Contrat d'API (version 1, préfixe `/v1`)

Toutes les réponses en JSON ; erreurs `{ "code": "…", "message": "…" }` ; codes : `auth.required`, `auth.invalid`, `auth.expired`, `credits.insufficient`, `model.not_allowed`, `rate.limited` (+ `Retry-After`), `turn.limit`, `upstream.error`, `upstream.unavailable`.

| Méthode et route | Entrée | Sortie |
|---|---|---|
| `POST /v1/auth/magic-link` | `{ email }` | `204` toujours (ne révèle pas si le compte existe) ; e-mail avec un lien `https://<app>/login?token=…` **et** un code à 8 caractères à saisir (desktop sans retour navigateur) ; validité 15 min, usage unique |
| `POST /v1/auth/exchange` | `{ token }` ou `{ email, code }` | `{ session_token, expires_at, account: { email, balance_cents } }` ; jeton opaque 256 bits, 90 jours glissants |
| `POST /v1/auth/logout` | `Authorization: Bearer` | `204` (révoque le jeton) |
| `GET /v1/me` | Bearer | `{ email, balance_cents, currency: "EUR", models: [ { id, label, input_cents_per_mtok, output_cents_per_mtok } ], low_balance: bool }` |
| `GET /v1/usage?from&to` | Bearer | `{ items: [ { at, model, prompt_tokens, completion_tokens, cached_tokens, cost_cents } ], total_cents }` |
| `POST /v1/assistant/chat` | Bearer, `X-PlannerCraft-Request: <uuid>`, `X-PlannerCraft-Turn: n`, corps = requête OpenRouter **sans** `provider`, `transforms`, `route`, clés inconnues refusées | flux SSE identique à OpenRouter (y compris le `usage` final) ; en-tête de réponse `X-PlannerCraft-Cost-Cents` et `X-PlannerCraft-Balance-Cents` à la fin (trailer ou dernier évènement `: plannercraft-cost …`) |
| `POST /v1/credits/checkout` | Bearer, `{ pack: "p5" \| "p20" \| "p50" }` | `{ url }` (Stripe Checkout, mode paiement unique ; `client_reference_id` = compte) |
| `POST /v1/stripe/webhook` | signature Stripe | `checkout.session.completed` ⇒ crédit du compte, idempotent par `event.id` |
| `GET /healthz` | — | `200` |

Packs (prix TTC affichés par Stripe, HT en base) : p5 = 5 €, p20 = 20 €, p50 = 50 € ; marge = coût OpenRouter × 2 (paramètre), à ajuster après mesure réelle. Un compte neuf reçoit 0,50 € d'essai (paramètre).

## 3. Lots

| Lot | Contenu | Dépôt |
|---|---|---|
| **R1** | Relais : axum, Postgres (sqlx, migrations), comptes et lien magique (envoi par SMTP via `lettre`, mailer « console » en dev), jetons, grand livre, endpoint chat, liste de modèles, limites, journal, back-office CLI, Dockerfile, `docker-compose.dev.yml`, tests (sans réseau : OpenRouter et SMTP simulés), docs | `plannercraft-relay` |
| **A4** | App : mode « Compte PlannerCraft » à côté de « Ma clé » (connexion par e-mail + code, jeton dans le trousseau, solde et coût affichés, achat de crédits par lien, messages d'erreur), transport vers le relais, tests avec un faux relais | `plannercraft` |
| **R2** | Stripe réel (Managed Payments après éligibilité), CGV, DPA, page de prix, déploiement Coolify avec domaine, surveillance | les deux |

R1 puis A4 (une compilation lourde à la fois sur la machine). R2 attend le compte Stripe et la validation juridique du propriétaire.

## 4. Entrées encore attendues du propriétaire

Compte Stripe (clé de test puis réelle), nom de domaine du relais (ex. `api.plannercraft.app`), expéditeur SMTP (compte Brevo/Resend ou SMTP existant), statut juridique et TVA (expert-comptable), CGV.
