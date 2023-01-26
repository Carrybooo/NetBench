#!/bin/bash
#
# config.sh
#
# Description : 
# Ce script a pour but de paramétrer le fichier config.toml
#
# Auteurs : Walfroy BOUTIN ; Valentin GUERLESQUIN ; Ali JOUA.
# 
# Test : script bash fonctionnel.
#
# Date : 26 Janvier 2023.
#
# Contexte : ce script a été écrit pour le projet Université de Rennes 1 / ISTIC / Master 2 - Cloud et Réseaux, millésime 2022-2023, intitulé "Rédiger un mode d'emploi d’utilisation du simulateur NS-3 pour simuler des réseaux mobiles Ad-Hoc - utilisant le protocole OLSR - réalistes".
#
# Commentaire : ce projet implique donc de tester des configurations de noeuds en mode Ad-Hoc en réel, de les reproduire sur NS-3, puis d'ajuster les paramètres de NS-3 pour que le résultat de simulation s'approche au plus près de la réalité.
#
# Avertissement: ce script ne doit être modifié que si vous souhaitez rajouter un 5ème noeud ou plus dans le réseau AD-HOC. Il va chercher toutes ses variables dans le fichier config.toml
#
# Licence : free


# Je récupère dans config.toml les 4 adresses IPv4 qui y sont configurées en dur
ip1=`cat config.toml | grep "ip1" | cut -c7-14`
ip2=`cat config.toml | grep "ip2" | cut -c7-14`
ip3=`cat config.toml | grep "ip3" | cut -c7-14`
ip4=`cat config.toml | grep "ip4" | cut -c7-16`

# On choisit la machine locale
while true
do
    echo "Vous allez choisir quelle machine est le local"
    echo "1 pour être le local comme étant $ip1"
    echo "2 pour être le local comme étant $ip2"
    echo "3 pour être le local comme étant $ip3"
    echo "4 pour être le local comme étant $ip4"
    echo "Q pour Quitter"

    read -p "Sélection : " selection

    case $selection in
        1)  ipS=$ip1
            x="1"
            break
            ;;
        2)  ipS=$ip2
            x="2"
            break
            ;;
        3)  ipS=$ip3
            x="3"
            break
            ;;
        4)  ipS=$ip4
            x="4"
            break
            ;;
        Q)  echo "Au revoir!"
            exit
            ;;
        *)  echo "Sélection non valide, veuillez réessayer."
            ;;
    esac
done
echo "Vous choisissez d'être le local $x comme étant $ipS"
# Validation de l'adresse IP
while true; do
    read -p "Validez-vous cette sélection (O/N) ? " validate
    case $validate in
        [oO])
            echo "Vous êtes le local $x comme étant $ipS"
            break
            ;;
        [nN])
            echo "Veuillez faire une nouvelle sélection"
            break
            ;;
        *)
            echo "Réponse non valide, veuillez réessayer."
            ;;
    esac
done
# Changer dans le fichier config.toml le num_local
sed -i "s/num_local= .*/num_local= $x/" config.toml

# # On choisit la machine distante
while true
do
    echo "Vous allez choisir quelle machine est le distant"
    echo "1 pour être le distant comme étant $ip1"
    echo "2 pour être le distant comme étant $ip2"
    echo "3 pour être le distant comme étant $ip3"
    echo "4 pour être le distant comme étant $ip4"
    echo "Q pour Quitter"

    read -p "Sélection : " selection

    case $selection in
        1)  ipR=$ip1
            y="1"
            break
            ;;
        2)  ipR=$ip2
            y="2"
            break
            ;;
        3)  ipR=$ip3
            y="3"
            break
            ;;
        4)  ipR=$ip4
            y="4"
            break
            ;;
        Q)  echo "Au revoir!"
            exit
            ;;
        *)  echo "Sélection non valide, veuillez réessayer."
            ;;
    esac
done
echo "Vous choisissez d'être le distant $y comme étant $ipR"
# Validation de l'adresse IP
while true; do
    read -p "Validez-vous cette sélection (O/N) ? " validate
    case $validate in
        [oO])
            echo "Vous êtes le distant $y comme étant $ipR"
            break
            ;;
        [nN])
            echo "Veuillez faire une nouvelle sélection"
            break
            ;;
        *)
            echo "Réponse non valide, veuillez réessayer."
            ;;
    esac
done
# Changer dans le fichier config.toml le num_dist
sed -i "s/num_dist= .*/num_dist= $y/" config.toml

# Message final
echo " _______________________________________________________________"
echo "| Tout est configuré"
echo "|Le LOCAL est $x comme étant $ipS"
echo "|Le DISTANT est $y comme étant $ipR"
echo "|Il vous reste à lancer le fichier ./sender.sh ou ./receiver.sh"
echo "|_______________________________________________________________"
