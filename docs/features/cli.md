# Ligne de commande `plannercraft` (`apps/plannercraft-cli`)

État au 2026-10-19 (lot L9 de J4, ADR-0012 D1, brief § 7). Le binaire expose les opérations du MCP pour les scripts et les harnais d'agents. Chaque sous-commande ouvre un `Workspace` et appelle `Workspace::call` avec les ids du registre : aucune logique de planification ici (règle 3), `--json` rend exactement les vues JSON du MCP (`docs/features/workspace.md`).

## 1. Sous-commandes

| Commande | Ids du registre | Sortie |
|---|---|---|
| `plannercraft summary <plan> [--json]` | `project.open`, `project.summary` | `SummaryView` ; texte : nom, début, fin, durée, comptes |
| `plannercraft tasks <plan> [--critical] [--late] [--from J] [--to J] [--resource ID] [--limit N] [--json]` | `task.list` | `ListView` ; texte : tableau ID, nom (indenté), durée, début, fin, marge totale, `*` si critique ; `--late` suit R-236 (J6 : fin après l'échéance ou la ligne de base 0, non achevée à fin ≤ date d'état, non commencée à début < date d'état ; date d'état du projet, sinon la date du jour ; aide `--help` du même texte, `cli_tasks_late_help_follows_r236`) |
| `plannercraft resources <plan> [--overallocated] [--json]` | `resource.list` | `ResourceListView` (toutes pages) ; texte : tableau ID, nom, type, capacité, travail, pic, `!` si surutilisée |
| `plannercraft usage <plan> [--resource ID]… [--by day\|week\|month] [--from D] [--to D] [--json]` | `analysis.resource_usage` | `UsageView` ; texte : une section par ressource, une ligne par période (début, travail), `!` si surutilisée |
| `plannercraft apply <plan> <ops.json> [--dry-run] [-o out.xml] [--json]` | `batch.apply`, `project.save` | `OutcomeView` ; texte : appliqué ou simulé, fin avant → après, tâches décalées, fichier écrit |
| `plannercraft render <plan> -o x.svg [--from J] [--to J] [--critical] [--zoom auto\|day\|week\|month\|quarter]` | `render.gantt` | SVG écrit ; texte : lignes affichées, masquées, octets |
| `plannercraft import-csv <in.csv> -o out.xml` | `project.import_csv`, `project.save` | MSPDI écrit ; texte : tâches, liens, diagnostics (détail sur la sortie d'erreur) |
| `plannercraft mcp [--journal F] [--open plan.xml] [--headless] [--approval-timeout S] [--http ADDR --root DOSSIER [--allow-remote] [--token-file F]]` | `plannercraft_mcp::serve_stdio`, `plannercraft_mcp::serve_http` | serveur MCP stdio (lot L8) ou HTTP « streamable HTTP » sur `http://ADDR/mcp` (lot L17) |
| `plannercraft explain <plan> --task ID [--finish] [--depth N] [--json]` | `project.open`, `analysis.explain_date` | `ExplainView` (J6, lot L7, R-230 à R-233) ; texte : résumé du moteur dans la langue choisie, dates au plus tard, marges, criticité |
| `plannercraft style list\|get <plan> [--json]` ; `plannercraft style apply <plan> (--template T \| --patch F.json \| --reset) [--dry-run] [-o out.xml] [--json]` | `style.list`, `style.get`, `style.apply`, `project.save` | J8, lot L6 : `StyleListView` (texte : un modèle par ligne, `*` devant celui du document), `StyleView` (texte : modèle et provenance, sections surchargées, contrastes insuffisants sur la sortie d'erreur), `OutcomeView` (texte : appliqué ou simulé, fichier écrit) |
| `plannercraft export-pdf <plan> <sortie.pdf> [--style T] [--paper A4\|A3\|letter\|tabloid] [--landscape\|--portrait] [--pages-wide N] [--from D --to D] [--critical] [--outline-max N] [--json]` | `project.export_pdf` | J8, lot L7 : `PdfView` (texte : chemin, pages, tâches, octets, échelle en % ; caractères absents des polices sur la sortie d'erreur, `pdf.glyph_missing`) ; le plan n'est pas modifié, le PDF est écrit de façon atomique ; `--pages-wide` hors de 1 à 20 : usage (code 2) ; plage inversée : `param.invalid` (code 1) ; plus de 1 000 pages : `pdf.too_many_pages` (code 1) |

- Dates d'option : jours `AAAA-MM-JJ` (`--from`, `--to`) ; pour `usage`, aussi `AAAA-MM-JJTHH:MM` (un jour seul vaut minuit, `--to` exclu) ; une valeur mal formée est une erreur d'usage (2).
- `--resource ID` (`tasks`, `usage`, J5) : ID de la colonne ID de `plannercraft resources` (ID de MS Project, comme l'écrivain MSPDI : 1, 2… dans l'ordre des ressources, la ressource UID 0 cachée d'un fichier MS Project ayant l'ID 0 sans être comptée ; `plannercraft_core::resource_id`), converti en UID ; ID inconnu : `resource.not_found` (code 1).
- `tasks` sans `--limit` lit toutes les pages (500 par page) et rend une seule `ListView` sans `next_cursor` ; avec `--limit N` (1 à 500), une seule page telle que le MCP la rend (`next_cursor` s'il en reste) ; `N` hors plage est une erreur d'usage (code 2) contrôlée par clap avant la lecture du plan (test `cli_usage_errors_exit_2`).
- `explain` (J6) : `--task ID` est l'ID de la colonne ID de `plannercraft tasks`, converti en UID par `Project::task_by_id` (ID 0 : récapitulative du projet) ; ID inconnu : `task.not_found` (code 1) ; `--depth` de 1 à 50 (défaut 5), hors plage : usage (2) contrôlé par clap ; `--finish` explique la fin. Le texte n'ajoute aucune date : il recopie `summary_fr` ou `summary_en` et le côté au plus tard de la vue.
- `apply` : le fichier de lot a le format versionné unique (`{"format_version":1,"lang":"fr","ops":[{"op":"task.add",…}]}`, ADR-0012 D6), lu au plus 64 Mio. `--dry-run` n'écrit rien, même avec `-o` (un `"dry_run": true` dans le fichier aussi). `-o` écrit le résultat ailleurs et laisse `<plan>` intact ; sans `-o`, `<plan>` est remplacé de façon atomique (`files::write_atomic` : temporaire du même dossier, `fsync`, renommage). Un lot refusé ne touche aucun fichier.
- `style apply` (J8) : `--template` (`classique`, `sobre`, `contraste-eleve`, `presentation`, `chantier`, `impression-nb`), `--patch` (surcharge JSON lue au plus 64 Kio, fusionnée avec celle du document ou posée sur `--template`), `--reset` (exclusif des deux autres : usage, code 2) ; rien de tout cela : `param.invalid` (code 1). Écriture comme `apply` (`--dry-run`, `-o`, remplacement atomique) ; le fichier porte `<?plannercraft-style?>` dès qu'un style est choisi (H-R12), plus rien après `--reset`.
- `render` : 200 lignes au plus (borne de `render.gantt`), thème clair, libellés dans la langue de la CLI ; une troncature (plage, taille) est signalée sur la sortie d'erreur.
- `import-csv` : séparateur et langue des en-têtes détectés (R-144 à R-146), projet planifié depuis « maintenant » (hypothèse de `project.import_csv`).
- `mcp` : langue par défaut anglaise (ADR-0012, arbitrage 6) sauf `--lang` explicite ; sans `--headless`, relais vers l'application PlannerCraft qui tient le fichier (J7, `docs/features/mcp.md` § 3 bis) ; `--headless` coupe ce relais et garde le refus ADR-0002 (`file.open_in_app`) ; `--approval-timeout S` (1 à 3 600, défaut 120 ; hors bornes : code 2) borne l'attente de la validation d'une écriture relayée.
- `mcp --http ADDR` (lot L17) : `:8080` = `127.0.0.1:8080`, aussi `localhost:P`, `IP:P`, `[IPv6]:P` (pas d'autre nom d'hôte) ; HTTP/1.1, chemin `/mcp` (404 ailleurs) ; l'adresse liée est écrite sur la sortie d'erreur (port `0` : port libre). `--root` obligatoire : tout chemin de fichier y est confiné (`file.outside_root`, `--open` compris) ; toutes les sessions HTTP partagent le même espace de travail. `Host` limité à la boucle locale, `Origin` à `http://127.0.0.1:*` et `http://localhost:*` (403 sinon), corps de requête ≤ 8 Mio. Adresse non locale : `--allow-remote` et `--token-file` exigés (jeton d'au moins 16 caractères sans espace, `Authorization: Bearer …` comparé en temps constant, 401 sinon ; un jeton donné en boucle locale est exigé aussi), `Host` élargi à l'IP donnée ; `0.0.0.0`/`::` refusés. Le serveur tourne jusqu'à l'arrêt du processus.

## 2. Options globales : « maintenant » et langue (`now.rs`)

- `--now 2026-10-19T08:00` > variable `PLANNERCRAFT_NOW` > horloge locale (tronquée à la minute). C'est la seule lecture d'horloge du binaire ; la valeur est injectée dans le `Workspace` (règle 5). Une valeur invalide de l'option ou de la variable est une erreur d'usage (2) ; une variable vide est ignorée ; une variable invalide est ignorée quand l'option est donnée.
- `--lang fr|en` > `LANG` (`en…` → anglais, `fr…` → français) > français. La langue fixe les messages, les textes de durée des vues et les libellés du rendu.

## 3. Codes de sortie (C-MCP-08, `exit.rs`)

| Code | Sens | Cas |
|---|---|---|
| 0 | succès | y compris `--help`, `--version`, `apply --dry-run` |
| 1 | commande refusée | erreur métier (`task.not_found`, `batch.would_create_errors`, `param.invalid`, lot JSON mal formé…) |
| 2 | usage | arguments (clap), date d'option invalide, `mcp --http` sans `--root`, adresse invalide ou non locale sans `--allow-remote`/`--token-file`, jeton trop court, serveur MCP indisponible |
| 3 | fichier | `file.*` (lecture, écriture, taille, MSPDI inexploitable, fichier tenu par l'application) et `csv.error.*` (CSV illisible) ; serveur MCP arrêté sur une erreur d'entrée/sortie ou une ligne trop longue (C-MCP-06) ; `--root` ou `--token-file` illisible, adresse HTTP déjà prise |

Erreur : `plannercraft : <code> : <message>` sur la sortie d'erreur (diagnostics R-160 en dessous) ; avec `--json`, l'erreur structurée du MCP `{"error": {code, message, op_index, field, subject, diagnostics}}` sur la sortie standard. Aucune écriture ne peut planter (sortie standard fermée, `plannercraft tasks plan.xml | head` : code 3 sans message).

## 4. Tests (`apps/plannercraft-cli/tests/cli.rs`)

Binaire lancé par `env!("CARGO_BIN_EXE_plannercraft")`, `PLANNERCRAFT_NOW` retiré et `LANG` fixé, fichiers dans un dossier temporaire propre au test.

- `cli_summary_house_json_equals_mcp_view` : `summary --json` et `tasks --json` (toutes pages, `--critical`, `--limit`) égaux aux vues de `Workspace::call` sur `house.xml`.
- `cli_apply_dry_run_leaves_file`, `cli_apply_writes_atomic` (`-o`, remplacement sans temporaire résiduel), `cli_apply_refused_exit_1` (erreur structurée, fichier intact).
- `cli_missing_file_exit_3` (absent, MSPDI illisible, lot absent, écriture impossible), `cli_render_svg_bytes_stable` (deux rendus identiques, égaux à `render.gantt`), `cli_now_env_and_flag_precedence`, `cli_usage_errors_exit_2`, `cli_explain_text`, `cli_explain_json` (J6), `cli_import_csv_roundtrip`, `cli_lang_flag_and_env`, `cli_style_list_get_apply` (J8 : JSON égal à `style.list`, `--dry-run` sans effet, instruction écrite, `--reset`), `cli_export_pdf` (J8, L7 : plan inchangé, mêmes octets à deux lancements, A3 paysage sur 2 pages en largeur en JSON, plage inversée code 1, `--pages-wide 0` code 2).
- `cli_resources_json` (`resources --json` égal à `resource.list`, `--overallocated`, texte, `tasks --resource`, ID inconnu), `cli_usage_by_week` (`usage --json` égal à `analysis.resource_usage`, plage par jour, texte, `--by` inconnu : code 2) : plan à ressources construit par `Workspace::call` puis enregistré.
- Tests unitaires : priorité de « maintenant » et lecture de `LANG` (`now.rs`), familles de codes (`exit.rs`).
