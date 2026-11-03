# Oracle MS Project

Coquille pour le jour où une licence MS Project sera disponible (ADR-0004). Elle permettra de passer des règles du statut `hypothèse` à `vérifié contre MS Project`.

## Format

```
tests/oracle/
├── input/<cas>.xml       # planning MSPDI soumis à MS Project
└── msproject/<cas>.xml   # le même planning recalculé et exporté par MS Project
```

Les fichiers d'entrée sont écrits pour PlannerCraft (versionnables) ; les sorties sont produites par MS Project et versionnées avec eux, avec la version de MS Project relevée dans `msproject/oracle.log`.

## Générer les sorties

1. Copier `input/` sur une machine Windows avec MS Project Desktop.
2. Dans MS Project : Affichage > Macros > Visual Basic, puis Fichier > Importer un fichier… > [`tools/oracle/generate.bas`](../../tools/oracle/generate.bas).
3. Exécuter la macro `GenerateOracle` (elle demande les dossiers d'entrée et de sortie) ou, dans la fenêtre Exécution : `RunOracle "C:\...\input", "C:\...\msproject"`.
4. Relire `msproject/oracle.log` (fichiers en erreur), puis rapatrier `msproject/`.

## Harnais

`cargo test -p plannercraft-tests --test oracle` découvre les paires `input/` ↔ `msproject/`. En J1 il passe sans cas et **échoue s'il trouve un cas**, tant que la comparaison n'existe pas (lecture MSPDI en J3) : un cas déposé ne doit jamais passer en silence. Le harnais complet recalculera chaque entrée avec le moteur, comparera les dates champ par champ et produira un rapport par cas.
