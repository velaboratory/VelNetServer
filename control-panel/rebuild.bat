docker stop velnet_control_panel
docker build -t velaboratory/velnet_control_panel .
docker rm velnet_control_panel
docker run -d -p 8080:8080 --name velnet_control_panel velaboratory/velnet_control_panel