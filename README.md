## NETBENCH

Cette partie du projet est un outil de benchmark que nous avons développé pour mesurer plusieurs caractéristiques réseau, à savoir:

- Le débit moyen
- Le Packet Delivery Ratio (grâce au nombre de paquets transmis et au nombre de packets reçus)
- La latence moyenne
- X Les variations de latence (pas fait encore)
- La route utilisée par les paquets

Cet outil comporte plusieurs parties : 

- Un reader qui a pour but d'aller lire un fichier de config (nommé config.toml) et de le parser pour récupérer les addresses et ports nécessaires au benchmark.

- Un dumper qui va écrire dans le dossier ```data``` un fichier CSV comprenant des infos sur les paquets envoyés/reçus pendant l'utilisation du script.

- Un receiver ayant pour tầche d'ouvrir un socket TCP et de compter les paquets reçus et de pouvoir renvoyer périodiquement ce nombre.

- Un sender, qui s'occupe d'envoyer les paquets et de faire les mesures. l'exécution de ce dernier est divisée en 5 threads :
  
  1. Un thread **sender** qui a pour but d'envoyer des packets IP bruts en boucle à une addresse distante. la taille des packets et le débit est variable selon les données passées en entrée du script.
     Ce thread s'occupe également de stocker dans un arbre binaire le numéro de séquence du packet envoyé, ainsi que le timestamp de cet envoi et la taille du paquet.
  
  2. Un thread **compute** ayant pour tâche de demander périodiquement au receveur de lui envoyer des données, de s'occuper des acquitements et des calculs à effectuer avec ces dernières, avant de print périodiquement les résultats.
  
  3. Un thread **icmp_ping** qui s'occupe d'envoyer toutes les 200ms un ping à l'adresse cible et mesure le temps d'aller/retour pour déterminer la latence moyenne.
  
  4. Un thread **icmp_route** qui détermine la route utilisée jusqu'à l'adresse cible en envoyant des requêtes d'écho ICMP avec un TTL croissant, et en examinant les réponses "TTL exceeded" pour retrouver les adresses des noeuds sur le chemin.
  
  5. Et finalement un thread **sync** qui sert à synchroniser les affichages des différents threads, car nous avons implémenté, en plus de récupérer les données à la fin, un affichage périodique des valeurs, pour avoir un aperçu en temps réel des statistiques de la connection. Les threads gardent donc en mémoire des stats "partielles" qui séparent chaque print. 


Le thread **sender** utilisant des paquets IP bruts, les différents types de payloads utilisables sont situés dans la librairie ```utils```. Ils utilisent pour l'instant tous la même structure par soucis de facilité pour la sérialisation/désérialisation en octets, mais il est possible d'ajouter des types et formats utilisables.

Le numéro de protocole utilisé est 254. C'est un numéro réservé aux tests.



Cet outil est programmé dans le langage Rust car pour mesurer certaines caractéristiques il faut pouvoir utiliser des "raw sockets", il nous fallait donc un language bas niveau. Le Rust a des performances similaires au C mais est plus sûr dans sa gestion de la mémoire et fournit quelques abstractions assez utiles. Il était donc tout indiqué pour cette tầche. 

### Utilisation :

- Installer git, puis cargo et rustc (le plus simple c'est en utilisant [Rustup](https://rustup.rs/))
- Cloner le [repo](https://github.com/Carrybooo/GPROJ/) et se positionner dans le dossier netbench
- Génerer les binaires netperf avec cargo en utilisant ```cargo build```
- Remplir le fichier config.toml avec les IP des machines (prévu pour 4 ici car on n'en utilise que 4 mais c'est modifiable assez facilement)
- Utiliser le script ```config.sh``` pour sélectionner le numéro de machine parmis les différents addresses.
- Lancer le receiver sur la machine qui va recevoir le traffic
  - Utiliser le script prévu à cet effet : ```sender.sh```
- Lancer le sender sur la machine qui va transmettre les données
  - Utiliser le script prévu à cet effet : `receiver.sh`
- (Pour lancer les deux parties du programme, il est nécessaire que le dossier courant contienne bien le fichier config.toml).
- Une fois les 2 parties lancées, des logs doivent apparaître régulièrement sur le sender et le receiver.
- Pour arrêter le script, il faut arrêter le sender en 1er, simplement via un **CTRL+C** sur le sender qui déclenche le print de fin. Vous pouvez ensuite arrêter le receiver aussi.
