#!/bin/bash

echo "Building VelNet Server..."
cargo build --release
(
  cd control-panel || exit
  echo "Building VelNet Control Panel..."
  cargo build --release
)
echo "Copying systemd configs..."

(
  sudo cp velnet.service /etc/systemd/system/
  service="/etc/systemd/system/velnet.service"
  wd="$(pwd)"
  exe="$(pwd)/target/release/velnet_server"
  sudo sed -i -e "s+WorkingDirectory=+WorkingDirectory=$wd+g" "$service"
  sudo sed -i -e "s+ExecStart=+ExecStart=$exe+g" "$service"
  systemctl enable velnet
  systemctl restart velnet
)

(
  cd control-panel || exit
  sudo cp ../velnet.service /etc/systemd/system/velnet-control-panel.service
  service="/etc/systemd/system/velnet-control-panel.service"
  wd="$(pwd)"
  exe="$(pwd)/target/release/velnet_control_panel"
  sudo sed -i -e "s+WorkingDirectory=+WorkingDirectory=$wd+g" "$service"
  sudo sed -i -e "s+ExecStart=+ExecStart=$exe+g" "$service"
  systemctl enable velnet-control-panel
  systemctl restart velnet-control-panel
)

echo "'velnet' and 'velnet-control-panel' systemd services enabled."
