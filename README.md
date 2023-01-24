## NETPERF

Cette partie du projet est un outil de benchmark que nous avons développé pour mesurer plusieurs caractéristiques réseau, à savoir:

- Le débit moyen
- Le Packet Drop Ratio (grâce au nombre de paquets transmis et au nombre de packets reçus)
- La latence moyenne
- X Les variations de latence
- La route utilisée par les paquets



Cet outil comporte 3 parties : 

- Un reader qui a pour but d'aller lire un fichier de config (nommé config.toml) et de le parser pour récupérer les addresses et ports nécessaires au benchmark.

- Un receiver ayant pour tầche d'ouvrir un socket TCP et de compter les paquets reçus et de pouvoir renvoyer périodiquement ce nombre.

- Un sender, qui s'occupe d'envoyer les paquets et de faire les mesures. l'exécution de ce dernier est divisée en 4 threads:
  
  - Un thread **tcp_connection** qui va ouvrir une connection TCP jusqu'au receiver et la saturer pour déterminer le débit max. Il garde le compte du nombre de paquets envoyés.
    Il envoie également périodiquement des messages "update" pour que le receiver lui renvoie son compte actuel de paquets pour pouvoir calculer le drop ratio à un instant T.
  
  - Un thread **icmp_ping** qui s'occupe d'envoyer toutes les 200ms un ping à l'adresse cible et mesure le temps d'aller/retour pour déterminer la latence.
  
  - Un thread **icmp_route** qui détermine la route utilisée jusqu'à l'adresse cible en envoyant des requêtes d'écho ICMP avec un TTL croissant, et en examinant les réponses "TTL exceeded" pour retrouver les adresses des noeuds sur le chemin.
  
  - Et finalement un thread **sync** qui sert à synchroniser les affichages des différents threads, car nous avons implémenté, en plus de récupérer les données à la fin, un affichage périodique des valeurs, pour avoir un aperçu en temps réel des statistiques de la connection. Les threads gardent donc tous en mémoire des stats "partielles" qui séparent chaque print.



Cet outil est programmé dans le langage Rust car pour mesurer certaines caractéristiques il faut pouvoir utiliser des "raw sockets", il nous fallait donc un language bas niveau. Le Rust a des performances similaires au C mais est plus sûr dans sa gestion de la mémoire et fournit quelques abstractions assez utiles. Il était donc tout indiqué pour cette tầche. 


### Utilisation :

- Installer git, puis cargo et rustc (le plus simple c'est en utilisant [Rustup](https://rustup.rs/))
- Cloner le repo et se positionner dans le dossier netperf
- Génerer les binaires netperf avec cargo en utilisant ```cargo build```
- Remplir le fichier config.toml avec les IP des machines (prévu pour 4 ici car on n'en utilise que 4 mais c'est modifiable assez facilement)
- Choisir pour chaque machine les champs "num_local" et "num_dist" pour le numéro d'addresse locale et celui de l'addresse distante avec laquelle elle va communiquer. (Ce ne sont ni plus ni moins que des selecteurs).
- Lancer le receiver sur la machine qui va recevoir le traffic
  - Peut se faire avec ```cargo run --bin receiver``` ou en lançant l'executable à la main via ```./target/debug/receiver``` (en supposant que vous l'ayez laissé là après la compilation)
- Lancer le sender sur la machine qui va transmettre les données
  - ***Attention !*** Le sender nécessite les droits administrateurs pour créer certains sockets ICMP, il faut donc le lancer avec ```sudo ./target/debug/sender```
- (Pour lancer les deux parties du programme, il est nécessaire que le dossier courant contienne bien le fichier config.toml).
- Une fois les 2 parties lancées, des logs doivent apparaître régulièrement sur le sender et le receiver.
- Pour arrêter le script, il faut arrêter le sender en 1er, simplement via un **CTRL+C** sur le sender qui déclenche le print de fin. Vous pouvez ensuite arrêter le receiver aussi.

