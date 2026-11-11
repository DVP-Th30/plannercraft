# Canal de contrôle de l'application (`plannercraft_automation::control`)

État au 2026-10-26 (lot L10 de J4, ADR-0002, ADR-0012 D9 ; relais J7 aux lots L2 et L3, ADR-0015 D9 à D11 ; transport TCP en boucle locale pour Windows au lot L8 de J8, ADR-0016 D12). Le canal relie `plannercraft mcp` à une application desktop ouverte : `plannercraft mcp` s'y **rattache** et lui relaie ses appels d'outils ; l'application les exécute sur la session de son document, **toute écriture après validation par l'utilisateur** (bandeau). En `--headless`, le canal garde le fichier (pas de double écrivain, `file.open_in_app`). Module absent en wasm (`#[cfg(not(target_arch = "wasm32"))]`) : la version web n'a pas de canal.

## 1. Déroulé

1. L'application crée un jeton (`new_token` : 32 octets de `getrandom`, 64 chiffres hexadécimaux), une file bornée (`sync_channel(RELAY_QUEUE)`, 64), lance l'écouteur du relais (`spawn_listener_in(dir, token, inbox, wake)`, `wake` = `ctx.request_repaint`) et publie son rendez-vous (`publish(&handle.rendezvous(canonical_project(path)))`). Le récepteur de la file est remis à `App::control_inbox`.
2. `plannercraft mcp`, avant d'ouvrir un fichier, appelle `find_live_for(path)`. Un rendez-vous vivant qui tient ce fichier ⇒ rattachement (`RelayClient`, lot L2) ; en `--headless`, erreur `file.open_in_app`.
3. À la fermeture, `RendezvousGuard` supprime le fichier et `ListenerHandle` arrête le fil et supprime la socket (`Drop`). Après un plantage, le prochain `find_live_for` nettoie.
4. **Document neuf** (autre numéro d'ouverture : autre fichier, même fichier rouvert, « Nouveau ») : l'application change le jeton de l'écouteur (`ListenerHandle::rotate_token`) puis republie son rendez-vous. Un agent rattaché au document précédent voit son `hello` refusé : `control.document_changed`, rien n'est fait, le serveur MCP se détache (`open_project` du fichier voulu rattache de nouveau). « Enregistrer sous » (même ouverture) garde le jeton. Relecture finale J7, relais F2 : un seul écouteur et un seul jeton servaient tous les documents, si bien que les appels d'un agent rattaché à A (UID compris) s'exécutaient en silence sur B après l'ouverture de B dans l'application (tests `relay_rotated_token_refuses_previous_document`, `relay_document_changed_detaches`, `desktop_new_document_rotates_token`).

## 2. Dossier d'état et rendez-vous

| Plateforme | Dossier (`state_dir()`) |
|---|---|
| Linux et autres Unix | `$XDG_STATE_HOME/plannercraft/control`, à défaut `~/.local/state/plannercraft/control` |
| macOS | `~/Library/Application Support/plannercraft/control` |
| Windows | `%LOCALAPPDATA%\plannercraft\control` |

- `PLANNERCRAFT_CONTROL_DIR` (chemin absolu) remplace le dossier : tests d'intégration de bout en bout, installations portables. Une valeur relative est ignorée, comme pour les variables XDG.
- Les fonctions `publish_in`, `find_live_for_in`, `spawn_hello_listener_in` prennent le dossier en paramètre (tests, sans toucher à l'environnement).
- Dossier créé en 0700 (parents compris) ; s'il existait avec d'autres droits, ils sont resserrés à 0700.
- Un fichier `<pid>-<n>.json` par document, en 0600 (`OpenOptionsExt::mode`), écrit dans `<pid>-<n>.tmp` puis renommé : un lecteur ne voit jamais un rendez-vous à moitié écrit.
- Contenu (`Rendezvous`, `deny_unknown_fields`) : `{"v":1,"pid":1234,"endpoint":"/…/1234-0.sock","token":"<64 hex>","project":"/chemin/canonique.xml"}` ; sous TCP (Windows), `"endpoint":"tcp:127.0.0.1:50123"` (C-CTL-11). La socket Unix garde le chemin nu du J7 (octets du rendez-vous inchangés sous Linux et macOS) ; la lecture (`Endpoint::from_rendezvous`) prend le chemin nu, `unix:<chemin>` et `tcp:<ipv4 de boucle locale>:<port>` ; toute autre adresse est `control.bad_endpoint`. `project` est le chemin rendu par `std::fs::canonicalize` (`canonical_project`) ; un chemin non UTF-8 ne se publie pas. Le `Debug` de `Rendezvous` et de `ListenerHandle` masque le jeton.

### `find_live_for(path)`

- Canonicalise `path` (fichier absent ⇒ `None`), lit les `.json` du dossier dans l'ordre de leur nom (64 Kio au plus chacun).
- Fichier de version 1 illisible : supprimé. Autre version, ou sans `v` : ignoré et **conservé** (appartient à une autre version de l'application). `.tmp` : ignoré.
- Rendez-vous qui désigne `path` : sondé par `probe` (`hello` avec son jeton, connexion et réponse en 2 s chacune), même sonde sur les deux transports. Réponse `ok` ⇒ rendu.
  - **Socket Unix** : fichier supprimé seulement sur une preuve de mort : refus du jeton (rendez-vous périmé), socket absente ou sans écouteur (`ECONNREFUSED`). Tout autre échec (délai, `control.busy` parce que l'application sert déjà ses 8 connexions, connexion fermée sans réponse, réponse illisible) ⇒ application tenue pour vivante, rendez-vous gardé et rendu : la garde ADR-0002 n'est jamais levée par une application occupée (test `control_busy_app_kept_live`).
  - **TCP (C-CTL-13, J8 lot L8)** : seule une réponse de l'application garde le rendez-vous (`hello` accepté, ou `control.busy` : application vivante au maximum de ses connexions, tests `control_busy_app_kept_live_tcp`) ; toute sonde qui échoue en 2 s le supprime : port fermé, programme muet sur le port, connexion fermée sans réponse, jeton refusé (test `c_ctl_13_dead_rendezvous_removed_by_probe`). Raison : un port libéré peut être repris par n'importe quel programme, une connexion acceptée ne prouve donc pas que l'application vit (contrairement à une socket dans un dossier 0700). Remplace la règle des 24 h de Windows (J4) : une application plantée ne bloque plus son fichier. L'écouteur répond à `hello` depuis son propre fil, même quand l'interface est occupée.
- Adresse illisible ou hors boucle locale (`control.bad_endpoint`) : rendez-vous supprimé, pour ce fichier ou un autre.
- Rendez-vous d'un autre fichier : supprimé si sa socket Unix n'existe plus (nettoyage au passage, sans connexion) ; un rendez-vous TCP d'un autre fichier n'est ni sondé ni supprimé.

## 3. Protocole

JSON par lignes (`\n`, un `\r` final toléré), 8 Mio au plus par ligne (`MAX_LINE_BYTES`) : `read_line_bounded` refuse dès que la borne est dépassée, sans garder plus en mémoire. Requête `Frame` (`deny_unknown_fields`) :

```json
{"v":1,"id":1,"method":"hello","params":{"token":"<64 hex>"}}
```

`params` absent vaut `null`. Méthodes : `hello`, `query`, `execute`, `ui_state`, `screenshot` (J7 : `undo`, `redo`, `history` retirées, remplacées par `execute { tool: "history.undo" }`). Réponses `Reply` (enum externe, `snake_case`) :

```json
{"ok":{"id":1,"result":{"v":1,"app":"plannercraft","version":"0.1.0","pid":1234,"methods":["hello","query"]}}}
{"err":{"id":2,"error":{"code":"control.bad_token","message":"jeton refusé"}}}
{"pending_approval":{"id":3}}
```

`pending_approval` (J7) annonce une écriture en attente de validation : la réponse finale (`ok` ou `err`) suit sur la même connexion, avec le même `id`. Une erreur d'outil passe avec tous ses champs (`code`, `message`, `op_index`, `field`, `subject`, `diagnostics`, aplatis dans `WireError::details`) : même objet que l'erreur structurée du serveur MCP local.

### Relais (J7, lot L3)

| Méthode | `params` | Appel poussé dans la file |
|---|---|---|
| `query` | `null`, ou `{ command?, params? }` (défaut `project.summary`, `{}`) ; `command` du genre `Read` ou `Analysis` seulement, sinon `control.unsupported` | `Execute { tool: command, params }` |
| `execute` | `{ tool, params?, approval_timeout_ms? }` (`params` absent : `{}` ; `approval_timeout_ms` : délai de validation voulu par le client, entier > 0) | `Execute { tool, params }` |
| `ui_state` | `null` ou `{}` | `UiState` |
| `screenshot` | `{ scale_down? }` | `Screenshot { scale_down }` |

Écouteur (`spawn_listener_in`, réglages `ListenerOptions` par `spawn_listener_with`) :
- l'écouteur ne touche jamais la session : un fil par connexion pousse un `RelayCall { id, kind, state: Arc<CallState>, reply }` par `try_send`, réveille l'interface (`Waker`), puis attend les réponses sur `reply` ; une requête à la fois par connexion ;
- file pleine ⇒ `control.busy`, connexion maintenue ; application partie (file fermée) ⇒ `control.app_gone` ; plus de 8 connexions ⇒ `control.busy` (id 0) puis fermeture : le client ne se détache pas (relecture finale J7, relais F4 ; une fermeture muette se lisait « application partie ») ; `hello` attendu 5 s au plus, puis 30 s d'inactivité ;
- délai `approval_timeout` (120 s par défaut, `APPROVAL_TIMEOUT`) depuis la mise en file, ou `approval_timeout_ms` du client s'il est plus court (relecture finale J7, relais F1 : avec `--approval-timeout 30`, l'agent apprenait « rien n'a été appliqué » à 40 s alors que le bandeau restait acceptable jusqu'à 120 s, d'où un doublon à la relance) : `CallState::expire()` ; s'il réussit, `Cancel(id)` est poussé dans la file avec l'état de l'appel abandonné (l'application retire **ce** bandeau, pas celui d'un autre agent au même id) et l'agent reçoit `control.approval_timeout` (après `pending_approval`) ou `control.app_gone` (aucune réponse) ; s'il échoue, l'application vient de prendre l'appel et sa réponse est attendue `reply_grace` (10 s) au plus. La course Accepter/délai est donc tranchée par `CallState` (`Pending` → `Taken` ou `Expired`, une seule fois) : rien n'est appliqué après l'expiration. Pendant l'attente, la connexion est surveillée (lecture non bloquante tous les 100 ms) : client parti (délai du client écoulé, processus mort) ⇒ appel expiré et `Cancel`, plus rien n'est acceptable.

Application (`plannercraft-ui-egui/src/relay.rs`, file vidée en **début d'image**, avant le dessin) :
- appel déjà expiré dans la file : ignoré ; `Cancel(id)` : écriture `id` **de même état** retirée (deux `plannercraft mcp` numérotent leurs appels de la même façon, relecture finale J7, relais F3) ; Accepter et Refuser ne visent que le bandeau affiché (tête de file) ;
- `app.ui_state` (ou méthode `ui_state`) ⇒ `{ ui_state, selection (UID triés), active, view, visible_rows: [première, dernière], dirty, generation, opening }` ; `ui_state` est l'`UiState` **sans** `recent` (les chemins des fichiers récents sont d'autres projets de l'utilisateur, jamais donnés à l'agent : relecture finale J7, relais F7) ;
- `app.ui` ⇒ actions d'interface (`select`, `scroll_to`, `zoom`, `view`, `theme`, `language`), appliquées en fin d'image, sans historique ; au moins un champ ; UID absent de la vue ⇒ `task.not_found` ; réponse `{ applied }` ;
- `app.screenshot` (ou méthode `screenshot`) ⇒ capture de la fenêtre (desktop, J7 lot L14, C-CTL-08) : `{ mime_type, data (PNG en base64), width, height, scale_down, bytes }`, réduite de moitié `scale_down` fois (0 à 2) puis encore, deux fois au plus, tant que le base64 dépasse 6 Mio (sinon `control.screenshot_too_large`) ; aucune image en 5 s, ou capture non activée (kittest, web) ⇒ `control.screenshot_unavailable` (`docs/features/ui.md` § 1.21) ;
- id inconnu ⇒ `param.invalid` ; `project.open`, `project.new`, `project.import_csv` ⇒ `control.not_relayable` (C-CTL-05) ; aucun document ⇒ `session.none` ;
- lectures (genres `Read`, `Analysis`, `history.list`, et toute écriture en `dry_run: true`) ⇒ `workspace::call_with` sur la session du document, réponse immédiate : même JSON que `Workspace::call` local (C-CTL-09) ;
- écritures (genre `Write`, `batch.apply`, `project.save`, `project.export_mspdi`, `history.undo`, `history.redo`) ⇒ validation (voir § 3.1).

#### 3.1 Validation d'une écriture (H-UI-27, C-CTL-02 à C-CTL-04, C-CTL-07)

1. `project.save`/`project.export_mspdi` avec un `path` tenu par une autre application (rendez-vous vivant d'un autre pid) ⇒ `file.open_in_app` (garde ADR-0002 côté application : le serveur rattaché ne la vérifie plus).
2. Lot ou écriture unitaire : `call_with` avec `dry_run: true` ; refus ⇒ erreur rendue aussitôt, aucun bandeau. Sinon `pending_approval`, et `PendingApproval { id, tool, params, preview: ApprovalSummary, generation, opening, now }` entre dans la file des bandeaux. `ApprovalSummary { changed_tasks, touched_tasks, finish_delta (minutes), finish_delta_text (texte du moteur), critical_changed, warnings }` est lu dans le diff du `dry_run` (R-163) : l'interface ne calcule rien. Enregistrer, exporter, annuler, rétablir : sans aperçu (outil et chemin seulement).
3. Bandeau : « L'agent propose sur « maison.xml » : task.update — 3 tâches modifiées · 3 dates déplacées, fin +2j, chemin critique modifié » (tâches touchées `diff.touched_total` et dates déplacées `diff.changed_total`, R-163 amendée le 2026-11-11), Accepter / Refuser, « N autres propositions en attente » ; un seul à la fois. Le bandeau ne prend pas le focus clavier (relecture finale J7 : un Entrée ou un Espace tapé au moment où il paraît validait l'écriture sans lecture) : Ctrl/Cmd+Entrée accepte (hors champ de texte), Échap refuse quand un bouton du bandeau a le focus ; sinon, la souris. Le focus revient à la table quand la file est vide.
4. Accepter : appel expiré ⇒ rien ; document fermé ou remplacé (numéro d'ouverture) ⇒ `control.rejected` ; génération changée depuis l'aperçu ⇒ nouvel aperçu, rien d'appliqué, le bandeau reste (C-CTL-07) ; sinon `CallState::claim()` puis `call_with` avec `source: Agent` et le « maintenant » de la demande (règle 5) : une entrée d'historique d'origine `Agent` (« Annuler : agent — … » au menu Édition), puis `ok`.
5. Refuser ⇒ `control.rejected` ; projet, historique, génération et drapeau modifié intacts (C-CTL-03).
6. Panneau d'opérations IA (J8 lot L12, H-UI-41, `ui.md` § 1.27) : à l'arrivée, le volet latéral s'ouvre et `PendingApproval` gagne `diff: Option<DiffView>` (le même lot simulé par `Session::simulate`, diff détaillé, au plus 200 lignes) et `document` (nom du document visé) ; le volet porte alors la décision à la place du bandeau, mêmes règles de clavier. Chaque issue entre dans l'historique de session (`Relay::history`, 50 au plus) : acceptée, refusée, expirée (délai, client parti, `Cancel`), périmée (document fermé ou remplacé), échouée (code du moteur). Réponses du canal inchangées (C-CTL-02 à C-CTL-07).

### Écouteur J4 (`spawn_hello_listener_in`, gardé pour les tests des surfaces)

- Socket Unix `<pid>-<n>.sock` en 0600 dans le dossier d'état ; un fil `plannercraft-control` accepte, un fil par connexion, 8 connexions simultanées au plus (au-delà, `control.busy` puis fermeture). `hello` attendu 5 s au plus **depuis l'acceptation** (échéance absolue : une ligne envoyée octet par octet ne la repousse pas ; ligne de 16 Kio au plus avant le `hello`), puis connexion inactive fermée après 30 s ; écriture bornée à 5 s. Connexions sans `hello` accepté : 4 au plus, la plus ancienne évincée (`control.busy`, id 0, puis coupure) à l'arrivée d'une cinquième, pour qu'elles ne prennent jamais toutes les places (relecture finale de J8, sécurité F1). Avant le `hello`, la connexion est lue par tranches de 50 ms et son fil vérifie à chaque tranche une marque d'éviction : sous Windows, la coupure (`shutdown`) par le fil d'acceptation ne réveille pas la lecture bloquée d'un autre fil, et les évincées gardaient leur place jusqu'à l'échéance du `hello` (5 s) — le client muni du jeton recevait `control.busy` (CI Windows de la PR #7). Les tests e2e à socket Unix placent la socket dans un dossier court de `temp_dir()` : sous macOS, le dossier temporaire canonique (`/private/var/folders/…`) dépassait avec le nom du test les 104 octets de `sun_path`.
- Première requête : `hello` avec le jeton. Comparaison en temps constant (décodage hexadécimal puis boucle XOR sur 32 octets ; un jeton mal formé suit la même boucle). Majuscules acceptées.
- `query` : `params` nul, `{}` ou `{"command":"project.summary"}` ⇒ `SummaryView` du document (ou `null`). Toute autre lecture ⇒ `control.unsupported`.
- `execute`, `ui_state`, `screenshot` ⇒ `control.unsupported`, connexion maintenue.

| Code | Cas | Connexion |
|---|---|---|
| `control.bad_token` | `hello` sans jeton ou jeton faux | fermée |
| `control.unauthenticated` | requête avant un `hello` accepté | fermée |
| `control.line_too_long` | ligne de plus de 8 Mio (id 0), après `hello` ; avant, fermée sans réponse (C-CTL-14) | fermée |
| `control.invalid_frame` | après `hello` : JSON invalide, champ ou méthode inconnus, `params` mal formés ; `id` repris s'il se lit. **Avant `hello`** (J8, C-CTL-14) : aucune réponse, connexion fermée | maintenue (après `hello`) |
| `control.unsupported_version` | `v` ≠ 1 | idem |
| `control.unsupported` | méthode non servie par l'écouteur J4, `query` d'une commande qui n'est pas une lecture (relais) | maintenue |
| `control.busy` | file de l'application pleine (relais) ; plus de 8 connexions (id 0) | maintenue ; fermée (connexions) |
| `control.internal` | sérialisation impossible du résumé | maintenue |

Les codes sont dans `control::codes` (propres au fil, distincts de `error::codes` qui couvre les commandes). Codes J7 des commandes relayées (`error::codes`) : `control.rejected`, `control.approval_timeout`, `control.not_relayable`, `control.not_attached`, `control.app_gone`, `control.screenshot_unavailable`, `control.screenshot_too_large` ; relecture finale J7 : `control.busy` (application occupée, rattachement gardé), `control.document_changed` (jeton du document précédent refusé, détachement).

### Transport TCP en boucle locale (J8, ADR-0016 D12 ; contrat L0, implémentation L8)

Sous Windows, le canal passe par TCP en boucle locale (std seul, sans `unsafe` ni dépendance, même jeton par document, rotation J7 inchangée) ; ailleurs, la socket Unix reste (`Transport::NATIVE`). Le service est le même sur les deux transports : protocole, codes, délais, file, 8 connexions.

- `control::tcp` : `bind_loopback()` lie `127.0.0.1:0` (port choisi par le système), `local_endpoint` rend l'adresse réelle (refuse un écouteur lié ailleurs), `accept_peer(&SocketAddr)` n'accepte que la boucle locale (IPv4 `127/8`, `::1`, IPv4 portée en IPv6) : un autre pair est fermé aussitôt, sans lecture ni réponse (C-CTL-12).
- `Endpoint` (`unix:<chemin>` ou `tcp:127.0.0.1:<port>`, `parse` / `render`, `from_rendezvous` / `to_rendezvous`, refus `control.bad_endpoint`) et `Endpoint::connect(timeout)` → `ControlStream` : **client commun** aux deux transports (sonde, `RelayClient`, rattachement au démarrage `single_live_in`), disponible sur tous les systèmes (C-CTL-11).
- Écouteur : `spawn_listener_in` / `spawn_listener_with` / `spawn_hello_listener_in` prennent le transport de la plateforme ; `spawn_listener_on` / `spawn_hello_listener_on` le choisissent (tests du transport TCP sous Linux et macOS). Sous TCP, `ListenerHandle::endpoint()` vaut `tcp:127.0.0.1:<port>` et aucun fichier de socket n'est créé ; à l'arrêt, une connexion de réveil débloque `accept`.
- Première ligne qui n'est pas une trame (`GET / HTTP/1.1` d'une page web, texte, ligne vide, JSON incomplet, ligne de plus de 8 Mio) ⇒ connexion fermée **sans réponse**, sur les deux transports (C-CTL-14) : une page web qui viserait le port n'apprend rien, pas même qu'un service y écoute.
- Fermeture TCP : fin d'émission puis lecture de ce que le pair a déjà envoyé (100 ms et 64 Kio au plus) avant de fermer : fermer avec des octets non lus enverrait une remise à zéro (`RST`) qui peut effacer chez le pair la dernière réponse (`control.busy`, `control.bad_token`) avant qu'il ne la lise. La sonde lit de même la réponse même si l'envoi du `hello` a échoué.

| Id | Contrat | Lot | Tests |
|---|---|---|---|
| C-CTL-11 | Écouteur TCP lié à `127.0.0.1:0` uniquement ; rendez-vous `endpoint = "tcp:127.0.0.1:<port>"` ; client commun aux deux transports | L0 (adresse), L8 | `c_ctl_11_bind_loopback_only`, `c_ctl_11_endpoint_roundtrip`, `c_ctl_11_endpoint_refuses_non_loopback_and_garbage`, `endpoint_parse_roundtrip`, `endpoint_bad_refused`, `c_ctl_11_rendezvous_and_common_client` (et `_unix`) |
| C-CTL-12 | Pair dont l'adresse n'est pas en boucle locale ⇒ fermé sans lecture (fonction `accept_peer` testée sur adresses construites) | L8 | `c_ctl_12_non_loopback_peer_refused` |
| C-CTL-13 | Sous transport TCP, rendez-vous dont la sonde `hello` échoue en 2 s ⇒ retiré (remplace la règle des 24 h) | L8 | `c_ctl_13_dead_rendezvous_removed_by_probe`, `control_busy_app_kept_live_tcp` |
| C-CTL-14 | Première ligne qui n'est pas une trame valide (ex. `GET / HTTP/1.1`) ⇒ connexion fermée sans réponse | L8 | `c_ctl_14_http_first_line_closed` (et `_unix`) |

C-CTL-01 à C-CTL-10 sous TCP : les tests de l'écouteur et du client de `tests/control.rs` sont rejoués sur les deux transports (suffixe `_tcp`), et le scénario `relay_j7` l'est avec le vrai `plannercraft mcp` (`relay_app_real_mcp_tcp`, P47) ; les tests de l'application avec file en mémoire (`tests/ui/relay.rs`) ne dépendent pas du transport.

## 4. Sécurité

- Seul l'utilisateur lit le dossier (0700) et le jeton (0600) ; la socket est elle aussi en 0600. Le jeton protège en plus contre une socket réutilisée par un autre processus au même chemin.
- Le jeton n'est jamais journalisé (`Debug` masqué).
- Pas de TCP en J4 (ADR-0002 : seulement sur demande explicite, lié à 127.0.0.1). J8 (ADR-0016 D12) : TCP sous Windows seulement, lié à `127.0.0.1` ; pair hors boucle locale fermé sans lecture (C-CTL-12) ; première ligne qui n'est pas une trame fermée sans réponse (C-CTL-14). La boucle locale n'est pas privée à l'utilisateur comme un dossier 0700 : un autre compte de la machine peut se connecter au port ; le jeton (32 octets aléatoires, lu dans le rendez-vous sous `%LOCALAPPDATA%`, comparé en temps constant) reste alors la seule clé, et toute requête avant un `hello` accepté ferme la connexion. **Disponibilité** (relecture finale de J8) : un tel compte pouvait bloquer le relais sans jeton en tenant les 8 places par des connexions muettes ou un `hello` envoyé octet par octet ; depuis, échéance absolue du `hello` et 4 connexions sans `hello` au plus (évincées de la plus ancienne). Risque restant, accepté (déni de service local, ni fuite ni écriture) : un compte qui rouvre des connexions en continu peut évincer un client légitime pendant les quelques millisecondes qui précèdent son `hello` ; il reçoit `control.busy` et réessaie sans se détacher ; si la coupure le devance (course entre le refus et la fin de connexion), il lit une connexion fermée et se détache comme d'une application partie, jusqu'à la sonde suivante. Tests : `relay_silent_connections_never_block*`, `hello_drip_deadline_absolute*`.

## 5. Tests

`crates/plannercraft-automation/tests/control.rs` : `control_frame_roundtrip`, `control_line_too_long_refused` (lecteur et socket), `control_rendezvous_mode_0600` (Unix), `control_stale_rendezvous_removed`, `control_hello_wrong_token_refused`, `control_find_live_matches_canonical_path` (détour `sub/..` et lien symbolique), `control_unsupported_methods_in_j4` ; relais J7 : `c_ctl_01_bad_token_closes`, `relay_queue_full_busy`, `c_ctl_04_listener_expires_and_cancels`, `relay_methods_become_calls`, `relay_call_with_marks_agent_source` ; relecture finale J7 : `relay_asked_timeout_shorter_wins`, `relay_client_hangup_expires_call`, `relay_connections_full_busy`, `relay_rotated_token_refuses_previous_document`. J8 lot L8 : chaque test de l'écouteur et du client a sa variante TCP (`…_tcp`, exécutée sur tous les systèmes, Windows compris par le job `tests-matrix`), plus `c_ctl_11_rendezvous_and_common_client`, `c_ctl_13_dead_rendezvous_removed_by_probe`, `c_ctl_14_http_first_line_closed` ; les tests des fichiers de socket (`control_rendezvous_mode_0600`, `control_stale_rendezvous_removed`, `control_find_live_matches_canonical_path`) restent Unix ; `control_find_live_by_age_without_listener` (règle des 24 h) est retiré. Tests unitaires du jeton dans `control/mod.rs`, de l'adresse dans `control/endpoint.rs`, de `bind_loopback` et `accept_peer` dans `control/tcp.rs` (`c_ctl_11_bind_loopback_only`, `c_ctl_12_non_loopback_peer_refused`).

`crates/plannercraft-ui-egui/tests/ui/e2e.rs` (J7 lot L16, Unix) : `relay_app_real_mcp`, application kittest avec écouteur réel et rendez-vous publié dans un dossier temporaire (`PLANNERCRAFT_CONTROL_DIR`), **vrai binaire** `plannercraft mcp` rattaché au démarrage (un seul rendez-vous vivant), scénario `tests/mcp/scenarios/relay_j7.jsonl` (11 appels : écriture acceptée, refusée, édition de l'utilisateur pendant une attente puis double Accepter (C-CTL-07), `ui_state`, `ui_action`, `screenshot` ⇒ `control.screenshot_unavailable`, `open_project` du fichier tenu ⇒ résumé rattaché, `undo` relayé, application fermée ⇒ `control.app_gone` puis `session.none` (C-CTL-06)) ; résultats et journal du serveur égaux au scénario ; P47 contre un rejeu local. J8 lot L8 : `relay_app_real_mcp_tcp`, même scénario relayé en TCP sur la boucle locale (transport de Windows rejoué sous Linux) ; seul le détail du système dans `control.app_gone` (socket absente / port fermé) diffère et est ramené à la forme Unix avant la comparaison. Binaire : `PLANNERCRAFT_MCP_BIN`, sinon à côté de `deps/` ; `PLANNERCRAFT_RELAY=required` en CI (job `ui`).

`crates/plannercraft-ui-egui/tests/ui/ai_panel.rs` (J8 lot L12, Unix pour le dernier) : `relay_j8_panel_real_mcp`, même montage que `relay_app_real_mcp` (fonctions de `e2e::relay_app` partagées), scénario `tests/mcp/scenarios/relay_j8.jsonl` (ajout de tâche accepté par un clic dans le volet, renommage refusé dans le volet, durée acceptée par Ctrl/Cmd+Entrée, historique) ; journal égal au scénario, historique du volet [acceptée, refusée, acceptée], un diff affiché par écriture. Lot L16 (vérification 6) : `relay_j8_panel_real_mcp` et `relay_j8_panel_real_mcp_tcp` (même scénario, même journal, socket Unix et TCP en boucle locale) ; le scénario gagne `apply_style` (Présentation) et `export_pdf` validés dans le volet, puis une écriture laissée sans réponse jusqu'à l'expiration (`control.approval_timeout`, délai de validation de 4 s pour ce test, écouteur et serveur : `e2e::relay_app::wired_with`) ; historique du volet [acceptée, refusée, acceptée, acceptée, acceptée, expirée], style Présentation dans le document de l'application, PDF écrit.

`crates/plannercraft-ui-egui/tests/ui/relay.rs` (application réelle, file en mémoire, et socket réelle sous Unix) : `c_ctl_02_pending_then_ok` (et `…_over_socket`), `c_ctl_03_reject_leaves_state`, `c_ctl_04_timeout_then_accept_does_nothing` (et `c_ctl_04_timeout_over_socket`, `c_ctl_04_client_timeout_shorter_than_listener`), `relay_cancel_same_id_other_agent_kept`, `c_ctl_05_open_not_relayable`, `c_ctl_07_stale_generation_new_preview`, `c_ctl_09_read_same_as_local`, `h_ui_27_banner_keyboard`, `h_ui_27_banner_enter_or_space_never_accepts`, `relay_agent_entry_single_undo`, `relay_ui_state_and_ui_actions`, `relay_expired_in_queue_is_skipped`, `p47_relay_replays_like_local` (30 appels). Fluidité : `approval_banner` dans `tests/perf.rs` (release, ignoré par défaut).

## 6. Limites et suites

- Capture (`app.screenshot`) : faite en lot L14 (§ 1.21 de `ui.md`) ; non vérifiée sur une fenêtre réelle par un test automatique (la CI n'a pas d'écran ; sous kittest, `control.screenshot_unavailable`, vérifié par `relay_app_real_mcp`). Panneau IA : fait en J8 lot L12 (diff détaillé par simulation du lot, historique de session) ; ni barres fantômes dans le Gantt (reportées, ADR-0016 § 0), ni identité de l'agent dans l'historique (le canal n'en transmet pas).
- `open_project` relayé : le fichier tenu garde le rattachement (résumé relayé, `attached: true`) ; un autre fichier détache ; `control.not_relayable` ne s'observe que par `execute project.open` sur le fil (lot L16, ADR-0015). Le résumé relayé est demandé avec des paramètres `{}` (et non `null`, refusé par `NoParams` côté application : défaut du lot L2 trouvé et corrigé par `relay_app_real_mcp`).
- Essai réel n° 4 (Claude rattaché à l'application ouverte) : hors lot, session principale.
- La garde d'un enregistrement relayé sonde les rendez-vous (`find_live_for_in`, 2 s au plus par application) sur le fil de l'interface, avant l'affichage du bandeau.
- Windows (J8 lot L8) : relais en TCP sur la boucle locale ; le transport est testé sous Linux (et sous Windows par `tests-matrix` pour les tests de `plannercraft-automation`), mais le scénario avec le vrai `plannercraft mcp` et l'application kittest (`relay_app_real_mcp_tcp`) ne tourne que sous Unix (job `ui`) ; aucun essai sur un Windows réel avec un agent.
- Le rendez-vous ne dit qu'un projet par fichier ; l'application J7 n'a qu'un document à la fois (un jeton par document, changé à chaque ouverture). Une application à plusieurs documents demanderait un écouteur (ou un jeton) par document.
