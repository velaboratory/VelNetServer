#!/bin/bash

BASEDIR=$(dirname "$0")
echo $(date +"%Y-%m-%dT%T.%3N%z") >> $BASEDIR/restarts.log
echo "Before: " >> $BASEDIR/restarts.log
pgrep -f "VelNetServerRust -v2" >> $BASEDIR/restarts.log
#pgrep -f "VelNetServerRust -v2" | xargs sudo kill >> $BASEDIRE/restarts.log
#kill $(pgrep -f "VelNetServerRust -v2")
pkill -f "VelNetServerRust -v2" 
echo "\nAfter: " >> $BASEDIR/restarts.log
pgrep -f "VelNetServerRust -v2" >> $BASEDIR/restarts.log
echo "\n"
(cd $BASEDIR && nohup ./target/release/VelNetServerRust -v2 >> nohup.out &)
# nohup "$BASEDIR/target/release/VelNetServerRust" &
echo "Starting..." >> $BASEDIR/restarts.log
