# Cahier de conception « Assistant IA » — PlannerCraft

Statut : **proposé** (ADR-0024), en attente des arbitrages du propriétaire (§ 9). Aucun code tant qu'ils ne sont pas rendus. Recherches du 2026-11-06 (docs officielles lues ce jour-là) : à revérifier avant la mise en vente, les tarifs et conditions bougent.

## 1. Objectif

Un bouton **Assistant** dans l'application : l'utilisateur écrit en français courant (« crée le planning d'une maison de 120 m² en 8 mois », « décale le second œuvre de deux semaines », « pourquoi la livraison glisse ? ») ; l'assistant propose des modifications, l'utilisateur voit l'avant/après et accepte ou refuse.

Deux façons d'y accéder :
- **Compte PlannerCraft (payant)** : l'utilisateur achète des crédits à l'éditeur ; aucune clé, aucun compte chez un fournisseur d'IA. Source de revenus.
- **Ma propre clé (gratuit côté PlannerCraft)** : l'utilisateur connecte son compte OpenRouter ; il paie ses appels lui-même.

Le code de l'application reste libre (MIT OR Apache-2.0). Le relais payant est un service de l'éditeur, dans un dépôt privé : rien dans les licences n'oblige à publier un code qui n'est pas distribué. « Contourner » le mode payant en modifiant l'app revient à utiliser sa propre clé, ce que le mode gratuit offre déjà : il n'y a rien à protéger dans le code. **La clé de l'éditeur n'est jamais dans l'application.**

## 2. Principes non négociables

1. **Règle 3** : l'assistant ne calcule jamais une date. Il appelle les commandes du registre (les mêmes que l'interface, la CLI et le MCP) ; toute date affichée vient du moteur.
2. **Validation humaine** de toute écriture (ADR-0015 D10, ADR-0016 D10) : chaque proposition passe par le panneau Opérations IA (avant/après, Accepter / Refuser, une annulation). Pas d'acceptation automatique.
3. **Transparence** (AI Act art. 50, applicable depuis le 2 août 2026) : l'utilisateur sait qu'il échange avec une IA ; l'assistant est désactivé tant qu'il ne l'a pas activé lui-même, avec l'information « vos données de planning sont envoyées à un fournisseur d'IA ».
4. **Aucun secret dans `UiState`** : il est envoyé aux agents MCP rattachés (`App::ui_state_json`). Les clés et jetons vivent à part (§ 5).
5. **Confidentialité** : fournisseurs sans conservation ni entraînement imposés (`provider: { zdr: true, data_collection: "deny" }`), rien de conservé sur le relais hors métadonnées de facturation.

## 3. Architecture

```
Application (libre)                         Relais éditeur (privé)               OpenRouter
┌────────────────────────────┐   jeton     ┌──────────────────────────┐  clé    ┌──────────┐
│ Volet Assistant (egui)      │──session──▶│ compte, solde, plafonds   │────────▶│ modèles  │
│ crate plannercraft-assistant│◀── SSE ────│ boucle d'outils bornée    │◀────────│          │
│  - client HTTP + SSE        │            │ liste fermée de modèles   │         └──────────┘
│  - outils = registre        │            │ décompte au coût réel     │
│  - propositions → panneau IA│            └──────────────────────────┘
└────────────────────────────┘
        │ mode « ma clé » : appel direct à OpenRouter (OAuth PKCE), sans relais
```

- **Nouvelle crate `plannercraft-assistant`**, sous `plannercraft-ui-egui` (couches : ni egui ni E/S dans `automation`) : client HTTP minimal (`reqwest`, qui passe par `fetch` dans le navigateur) et lecteur SSE tolérant (lignes de commentaire d'OpenRouter, `usage` dans le dernier morceau), boucle d'outils, conversion des appels d'outils en `Batch` du registre. Licences vérifiées par `cargo deny`.
- **Outils donnés au modèle** : un sous-ensemble du registre, schémas allégés (les 63 schémas pèsent environ 430 Ko, dont 148 Ko chacun pour `batch.apply` et `what_if`). Lectures libres (lister, chercher, `explain_date`, état de l'interface) ; écritures regroupées dans **un seul outil « proposer un lot »** au schéma réduit, qui ne s'applique jamais directement. Consignes système reprises de `server.rs::instructions()`.
- **Propositions** : une source interne au panneau Opérations IA (aujourd'hui alimenté par le canal Unix/TCP du relais MCP, `relay.rs`) ; desktop et web. Le web n'a pas de relais aujourd'hui : il faut une variante sans fil d'exécution (`spawn_local`).
- **Relais (mode payant)** : endpoint **métier** (« assistant de planning »), jamais un proxy générique compatible OpenAI (voir § 7) ; il fixe le modèle, `max_tokens`, le nombre de tours d'outils, les options de confidentialité ; il débite le solde au coût réel lu dans la réponse, multiplié par la marge ; en défense en profondeur, une clé OpenRouter plafonnée **par client mais gardée sur le relais** (Provisioning API keys : `limit`, `limit_reset`). Hébergement : infrastructure de l'éditeur (Coolify), base pour les comptes et soldes.
- **Mode « ma clé »** : OAuth PKCE d'OpenRouter (l'utilisateur se connecte, l'app reçoit une clé rattachée à son compte, révocable et plafonnable par lui ; retour sur `localhost` pour le desktop). CORS ouvert : le navigateur peut appeler OpenRouter directement. L'éditeur n'est alors pas dans la chaîne de traitement des données.

## 4. Ce que fait l'assistant (v1)

| Tâche | Outils |
|---|---|
| Créer un planning à partir d'une description (gabarits de chantier : maison, réhabilitation, tertiaire) | proposer un lot (tâches, durées, liens, lots en Texte1) |
| Modifier : décaler, lier, découper, renommer, affecter | proposer un lot (`task.shift`, `link.*`, `task.update`…) |
| Expliquer : pourquoi cette date, quel est le chemin critique, que se passe-t-il si… | `explain_date`, `what_if` (en lecture), lectures |
| Mettre en forme : regrouper par lot, mettre les jalons en gras | proposer un lot (`style.*`, `task.set_text_style`) |

Hors v1 : lecture de PDF (chantier J9, sans LLM pour les exports MS Project), voix, conversations partagées.

## 5. Secrets

- **Desktop** : trousseau du système (`keyring-core` et ses magasins macOS, Windows, Secret Service sous Linux ; avertissement et saisie à chaque session si le trousseau manque).
- **Web** : aucun stockage sûr dans un navigateur. Jeton de session en mémoire seulement par défaut, option « se souvenir » avec avertissement. En mode payant, le navigateur ne détient qu'un jeton de session vers le relais, jamais une clé de fournisseur.

## 6. Coûts (prix OpenRouter du 2026-11-06, à revérifier)

Une demande typique fait plusieurs tours d'outils (le contexte est renvoyé à chaque tour) : environ 100 000 tokens d'entrée cumulés et 2 000 de sortie.

| Modèle | Prix entrée / sortie ($/M) | Demande typique | Avec cache de prompt |
|---|---|---|---|
| Claude Sonnet 5.5 | 2 / 10 | ≈ 0,21 $ | ≈ 0,11 $ |
| Gemini 3.8 Flash | 0,825 / 4,125 | ≈ 0,09 $ | ≈ 0,04 $ |
| Claude Haiku 5.5 | 0,10 / 0,50 (prix à recontrôler) | ≈ 0,01 $ | < 0,01 $ |

Frais OpenRouter : 5,5 % sur l'achat de crédits (8 % en offre Business). Le prix de vente se calera sur le coût mesuré en conditions réelles, pas sur ces estimations.

## 7. Conditions, encaissement, obligations

- **OpenRouter** (conditions du 31/08/2026) : la § 5.1 prévoit l'intégration du service dans les produits de l'éditeur pour ses clients ; la § 7 interdit de « revendre l'accès API aux modèles » et de « transférer l'accès ». Lecture retenue (non juridique) : vendre une **fonction intégrée** est couvert, un proxy générique ou des clés brutes ne le sont pas. **Confirmation écrite d'OpenRouter à obtenir avant la vente.** Alternative en cas de refus : contrat direct avec un fournisseur (Anthropic, Mistral) sur le relais, même architecture.
- **Encaissement** : packs de crédits en paiement ponctuel, solde tenu par le relais.
  - Recommandé : **Stripe Managed Payments** (Stripe est le vendeur légal : TVA de tous les pays, litiges ; +3,5 % ; accepte les vendeurs en France ; code fiscal « AIaaS » prévu). Éligibilité à faire valider (examen de dossier, statut d'ouverture à revérifier).
  - Repli : Paddle (5 % + 0,50 $). Écarté : Lemon Squeezy (en cours d'absorption par Stripe) ; Stripe seul (TVA OSS à gérer soi-même).
- **RGPD** : le client professionnel est responsable de traitement, l'éditeur sous-traitant, OpenRouter sous-traitant ultérieur, les fournisseurs de modèles en aval. À produire : contrat de sous-traitance (DPA) de l'éditeur, liste des sous-traitants, finalité, durée de conservation, mention d'envoi à un fournisseur d'IA. Transfert hors UE encadré par les clauses types dans le DPA d'OpenRouter ; option « traitement en UE » (offre Business d'OpenRouter, ou endpoints UE de fournisseurs) pour les clients qui l'exigent. Option de pseudonymisation des noms de personnes et d'entreprises avant envoi.
- **À confirmer par des professionnels** : clause de revente (OpenRouter), DPA signé réservé ou non aux comptes Enterprise, seuils de TVA 2026 (expert-comptable), CGV.

## 8. Lots proposés

| Lot | Contenu | Prérequis |
|---|---|---|
| **A1** | Crate `plannercraft-assistant` : client HTTP/SSE desktop et web, mode « ma clé » (OAuth PKCE, trousseau), outils en lecture, réponses textuelles dans un volet Assistant | aucun (open source, gratuit) |
| **A2** | Outil « proposer un lot » branché sur le panneau Opérations IA (desktop et web), validation humaine, une annulation | A1 |
| **A3** | Gabarits et consignes métier (plannings de chantier), banc d'essai rejoué en CI avec des réponses enregistrées (comme les rejeux MCP) | A2 |
| **R1** | Relais payant (dépôt privé) : comptes, solde, endpoint métier, boucle d'outils bornée, confidentialité imposée | arbitrages § 9, réponse d'OpenRouter |
| **R2** | Encaissement (Stripe Managed Payments ou Paddle), CGV, DPA, page de prix | R1, validation juridique |
| **A4** | Mode « Compte PlannerCraft » dans l'app (connexion, solde affiché, achat de crédits par lien) | R1 |

Les lots A1 à A3 sont utiles seuls (mode « ma clé ») et ne demandent aucun arbitrage commercial.

## 9. Arbitrages du propriétaire

1. **Statut juridique et encaissement** : micro-entreprise ou société ? Stripe Managed Payments (recommandé) ou Paddle ?
2. **Modèle de prix** : packs de crédits prépayés (recommandé : aucun impayé, aucun abonnement à gérer) ou abonnement mensuel plafonné ?
3. **Modèle d'IA par défaut** : Sonnet 5.5 (qualité, ≈ 0,1–0,2 $ par demande) ou un modèle économique, avec choix laissé à l'utilisateur ?
4. **Option « données en UE »** dès le départ (offre Business d'OpenRouter, frais 8 %) ou plus tard ?
5. **Ordre** : commencer par A1–A3 (mode « ma clé ») avant la v1.0.0, ou après ?
