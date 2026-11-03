PlannerCraft pour Windows (x86_64)

- plannercraft-desktop.exe : l'application (double-clic).
- plannercraft.exe : la ligne de commande et le serveur MCP (plannercraft mcp), à lancer
  depuis un terminal.

Les binaires ne sont pas signés : au premier lancement, Microsoft Defender
SmartScreen affiche « Windows a protégé votre ordinateur ». Cliquer sur
« Informations complémentaires », puis sur « Exécuter quand même ».

plannercraft-desktop.exe --version n'affiche rien dans un terminal (application
fenêtrée) ; utiliser plannercraft.exe --version, ou plannercraft-desktop.exe --version | more.

Guide complet (empreintes SHA256SUMS comprises) : docs/user/install.md du
dépôt, https://github.com/DVP-Th30/plannercraft
