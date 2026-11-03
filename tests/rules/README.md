# Tests des règles de calcul

Chaque règle `R-xxx` de [`docs/scheduling-rules.md`](../../docs/scheduling-rules.md) a au moins un test ici (règle 4 de `CLAUDE.md`), ajouté dans le même commit que la règle.

## Convention

- **Un fichier par règle** : `rNNN_titre_court.rs`, en minuscules ASCII, chiffres et `_` (exemple : `r001_fs_link_lag.rs` pour `R-001`).
- Le fichier est déclaré dans `main.rs` par `mod rNNN_titre_court;`. Un test de `main.rs` vérifie le nommage et la déclaration : un fichier oublié ferait taire ses tests.
- Chaque test porte l'identifiant dans son nom : `fn r001_fs_lag_counts_working_days()`.
- Les valeurs attendues sont **calculées à la main et commentées** dans le test (calendrier utilisé, détail du calcul). Jamais de valeur recopiée depuis la sortie du moteur.
- Une règle au statut `hypothèse` le reste tant qu'aucun fichier recalculé par MS Project (oracle ou corpus) ne la confirme.

Lancer : `cargo test -p plannercraft-tests --test rules`.
