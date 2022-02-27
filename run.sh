BASEDIR=$(dirname "$0")
# pushd $BASEDIR
echo $(date +"%Y-%m-%dT%T.%3N%z") >> $BASEDIR/restarts.log
echo "Before: "
pgrep VelNetServer
pkill VelNetServer
echo "\nAfter: "
pgrep VelNetServer
echo "\n"
(cd $BASEDIR && nohup "./target/release/VelNetServerRust" &)
# nohup "$BASEDIR/target/release/VelNetServerRust" &
echo "Starting..." >> $BASEDIR/restarts.log
# popd $BASEDIR